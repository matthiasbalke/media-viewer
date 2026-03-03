<script lang="ts">
    import FolderTree from "./FolderTree.svelte";
    import { readDir, stat } from "@tauri-apps/plugin-fs";
    import { settingsStore } from "$lib/stores/settings.svelte";

    interface Props {
        rootId: string;
        path: string;
        depth?: number;
        selectedPath?: string | null;
        selectedTreeItemId?: string | null;
        onSelect?: (path: string, id: string) => void;
    }

    let { rootId, path, depth = 0, selectedPath = null, selectedTreeItemId = null, onSelect }: Props = $props();

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

    function toggleDir(dirPath: string, id: string) {
        if (expandedDirs.has(dirPath)) {
            expandedDirs.delete(dirPath);
            expandedDirs = new Set(expandedDirs);
        } else {
            expandedDirs.add(dirPath);
            expandedDirs = new Set(expandedDirs);
        }
        // Notify parent of selection
        onSelect?.(dirPath, id);
    }

    // Load entries on mount
    $effect(() => {
        loadEntries();
    });


</script>

<div class="folder-tree">
    {#if loading && depth === 0}
        <div class="text-zinc-500 text-sm py-2">Loading...</div>
    {:else if error}
        <div class="text-red-400 text-sm py-2">{error}</div>
    {:else}
        <!-- Show root directory as top-level element -->
        {#if depth === 0}
            {@const itemId = rootId + '|' + path}
            <button
                class="folder-tree-item flex items-center gap-2 w-full px-2 py-1 text-left text-sm hover:bg-zinc-800 rounded transition-colors {selectedTreeItemId === itemId
                    ? 'text-white font-bold bg-zinc-800/50'
                    : 'text-zinc-300 font-medium'}"
                onclick={() => toggleDir(path, itemId)}
                data-path={path}
                data-id={itemId}
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
                {@const entryId = rootId + '|' + entry.path}
                <button
                    class="folder-tree-item flex items-center gap-2 w-full px-2 py-1 text-left text-sm hover:bg-zinc-800 rounded transition-colors {selectedTreeItemId === entryId
                        ? 'text-white font-bold bg-zinc-800/50'
                        : 'text-zinc-300'}"
                    style="padding-left: {(depth + 1) * 16}px"
                    onclick={() => toggleDir(entry.path, entryId)}
                    data-path={entry.path}
                    data-id={entryId}
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
                        {rootId}
                        path={entry.path}
                        depth={depth + 1}
                        {selectedPath}
                        {selectedTreeItemId}
                        {onSelect}
                    />
                {/if}
            {/each}
        {/if}
    {/if}
</div>
