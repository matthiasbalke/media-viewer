<script lang="ts">
    import { convertFileSrc } from "@tauri-apps/api/core";

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
        files: MediaFile[];
        selectedPath: string | null;
        onSelect?: (file: MediaFile) => void;
    }

    let { files, selectedPath, onSelect }: Props = $props();

    let stripContainer: HTMLDivElement | undefined = $state();

    // Auto-scroll to keep the selected thumbnail visible
    $effect(() => {
        if (!selectedPath || !stripContainer) return;
        const active = stripContainer.querySelector('[data-active="true"]');
        if (active) {
            active.scrollIntoView({
                behavior: "smooth",
                block: "nearest",
                inline: "center",
            });
        }
    });
</script>

<div
    class="flex flex-row gap-1 px-2 py-1.5 overflow-x-auto overflow-y-hidden bg-zinc-900/90 border-t border-zinc-700/50 shrink-0 [&::-webkit-scrollbar]:h-1 [&::-webkit-scrollbar-track]:bg-transparent [&::-webkit-scrollbar-thumb]:bg-zinc-700 [&::-webkit-scrollbar-thumb]:rounded-sm"
    bind:this={stripContainer}
>
    {#each files as file}
        <button
            class="shrink-0 w-20 h-[60px] border-2 rounded-md overflow-hidden cursor-pointer p-0 transition-colors duration-150 {file.thumbnailState ===
            'ready'
                ? 'bg-transparent'
                : 'bg-zinc-800'} {file.path === selectedPath
                ? 'border-blue-500 shadow-[0_0_8px_rgba(59,130,246,0.3)]'
                : 'border-transparent hover:border-blue-500/50'}"
            data-active={file.path === selectedPath}
            onclick={() => onSelect?.(file)}
            title={file.name}
        >
            {#if file.thumbnailState === "loading" || file.thumbnailState === "frontend-render"}
                <div
                    class="w-full h-full flex items-center justify-center bg-zinc-900"
                >
                    <div
                        class="w-4 h-4 rounded-full border-2 border-zinc-700 border-t-zinc-400 animate-spin"
                    ></div>
                </div>
            {:else if file.thumbnailState === "error"}
                <div
                    class="w-full h-full flex items-center justify-center bg-zinc-900"
                >
                    <span class="text-lg text-red-400">✕</span>
                </div>
            {:else if file.thumbnailState === "unsupported"}
                <div
                    class="w-full h-full flex items-center justify-center bg-zinc-900"
                >
                    <span class="text-lg text-zinc-500">?</span>
                </div>
            {:else if file.thumbnailSrc}
                <img
                    src={file.thumbnailSrc}
                    alt={file.name}
                    class="w-full h-full object-scale-down block"
                    draggable="false"
                />
            {/if}
        </button>
    {/each}
</div>
