<script lang="ts">
    import { fade, scale } from "svelte/transition";
    import { invoke } from "@tauri-apps/api/core";
    import { open as openDialog } from "@tauri-apps/plugin-dialog";
    import { openPath } from "@tauri-apps/plugin-opener";
    import { settingsStore } from "$lib/stores/settings.svelte";

    let { onClose } = $props<{ onClose: () => void }>();

    let isCleaning = $state(false);
    let cleanMessage = $state("");

    async function handleChangeCacheDir() {
        const selected = await openDialog({
            directory: true,
            multiple: false,
            title: "Select Thumbnail Cache Directory",
        });

        if (selected && typeof selected === "string") {
            const normalized = selected.replace(/\\/g, "/");
            await settingsStore.setCacheBaseDir(normalized);
        }
    }

    async function handleOpenCacheDir() {
        if (settingsStore.cacheBaseDir) {
            try {
                await openPath(settingsStore.cacheBaseDir);
            } catch (e) {
                console.error("Failed to open cache dir", e);
            }
        }
    }

    async function handleCleanup() {
        isCleaning = true;
        cleanMessage = "Cleaning up...";
        try {
            if (settingsStore.cacheBaseDir) {
                await invoke("cleanup_orphan_thumbnails", {
                    cacheBaseDir: settingsStore.cacheBaseDir,
                });
            }
            cleanMessage = "Cleanup successful!";
        } catch (e) {
            console.error(e);
            cleanMessage = "Failed to cleanup cache.";
        } finally {
            setTimeout(() => {
                isCleaning = false;
                cleanMessage = "";
            }, 3000);
        }
    }

    let isDeletingAll = $state(false);
    let deleteAllMessage = $state("");
    let confirmDeleteAll = $state(false);

    async function handleDeleteAll() {
        if (!confirmDeleteAll) {
            confirmDeleteAll = true;
            return;
        }

        isDeletingAll = true;
        deleteAllMessage = "Deleting all thumbnails...";
        try {
            if (settingsStore.cacheBaseDir) {
                await invoke("delete_all_thumbnails", {
                    cacheBaseDir: settingsStore.cacheBaseDir,
                });
            }
            deleteAllMessage = "All thumbnails deleted!";
            confirmDeleteAll = false;
        } catch (e) {
            console.error(e);
            deleteAllMessage = "Failed to delete thumbnails.";
        } finally {
            setTimeout(() => {
                isDeletingAll = false;
                deleteAllMessage = "";
            }, 3000);
        }
    }

    function handleBackdropClick(e: MouseEvent) {
        if (e.target === e.currentTarget) {
            onClose();
        }
    }

    function handleKeydown(e: KeyboardEvent) {
        if (e.key === "Escape") {
            onClose();
        }
    }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- Backdrop -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
    onclick={handleBackdropClick}
    transition:fade={{ duration: 200 }}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
