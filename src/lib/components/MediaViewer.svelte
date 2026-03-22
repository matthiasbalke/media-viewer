<script lang="ts">
    import { convertFileSrc, invoke } from "@tauri-apps/api/core";
    import Filmstrip from "./Filmstrip.svelte";
    import { settingsStore } from "$lib/stores/settings.svelte";

    interface MediaFile {
        name: string;
        path: string;
        isVideo: boolean;
        thumbnailState:
            | "loading"
            | "ready"
            | "error"
            | "unsupported"
            | "frontend-render";
        thumbnailSrc: string | null;
    }

    interface Props {
        file: MediaFile;
        files: MediaFile[];
        onClose?: () => void;
        onFileChange?: (file: MediaFile) => void;
    }

    let { file, files, onClose, onFileChange }: Props = $props();

    // WebKit-supported image formats that can be displayed natively
    const WEBVIEW_SUPPORTED = [
        "jpg",
        "jpeg",
        "png",
        "gif",
        "webp",
        "svg",
        "avif",
        "bmp",
        "ico",
        "heic",
        "heif",
    ];

    function getExtension(filename: string): string {
        const parts = filename.split(".");
        return parts.length > 1 ? parts[parts.length - 1].toLowerCase() : "";
    }

    function isWebViewSupported(filename: string): boolean {
        return WEBVIEW_SUPPORTED.includes(getExtension(filename));
    }

    let isPaused = $state(true);
    let currentTime = $state(0);
    let duration = $state(0);
    let volume = $state(1);
    let muted = $state(false);

    let compatibleSrc = $state<string | null>(null);
    let convertingPreview = $state(false);

    $effect(() => {
        // Reset when navigating to a different file
        file.path;
        compatibleSrc = null;
        convertingPreview = false;
    });

    async function handleImageError() {
        if (compatibleSrc !== null || convertingPreview || !settingsStore.cacheBaseDir) return;
        convertingPreview = true;
        try {
            const p = await invoke<string>("get_image_preview", {
                path: file.path,
                cacheBaseDir: settingsStore.cacheBaseDir,
            });
            compatibleSrc = convertFileSrc(p);
        } catch {
            compatibleSrc = ""; // mark as failed so we don't retry
        } finally {
            convertingPreview = false;
        }
    }

    async function handleVideoError() {
        if (compatibleSrc !== null || convertingPreview || !settingsStore.cacheBaseDir) return;
        convertingPreview = true;
        try {
            const p = await invoke<string>("get_video_preview", {
                path: file.path,
                cacheBaseDir: settingsStore.cacheBaseDir,
            });
            compatibleSrc = convertFileSrc(p);
        } catch {
            compatibleSrc = "";
        } finally {
            convertingPreview = false;
        }
    }

    function togglePlay(e?: Event) {
        if (e) e.stopPropagation();
        isPaused = !isPaused;
    }

    function toggleMute(e?: Event) {
        if (e) e.stopPropagation();
        muted = !muted;
    }

    function formatTime(seconds: number, totalDuration: number): string {
        if (!seconds || isNaN(seconds)) seconds = 0;

        const h = Math.floor(seconds / 3600);
        const m = Math.floor((seconds % 3600) / 60);
        const s = Math.floor(seconds % 60);

        // Always show hours if the total duration is >= 1 hour
        if (totalDuration >= 3600) {
            return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
        }

        return `${m}:${s.toString().padStart(2, "0")}`;
    }

    let mediaSrc = $derived(
        isWebViewSupported(file.name) || file.isVideo
            ? convertFileSrc(file.path)
            : null,
    );

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === "Escape") {
            onClose?.();
            return;
        }

        if (event.key === " " && file.isVideo) {
            event.preventDefault();
            togglePlay();
            return;
        }

        // Navigation and Seeking
        if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
            if (event.shiftKey && file.isVideo) {
                event.preventDefault();
                const seekAmount = 5; // seconds
                if (event.key === "ArrowRight") {
                    currentTime = Math.min(currentTime + seekAmount, duration);
                } else {
                    currentTime = Math.max(currentTime - seekAmount, 0);
                }
                return;
            }

            const currentIndex = files.findIndex((f) => f.path === file.path);
            if (currentIndex === -1) return;

            if (event.key === "ArrowRight" && currentIndex < files.length - 1) {
                onFileChange?.(files[currentIndex + 1]);
            } else if (event.key === "ArrowLeft" && currentIndex > 0) {
                onFileChange?.(files[currentIndex - 1]);
            }
        }
    }

    function handleFilmstripSelect(selected: MediaFile) {
        onFileChange?.(selected);
    }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="flex flex-col h-full bg-zinc-950">
    <div
        class="flex-1 flex items-center justify-center relative overflow-hidden min-h-0"
    >
        <!-- Close button -->
        <button
            class="absolute top-3 left-3 z-10 w-9 h-9 flex items-center justify-center bg-zinc-900/70 backdrop-blur-md border border-zinc-700/50 rounded-lg text-zinc-400 cursor-pointer p-0 transition-colors duration-150 hover:text-white hover:bg-zinc-800/90"
            onclick={() => onClose?.()}
            title="Back to grid (Esc)"
        >
            <svg
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                class="w-[18px] h-[18px]"
            >
                <path d="M19 12H5M12 19l-7-7 7-7" />
            </svg>
        </button>

        {#if mediaSrc}
            {#if file.isVideo}
                {#if convertingPreview}
                    <div class="flex flex-col items-center gap-3 text-zinc-500">
                        <p class="text-sm m-0">Converting for playback…</p>
                    </div>
                {:else}
                <!-- svelte-ignore a11y_media_has_caption -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <video
                    src={compatibleSrc || mediaSrc}
                    class="max-w-full max-h-full object-contain rounded outline-none cursor-pointer"
                    bind:paused={isPaused}
                    bind:currentTime
                    bind:duration
                    bind:volume
                    bind:muted
                    onclick={togglePlay}
                    onerror={handleVideoError}
                ></video>
                {/if}

                <!-- Transport Controls -->
                <div
                    class="absolute bottom-6 left-1/2 -translate-x-1/2 flex items-center gap-4 py-2 px-4 bg-zinc-900/80 backdrop-blur-md border border-zinc-700/50 rounded-xl max-w-xl w-[calc(100%-2rem)] shadow-2xl z-20"
                >
                    <button
                        class="w-8 h-8 shrink-0 flex items-center justify-center rounded-full hover:bg-zinc-700 text-zinc-300 transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500/50"
                        onclick={togglePlay}
                        title={isPaused ? "Play (Space)" : "Pause (Space)"}
                    >
                        {#if isPaused}
                            <!-- Play Icon -->
                            <svg
                                viewBox="0 0 24 24"
                                fill="currentColor"
                                class="w-5 h-5 ml-1"
                            >
                                <path d="M8 5v14l11-7z" />
                            </svg>
                        {:else}
                            <!-- Pause Icon -->
                            <svg
                                viewBox="0 0 24 24"
                                fill="currentColor"
                                class="w-5 h-5"
                            >
                                <path d="M6 19h4V5H6v14zm8-14v14h4V5h-4z" />
                            </svg>
                        {/if}
                    </button>

                    <span
                        class="text-xs font-medium text-zinc-400 font-mono tracking-wider w-14 text-center shrink-0"
                    >
                        {formatTime(currentTime, duration)}
                    </span>

                    <input
                        type="range"
                        min="0"
                        max={duration || 1}
                        step="0.01"
                        bind:value={currentTime}
                        class="flex-1 h-1.5 bg-zinc-700 rounded-lg appearance-none cursor-pointer accent-blue-500 hover:accent-blue-400 focus:outline-none focus:ring-2 focus:ring-blue-500/50"
                        onpointerdown={(e) => e.stopPropagation()}
                    />

                    <span
                        class="text-xs font-medium text-zinc-500 font-mono tracking-wider w-14 text-center shrink-0"
                    >
                        {formatTime(duration, duration)}
                    </span>

                    <div class="flex items-center gap-2 group w-24 shrink-0">
                        <button
                            class="w-8 h-8 shrink-0 flex items-center justify-center rounded-full hover:bg-zinc-700 text-zinc-400 hover:text-zinc-200 transition-colors focus:outline-none focus:ring-2 focus:ring-blue-500/50"
                            onclick={toggleMute}
                            title={muted ? "Unmute" : "Mute"}
                        >
                            {#if muted || volume === 0}
                                <!-- Muted Icon -->
                                <svg
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    class="w-4 h-4"
                                >
                                    <path
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                        d="M5.586 15H4a1 1 0 01-1-1v-4a1 1 0 011-1h1.586l4.707-4.707C10.923 3.663 12 4.109 12 5v14c0 .891-1.077 1.337-1.707.707L5.586 15z"
                                    />
                                    <path
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                        d="M17 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2"
                                    />
                                </svg>
                            {:else if volume < 0.5}
                                <!-- Volume Low Icon -->
                                <svg
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    class="w-4 h-4"
                                >
                                    <path
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                        d="M5.586 15H4a1 1 0 01-1-1v-4a1 1 0 011-1h1.586l4.707-4.707C10.923 3.663 12 4.109 12 5v14c0 .891-1.077 1.337-1.707.707L5.586 15z"
                                    />
                                    <path
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                        d="M15.536 8.464a5 5 0 010 7.072"
                                    />
                                </svg>
                            {:else}
                                <!-- Volume High Icon -->
                                <svg
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    class="w-4 h-4"
                                >
                                    <path
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                        d="M5.586 15H4a1 1 0 01-1-1v-4a1 1 0 011-1h1.586l4.707-4.707C10.923 3.663 12 4.109 12 5v14c0 .891-1.077 1.337-1.707.707L5.586 15z"
                                    />
                                    <path
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                        d="M15.536 8.464a5 5 0 010 7.072"
                                    />
                                    <path
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                        d="M19.071 4.929a10 10 0 010 14.142"
                                    />
                                </svg>
                            {/if}
                        </button>
                        <input
                            type="range"
                            min="0"
                            max="1"
                            step="0.05"
                            bind:value={volume}
                            class="w-full h-1.5 bg-zinc-700 rounded-lg appearance-none cursor-pointer accent-blue-500 hover:accent-blue-400 focus:outline-none focus:ring-2 focus:ring-blue-500/50 opacity-0 group-hover:opacity-100 transition-opacity"
                            onpointerdown={(e) => e.stopPropagation()}
                        />
                    </div>
                </div>
            {:else}
                {#if convertingPreview}
                    <div class="flex flex-col items-center gap-3 text-zinc-500">
                        <p class="text-sm m-0">Converting…</p>
                    </div>
                {:else}
                    <img
                        src={compatibleSrc || mediaSrc}
                        alt={file.name}
                        class="max-w-full max-h-full object-contain rounded shadow-2xl"
                        onerror={handleImageError}
                    />
                {/if}
            {/if}
        {:else}
            <div class="flex flex-col items-center gap-3 text-zinc-500">
                <span class="text-5xl">🖼️</span>
                <p class="text-sm m-0">
                    Preview not available for .{getExtension(file.name)} files
                </p>
            </div>
        {/if}

        <!-- File name -->
        <div
            class="absolute top-3 left-1/2 -translate-x-1/2 px-4 py-1.5 bg-zinc-900/70 backdrop-blur-md border border-zinc-700/50 rounded-lg text-sm font-medium text-zinc-300 whitespace-nowrap max-w-[60%] overflow-hidden text-ellipsis z-10 shadow-lg"
        >
            {file.name}
        </div>
    </div>

    <Filmstrip
        {files}
        selectedPath={file.path}
        onSelect={handleFilmstripSelect}
    />
</div>
