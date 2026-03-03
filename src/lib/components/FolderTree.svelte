<script lang="ts">
    import FolderTree from "./FolderTree.svelte";
    import { readDir, stat } from "@tauri-apps/plugin-fs";
    import { settingsStore } from "$lib/stores/settings.svelte";

    interface Props {
        path: string;
        depth?: number;
        selectedPath?: string | null;
        onSelect?: (path: string) => void;
    }

    let { path, depth = 0, selectedPath = null, onSelect }: Props = $props();

    interface FileEntry {
        name: string;
        path: string;
        isDirectory: boolean;
        hasSubfolders: boolean;
    }

    let entries: FileEntry[] = $state([]);
    let expandedDirs: Set<string> = $state(new Set());
    let loading = $state(true);
    let error: string | null = $state(null);

    // Extract directory name from path
    function getDirName(dirPath: string): string {
        const parts = dirPath.split("/").filter(Boolean);
        return parts[parts.length - 1] || dirPath;
    }

    async function hasSubfolders(dirPath: string): Promise<boolean> {
        try {
            const children = await readDir(dirPath);
            for (const child of children) {
                const childPath = `${dirPath}/${child.name}`;
                try {
                    const info = await stat(childPath);
                    if (info.isDirectory) return true;
                } catch {
                    // Skip files we can't stat
                }
            }
        } catch {
            // Can't read dir
        }
        return false;
    }

    async function loadEntries() {
        try {
            loading = true;
            error = null;

            const dirEntries = await readDir(path);
            const processed: FileEntry[] = [];

            for (const entry of dirEntries) {
                const fullPath = `${path}/${entry.name}`;
                try {
                    const info = await stat(fullPath);
                    const isDir = info.isDirectory;

                    // Show directories only
                    if (isDir) {
                        processed.push({
                            name: entry.name,
                            path: fullPath,
                            isDirectory: isDir,
                            hasSubfolders: await hasSubfolders(fullPath),
                        });
                    }
                } catch {
                    // Skip files we can't stat
                }
            }

            // Sort: directories by name
            entries = processed.sort((a, b) => a.name.localeCompare(b.name));
        } catch (e) {
            error = e instanceof Error ? e.message : "Failed to read directory";
        } finally {
            loading = false;
        }
    }

    function toggleDir(dirPath: string) {
        if (expandedDirs.has(dirPath)) {
            expandedDirs.delete(dirPath);
            expandedDirs = new Set(expandedDirs);
        } else {
            expandedDirs.add(dirPath);
            expandedDirs = new Set(expandedDirs);
        }
        // Notify parent of selection
        onSelect?.(dirPath);
    }

    // Load entries on mount
    $effect(() => {
        loadEntries();
    });

    function handleKeydown(e: KeyboardEvent) {
        if (depth !== 0) return; // Only process at root level

        const modifier = settingsStore.treeNavModifier;
        const hasModifier =
            (modifier === "Alt" &&
                e.altKey &&
                !e.ctrlKey &&
                !e.shiftKey &&
                !e.metaKey) ||
            (modifier === "Control" &&
                e.ctrlKey &&
                !e.altKey &&
                !e.shiftKey &&
                !e.metaKey) ||
            (modifier === "Shift" &&
                e.shiftKey &&
                !e.ctrlKey &&
                !e.altKey &&
                !e.metaKey) ||
            (modifier === "Meta" &&
                e.metaKey &&
                !e.ctrlKey &&
                !e.altKey &&
                !e.shiftKey);

        if (!hasModifier) return;

        if (
            [
                "ArrowUp",
                "ArrowDown",
                "PageUp",
                "PageDown",
                "ArrowLeft",
                "ArrowRight",
            ].includes(e.key)
        ) {
            e.preventDefault();

            // Use DOM query to find all flattened visible items in vertical order
            const items = Array.from(
                document.querySelectorAll<HTMLButtonElement>(
                    ".folder-tree-item",
                ),
            );
            if (items.length === 0) return;

            let currentIndex = items.findIndex(
                (el) => el.dataset.path === selectedPath,
            );

            let isInitial = false;
            if (currentIndex === -1) {
                currentIndex = 0;
                isInitial = true;
            }

            const currentItem = items[currentIndex];

            if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
                // if (currentIndex === -1) {
                //     const firstItem = items[0];
                //     if (firstItem.dataset.path) {
                //         onSelect?.(firstItem.dataset.path);
                //         firstItem.scrollIntoView({ block: "nearest" });
                //     }
                //     return;
                // }

                const currentItem = items[currentIndex];
                if (e.key === "ArrowRight") {
                    if (
                        currentItem.dataset.expanded === "false" &&
                        currentItem.dataset.hasSubfolders === "true"
                    ) {
                        currentItem.click();
                    }
                } else if (e.key === "ArrowLeft") {
                    if (currentItem.dataset.expanded === "true") {
                        currentItem.click();
                    }
                }
                return;
            }

            let nextIndex = currentIndex;

            if (e.key === "ArrowDown") {
                nextIndex =
                    currentIndex < items.length - 1
                        ? currentIndex + 1
                        : currentIndex;
            } else if (e.key === "ArrowUp") {
                nextIndex = currentIndex > 0 ? currentIndex - 1 : 0;
            } else if (e.key === "PageDown") {
                nextIndex = Math.min(currentIndex + 10, items.length - 1);
            } else if (e.key === "PageUp") {
                nextIndex = Math.max(currentIndex - 10, 0);
            }

            if (
                nextIndex !== currentIndex &&
                nextIndex >= 0 &&
                nextIndex < items.length
            ) {
                const nextPath = items[nextIndex].dataset.path;
                if (nextPath) {
                    onSelect?.(nextPath);
                    items[nextIndex].scrollIntoView({ block: "nearest" });
                }
            }
        }
    }