>
    <!-- Modal Content -->
    <div
        class="w-full max-w-lg overflow-hidden rounded-2xl bg-zinc-900 border border-zinc-800 shadow-2xl"
        transition:scale={{ duration: 200, start: 0.95 }}
    >
        <!-- Header -->
        <div
            class="flex items-center justify-between border-b border-zinc-800 px-6 py-4"
        >
            <h2 class="text-xl font-semibold text-white">Settings</h2>
            <button
                class="flex h-8 w-8 items-center justify-center rounded-full text-zinc-400 hover:bg-zinc-800 hover:text-white transition-colors"
                onclick={onClose}
                aria-label="Close"
            >
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    width="20"
                    height="20"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <line x1="18" y1="6" x2="6" y2="18"></line>
                    <line x1="6" y1="6" x2="18" y2="18"></line>
                </svg>
            </button>
        </div>

        <!-- Body -->
        <div class="p-6">
            <div class="space-y-6">
                <!-- Section: Navigation -->
                <div>
                    <h3
                        class="text-sm font-medium text-zinc-400 uppercase tracking-wider mb-4"
                    >
                        Navigation
                    </h3>
                    <div class="space-y-4">
                        <div class="flex items-center justify-between gap-4">
                            <div>
                                <p class="text-sm font-medium text-zinc-200">
                                    Folder Tree Modifier
                                </p>
                                <p class="text-xs text-zinc-500 mt-1 max-w-sm">
                                    Select the modifier key to use for keyboard
                                    navigation in the folder tree.
                                </p>
                            </div>
                            <select
                                class="bg-zinc-800 text-white text-sm rounded-lg border border-zinc-700 focus:ring-amber-500 focus:border-amber-500 block px-3 py-2"
                                value={settingsStore.treeNavModifier}
                                onchange={(e) =>
                                    settingsStore.setTreeNavModifier(
                                        e.currentTarget.value,
                                    )}
                            >
                                <option value="Alt">Alt / Option</option>
                                <option value="Control">Control</option>
                                <option value="Shift">Shift</option>
                                <option value="Meta">Meta / Command</option>
                            </select>
                        </div>
                    </div>
                </div>

                <!-- Section: Cache Management -->
                <div>
                    <h3
                        class="text-sm font-medium text-zinc-400 uppercase tracking-wider mb-4"
                    >
                        Cache
                    </h3>

                    <div class="space-y-4">
                        <div class="flex items-start justify-between gap-4">
                            <div>
                                <p class="text-sm font-medium text-zinc-200">
                                    Automatic Cleanup
                                </p>
                                <p class="text-xs text-zinc-500 mt-1 max-w-sm">
                                    Delete the thumbnails backing a root
                                    directory when you remove the folder from
                                    your tree.
                                </p>
                            </div>
                            <label
                                class="relative inline-flex items-center cursor-pointer pt-1"
                            >
                                <input
                                    type="checkbox"
                                    class="sr-only peer"
                                    checked={settingsStore.cleanupCacheOnRootRemove}
                                    onchange={(e) =>
                                        settingsStore.setCleanupCacheOnRootRemove(
                                            e.currentTarget.checked,
                                        )}
                                />
                                <div
                                    class="w-11 h-6 bg-zinc-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full rtl:peer-checked:after:-translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[6px] after:start-[2px] after:bg-white after:border-zinc-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-amber-500"
                                ></div>
                            </label>
                        </div>

                        <div class="h-px bg-zinc-800/50 my-2"></div>

                        <div class="flex items-start justify-between gap-4">
                            <div class="flex-1">
                                <p class="text-sm font-medium text-zinc-200">
                                    Cache Location
                                </p>
                                <p
                                    class="text-xs text-zinc-400 mt-1 break-all font-mono bg-zinc-950 p-2 rounded border border-zinc-800"
                                >
                                    {settingsStore.cacheBaseDir ||
                                        "Loading default..."}
                                </p>
                            </div>
                            <div class="flex flex-col gap-2 shrink-0">
                                <button
                                    class="whitespace-nowrap px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-sm font-medium text-white rounded-lg transition-colors border border-zinc-700"
                                    onclick={handleChangeCacheDir}
                                >
                                    Change...
                                </button>
                                <button
                                    class="whitespace-nowrap px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-sm font-medium text-white rounded-lg transition-colors border border-zinc-700"
                                    onclick={handleOpenCacheDir}
                                >
                                    Open
                                </button>
                            </div>
                        </div>

                        <div class="h-px bg-zinc-800/50 my-2"></div>

                        <div class="flex items-start justify-between gap-4">
                            <div>
                                <p class="text-sm font-medium text-zinc-200">
                                    Remove orphan thumbnails
                                </p>
                                <p class="text-xs text-zinc-500 mt-1 max-w-sm">
                                    Remove thumbnails for media files that no
                                    longer exist on disk or thumbnails generated
                                    for folders that got removed from media
                                    viewer.
                                </p>
                                <p class="text-xs text-zinc-500 mt-1 max-w-sm">
                                    This can help free up storage space.
                                </p>
                            </div>
                            <button
                                class="shrink-0 whitespace-nowrap px-4 py-2 bg-zinc-800 hover:bg-zinc-700 text-sm font-medium text-white rounded-lg transition-colors border border-zinc-700 disabled:opacity-50 disabled:cursor-not-allowed"
                                onclick={handleCleanup}
                                disabled={isCleaning}
                            >
                                {isCleaning ? "Cleaning..." : "Clean Up"}
                            </button>
                        </div>
                        {#if cleanMessage}
                            <p class="text-xs text-blue-400" transition:fade>
                                {cleanMessage}
                            </p>
                        {/if}

                        <div class="h-px bg-zinc-800/50 my-2"></div>

                        <div class="flex items-start justify-between gap-4">
                            <div>
                                <p class="text-sm font-medium text-red-400">
                                    Clear entire cache
                                </p>
                                <p class="text-xs text-zinc-500 mt-1 max-w-sm">
                                    Delete all generated thumbnails.
                                </p>
                                <p class="text-xs text-zinc-500 mt-1 max-w-sm">
                                    They will be regenerated the next time you
                                    browse your media folders. This might take a
                                    long time, depending on your collection
                                    size.
                                </p>
                            </div>
                            <button
                                class="shrink-0 px-4 py-2 {confirmDeleteAll
                                    ? 'bg-red-600 hover:bg-red-500 border-red-500 text-white'
                                    : 'bg-red-950/30 hover:bg-red-900/40 text-red-400 border-red-900/50'} text-sm font-medium rounded-lg transition-colors border disabled:opacity-50 disabled:cursor-not-allowed whitespace-nowrap"
                                onclick={handleDeleteAll}
                                disabled={isDeletingAll}
                            >
                                {#if isDeletingAll}
                                    Deleting...
                                {:else if confirmDeleteAll}
                                    Are you sure?
                                {:else}
                                    Delete All
                                {/if}
                            </button>
                        </div>
                        {#if deleteAllMessage}
                            <p class="text-xs text-red-400" transition:fade>
                                {deleteAllMessage}
                            </p>
                        {/if}
                    </div>
                </div>
            </div>
        </div>
    </div>
</div>
