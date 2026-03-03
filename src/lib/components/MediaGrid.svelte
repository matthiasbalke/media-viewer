<script lang="ts">
    import { readDir, stat } from "@tauri-apps/plugin-fs";
    import { invoke } from "@tauri-apps/api/core";
    import { convertFileSrc } from "@tauri-apps/api/core";
    import { listen } from "@tauri-apps/api/event";
    import { onDestroy } from "svelte";
    import { settingsStore } from "$lib/stores/settings.svelte";

    interface MediaFile {
        name: string;
        path: string;
        isVideo: boolean;
        thumbnailState: ThumbnailState;
        thumbnailSrc: string | null;
    }

    interface Props {
        path: string | null;
        thumbnailSize?: number;
        itemCount?: number;
        mediaFiles?: MediaFile[];
        onImageOpen?: (file: MediaFile) => void;
    }

    let {
        path,
        thumbnailSize = 128,
        itemCount = $bindable(0),
        mediaFiles = $bindable([]),
        onImageOpen,
    }: Props = $props();

    // Media file extensions (used to filter directory entries)
    const IMAGE_EXTENSIONS = [
        "jpg",
        "jpeg",
        "png",
        "gif",
        "webp",
        "bmp",
        "svg",
        "ico",
        "avif",
        "cr2",
        "heic",
        "heif",
    ];
    const VIDEO_EXTENSIONS = [
        "mp4",
        "webm",
        "mkv",
        "avi",
        "mov",
        "wmv",
        "flv",
        "m4v",
    ];

    type ThumbnailState = "loading" | "ready" | "error" | "unsupported";

    interface ThumbnailUpdate {
        path: string;
        status: ThumbnailState;
        thumbnailPath: string | null;
        sessionId: number;
    }

    let files: MediaFile[] = $state([]);
    let loading = $state(false);
    let error: string | null = $state(null);
    let currentSessionId: number | null = $state(null);
    let nextSessionId = 0;

    let selectedIndex = $state(0);
    let itemRefs: HTMLElement[] = [];

    // Event listener cleanup
    let unlistenFn: (() => void) | null = null;

    function getExtension(filename: string): string {
        const parts = filename.split(".");
        return parts.length > 1 ? parts[parts.length - 1].toLowerCase() : "";
    }

    function isImageFile(filename: string): boolean {
        return IMAGE_EXTENSIONS.includes(getExtension(filename));
    }

    function isVideoFile(filename: string): boolean {
        return VIDEO_EXTENSIONS.includes(getExtension(filename));
    }

    function isMediaFile(filename: string): boolean {
        return isImageFile(filename) || isVideoFile(filename);
    }

    async function setupListener() {
        // Clean up previous listener
        if (unlistenFn) {
            unlistenFn();
            unlistenFn = null;
        }

        unlistenFn = await listen<ThumbnailUpdate>(
            "thumbnail-update",
            (event) => {
                const update = event.payload;

                // Ignore events from a different session
                if (update.sessionId !== currentSessionId) return;

                const index = files.findIndex((f) => f.path === update.path);
                if (index === -1) return;

                files[index].thumbnailState = update.status;
                if (update.status === "ready" && update.thumbnailPath) {
                    files[index].thumbnailSrc = convertFileSrc(
                        update.thumbnailPath,
                    );
                }
            },
        );
    }

    async function loadMedia(dirPath: string) {
        try {
            loading = true;
            error = null;
            files = [];

            // Generate a new session ID
            currentSessionId = nextSessionId++;

            const entries = await readDir(dirPath);
            const mediaFiles: MediaFile[] = [];

            for (const entry of entries) {
                if (isMediaFile(entry.name)) {
                    const fullPath = `${dirPath}/${entry.name}`;
                    try {
                        const info = await stat(fullPath);
                        if (!info.isDirectory) {
                            mediaFiles.push({
                                name: entry.name,
                                path: fullPath,
                                isVideo: isVideoFile(entry.name),
                                thumbnailState: "loading",
                                thumbnailSrc: null,
                            });
                        }
                    } catch {
                        // Skip files we can't stat
                    }
                }
            }

            // Sort by name
            files = mediaFiles.sort((a, b) => a.name.localeCompare(b.name));
            loading = false;
            selectedIndex = 0;

            // Set up event listener before triggering generation
            await setupListener();

            // Trigger background thumbnail generation
            try {
                // cacheBaseDir should be loaded by now due to $effect wait
                if (settingsStore.cacheBaseDir) {
                    await invoke("generate_thumbnails", {
                        dir: dirPath,
                        sessionId: currentSessionId,
                        cacheBaseDir: settingsStore.cacheBaseDir,
                    });
                } else {
                    console.error(
                        "Settings not ready, cannot generate thumbnails.",
                    );
                }
            } catch (e) {
                console.error("Thumbnail generation failed:", e);
            }
        } catch (e) {
            error = e instanceof Error ? e.message : "Failed to load media";
            loading = false;
        }
    }

    // Reload when path changes or settings are ready
    $effect(() => {
        if (path && settingsStore.ready && settingsStore.cacheBaseDir) {
            loadMedia(path);
        } else if (!path) {
            files = [];
            currentSessionId = null;
            selectedIndex = 0;
        }
    });

    // Cleanup listener on component destroy
    onDestroy(() => {
        if (unlistenFn) {
            unlistenFn();
            unlistenFn = null;
        }
    });

    // Sync item count and files for parent
    $effect(() => {
        itemCount = files.length;
        mediaFiles = files;
    });

    function handleKeydown(event: KeyboardEvent) {
        if (!files.length) return;

        // Ignore if focus is in an input field
        const activeNode = document.activeElement?.nodeName;
        if (activeNode === "INPUT" || activeNode === "TEXTAREA") return;

        let newIndex = selectedIndex;
        // Determine columns
        let cols = 1;
        if (itemRefs.length > 1 && itemRefs[0] && itemRefs[1]) {
            const firstY = itemRefs[0].offsetTop;
            for (let i = 1; i < itemRefs.length; i++) {
                if (itemRefs[i] && itemRefs[i].offsetTop > firstY) {
                    cols = i;
                    break;
                }
            }
            if (
                cols === 1 &&
                itemRefs[itemRefs.length - 1] &&
                itemRefs[itemRefs.length - 1].offsetTop === firstY
            ) {
                cols = itemRefs.length;
            }
        }

        let handled = false;

        switch (event.key) {
            case "ArrowRight":
                if (newIndex < files.length - 1) newIndex++;
                handled = true;
                break;
            case "ArrowLeft":
                if (newIndex > 0) newIndex--;
                handled = true;
                break;
            case "ArrowDown":
                if (newIndex + cols < files.length) {
                    newIndex += cols;
                } else {
                    newIndex = files.length - 1;
                }
                handled = true;
                break;
            case "ArrowUp":
                if (newIndex - cols >= 0) {
                    newIndex -= cols;
                } else {
                    newIndex = 0;
                }
                handled = true;
                break;
            case "PageDown":
                if (newIndex + cols * 5 < files.length) {
                    newIndex += cols * 5;
                } else {
                    newIndex = files.length - 1;
                }
                handled = true;
                break;
            case "PageUp":
                if (newIndex - cols * 5 >= 0) {
                    newIndex -= cols * 5;
                } else {
                    newIndex = 0;
                }
                handled = true;
                break;
            case "Enter":
                if (onImageOpen && files[selectedIndex]) {
                    onImageOpen(files[selectedIndex]);
                }
                handled = true;
                break;
        }

        if (handled) {
            event.preventDefault();
        }

        if (
            newIndex !== selectedIndex &&
            newIndex >= 0 &&
            newIndex < files.length
        ) {
            selectedIndex = newIndex;
            // Scroll into view
            if (itemRefs[selectedIndex]) {
                itemRefs[selectedIndex].scrollIntoView({
                    block: "nearest",
                    behavior: "smooth",
                });
            }
        }
    }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="p-4">
    {#if !path}
        <div class="flex items-center justify-center h-full text-zinc-500">
            <p>Select a folder to view media</p>
        </div>
    {:else if loading}
        <div class="flex items-center justify-center h-32 text-zinc-500">
            <p>Loading...</p>
        </div>
    {:else if error}
        <div class="text-red-400 p-4">{error}</div>
    {:else if files.length === 0}
        <div class="flex items-center justify-center h-32 text-zinc-500">
            <p>No media files in this folder</p>
        </div>
    {:else}
        <div
            class="grid gap-4"
            style="grid-template-columns: repeat(auto-fill, minmax({thumbnailSize}px, 1fr));"
        >
            {#each files as file, i}
                <div
                    bind:this={itemRefs[i]}
                    class="group relative rounded-lg overflow-hidden hover:ring-2 hover:ring-blue-500 focus:outline-none transition-all cursor-pointer {i ===
                    selectedIndex
                        ? 'ring-2 ring-blue-500 ring-offset-2 ring-offset-zinc-900 z-10'
                        : ''}"
                    class:bg-zinc-800={file.thumbnailState !== "ready"}
                    class:bg-transparent={file.thumbnailState === "ready"}
                    style="height: {thumbnailSize}px;"
                    role="button"
                    tabindex="0"
                    onclick={() => (selectedIndex = i)}
                    ondblclick={() => onImageOpen?.(file)}
                >
                    {#if file.thumbnailState === "loading"}
                        <!-- Loading spinner -->
                        <div
                            class="absolute inset-0 flex items-center justify-center bg-zinc-900"
                        >
                            <div
                                class="animate-spin rounded-full h-8 w-8 border-2 border-zinc-600 border-t-zinc-300"
                            ></div>
                        </div>
                    {:else if file.thumbnailState === "error"}
                        <!-- Error placeholder -->
                        <div
                            class="absolute inset-0 flex items-center justify-center bg-zinc-900"
                        >
                            <span class="text-4xl text-red-400">✕</span>
                        </div>
                    {:else if file.thumbnailState === "unsupported"}
                        <!-- Unsupported format placeholder -->
                        <div
                            class="absolute inset-0 flex items-center justify-center bg-zinc-900"
                        >
                            <span class="text-4xl text-zinc-500">?</span>
                        </div>
                    {:else if file.thumbnailSrc}
                        <!-- Thumbnail ready -->
                        <img
                            src={file.thumbnailSrc}
                            alt={file.name}
                            class="w-full h-full object-scale-down"
                        />
                    {/if}

                    <!-- File name overlay -->
                    <div
                        class="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/80 to-transparent p-2 {file.thumbnailState ===
                        'ready'
                            ? 'opacity-0 group-hover:opacity-100'
                            : ''} transition-opacity"
                    >
                        <p class="text-xs text-zinc-300 truncate">
                            {file.name}
                        </p>
                    </div>
                </div>
            {/each}
        </div>
    {/if}
</div>