</script>

<div class="folder-tree">
    {#if loading && depth === 0}
        <div class="text-zinc-500 text-sm py-2">Loading...</div>
    {:else if error}
        <div class="text-red-400 text-sm py-2">{error}</div>
    {:else}
        <!-- Show root directory as top-level element -->
        {#if depth === 0}
            <button
                class="folder-tree-item flex items-center gap-2 w-full px-2 py-1 text-left text-sm hover:bg-zinc-800 rounded transition-colors {selectedPath ===
                path
                    ? 'text-white font-bold bg-zinc-800/50'
                    : 'text-zinc-300 font-medium'}"
                onclick={() => toggleDir(path)}
                data-path={path}
                data-expanded={expandedDirs.has(path)}
                data-has-subfolders={true}
            >
                <span class="text-zinc-500 w-4 text-center">
                    {expandedDirs.has(path) ? "▼" : "▶"}
                </span>
                <span class="text-amber-400">📁</span>
                <span class="truncate">{getDirName(path)}</span>
            </button>
        {/if}

        <!-- Show children when expanded (or always for non-root) -->
        {#if depth > 0 || expandedDirs.has(path)}
            {#each entries as entry}
                <button
                    class="folder-tree-item flex items-center gap-2 w-full px-2 py-1 text-left text-sm hover:bg-zinc-800 rounded transition-colors {selectedPath ===
                    entry.path
                        ? 'text-white font-bold bg-zinc-800/50'
                        : 'text-zinc-300'}"
                    style="padding-left: {(depth + 1) * 16}px"
                    onclick={() => toggleDir(entry.path)}
                    data-path={entry.path}
                    data-expanded={expandedDirs.has(entry.path)}
                    data-has-subfolders={entry.hasSubfolders}
                >
                    <span class="text-zinc-500 w-4 text-center">
                        {#if entry.hasSubfolders}
                            {expandedDirs.has(entry.path) ? "▼" : "▶"}
                        {/if}
                    </span>
                    <span class="text-amber-400">📁</span>
                    <span class="truncate">{entry.name}</span>
                </button>

                {#if expandedDirs.has(entry.path)}
                    <FolderTree
                        path={entry.path}
                        depth={depth + 1}
                        {selectedPath}
                        {onSelect}
                    />
                {/if}
            {/each}
        {/if}
    {/if}
</div>
