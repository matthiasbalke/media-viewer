<script lang="ts">
  import "../app.css";
  import FolderTree from "$lib/components/FolderTree.svelte";
  import MediaGrid from "$lib/components/MediaGrid.svelte";
  import MediaViewer from "$lib/components/MediaViewer.svelte";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";
  import { settingsStore } from "$lib/stores/settings.svelte";
  import SettingsModal from "$lib/components/SettingsModal.svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  onMount(() => {
    // Wait for Svelte to apply all pending state changes to the DOM
    tick().then(() => {
      // Wait for the browser to calculate layout and paint the frame
      requestAnimationFrame(() => {
        // A minimal timeout ensures the OS compositor has rasterized the paint
        setTimeout(() => {
          getCurrentWindow().show();
        }, 50);
      });
    });

    // Listen for the native menu shortcut event to open settings
    const unlistenPromise = listen("open-settings", () => {
      showSettings = true;
    });

    return () => {
      // Cleanup the event listener when component unmounts
      unlistenPromise.then((unlisten) => unlisten());
    };
  });

  let { children } = $props();

  // normalize path for cross platform compatibility
  function normalizePath(path: string): string {
    return path.replace(/\\/g, "/");
  }

  // Currently selected directory for media grid
  let selectedPath: string | null = $state(null);
  let selectedTreeItemId: string | null = $state(null);

  // Currently viewed file (null = grid mode, set = viewer mode)
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
  let viewingFile: MediaFile | null = $state(null);
  let mediaFiles: MediaFile[] = $state([]);
  let showSettings = $state(false);

  // Item count from media grid
  let mediaItemCount: number = $state(0);

  // Sidebar drag state
  let isSidebarDragging = $state(false);
  let currentSidebarWidth = $state(256); // Default, updated from store on mount

  // Sync initial width when settings are ready
  $effect(() => {
    if (settingsStore.ready && !isSidebarDragging) {
      if (currentSidebarWidth === 256 && settingsStore.sidebarWidth !== 256) {
        currentSidebarWidth = settingsStore.sidebarWidth;
      }
    }
  });

  function handleSidebarDragStart(e: MouseEvent) {
    isSidebarDragging = true;
    currentSidebarWidth = settingsStore.sidebarWidth;
  }

  function handleSidebarDragMove(e: MouseEvent) {
    if (!isSidebarDragging) return;

    // Calculate new width, clamp between 256 and 600
    let newWidth = e.clientX;
    if (newWidth < 256) newWidth = 256;
    if (newWidth > 600) newWidth = 600;

    currentSidebarWidth = newWidth;
  }

  function handleSidebarDragEnd() {
    if (isSidebarDragging) {
      isSidebarDragging = false;
      settingsStore.setSidebarWidth(currentSidebarWidth);
    }
  }

  function handleFolderSelect(path: string, id?: string) {
    selectedPath = path;
    selectedTreeItemId = id || null;
    viewingFile = null; // reset viewer when switching folders
  }

  function handleImageOpen(file: MediaFile) {
    viewingFile = file;
  }

  function handleViewerClose() {
    viewingFile = null;
  }

  function handleViewerFileChange(file: MediaFile) {
    viewingFile = file;
  }

  async function addFolderTree() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select Root Directory",
    });

    if (selected && typeof selected === "string") {
      const normalized = normalizePath(selected);
      settingsStore.addRootPath(normalized);
      // Auto-select the new directory
      selectedPath = normalized;
      selectedTreeItemId = normalized + "|" + normalized;
    }
  }

  async function removeRootPath(pathToRemove: string) {
    // Clean up thumbnails for this directory tree if enabled
    try {
      if (
        settingsStore.cacheBaseDir &&
        settingsStore.cleanupCacheOnRootRemove
      ) {
        await invoke("cleanup_thumbnails_for_dir", {
          dir: pathToRemove,
          cacheBaseDir: settingsStore.cacheBaseDir,
        });
      }
    } catch (e) {
      console.error("Thumbnail cleanup failed:", e);
    }

    // Remove from rootPaths
    settingsStore.removeRootPath(pathToRemove);

    // Clear selection if it was within the removed tree
    if (selectedPath?.startsWith(pathToRemove)) {
      selectedPath = null;
      selectedTreeItemId = null;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
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
        document.querySelectorAll<HTMLButtonElement>(".folder-tree-item"),
      );
      if (items.length === 0) return;

      const currentIndex = items.findIndex(
        (el) => el.dataset.id === selectedTreeItemId,
      );

      if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
        if (currentIndex === -1) {
          const firstItem = items[0];
          if (firstItem.dataset.path) {
            handleFolderSelect(firstItem.dataset.path, firstItem.dataset.id);
            firstItem.scrollIntoView({ block: "nearest" });
          }
          return;
        }

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
      let isInitial = false;

      if (currentIndex === -1) {
        nextIndex = 0;
        isInitial = true;
      } else if (e.key === "ArrowDown") {
        nextIndex =
          currentIndex < items.length - 1 ? currentIndex + 1 : currentIndex;
      } else if (e.key === "ArrowUp") {
        nextIndex = currentIndex > 0 ? currentIndex - 1 : 0;
      } else if (e.key === "PageDown") {
        nextIndex = Math.min(currentIndex + 10, items.length - 1);
      } else if (e.key === "PageUp") {
        nextIndex = Math.max(currentIndex - 10, 0);
      }

      if (
        (isInitial || nextIndex !== currentIndex) &&
        nextIndex >= 0 &&
        nextIndex < items.length
      ) {
        const nextPath = items[nextIndex].dataset.path;
        const nextId = items[nextIndex].dataset.id;
        if (nextPath) {
          handleFolderSelect(nextPath, nextId);
          items[nextIndex].scrollIntoView({ block: "nearest" });
        }
      }
    }
  }
</script>

<svelte:window
  onmousemove={handleSidebarDragMove}
  onmouseup={handleSidebarDragEnd}
  onkeydown={handleKeydown}
/>

<div
  class="flex h-screen bg-zinc-950 text-zinc-100 {isSidebarDragging
    ? 'select-none'
    : ''}"
>
  <!-- Sidebar -->
  <aside
    class="bg-zinc-900 border-r border-zinc-800 flex flex-col shrink-0"
    style="width: {isSidebarDragging
      ? currentSidebarWidth
      : settingsStore.sidebarWidth}px"
  >
    <div class="p-4 border-b border-zinc-800 flex items-center justify-between">
      <h1 class="text-lg font-semibold text-white">Media Viewer</h1>
      <div class="flex items-center gap-2">
        <button
          class="w-8 h-8 flex items-center justify-center rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-400 hover:text-white transition-colors"
          onclick={() => (showSettings = true)}
          title="Settings"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <circle cx="12" cy="12" r="3"></circle>
            <path
              d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"
            ></path>
          </svg>
        </button>
        <button
          class="w-8 h-8 flex items-center justify-center rounded-lg bg-zinc-800 hover:bg-zinc-700 text-zinc-400 hover:text-white transition-colors"
          onclick={addFolderTree}
          title="Add folder"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            ><line x1="12" y1="5" x2="12" y2="19"></line><line
              x1="5"
              y1="12"
              x2="19"
              y2="12"
            ></line></svg
          >
        </button>
      </div>
    </div>

    <nav class="flex-1 p-2 overflow-y-auto">
      {#if settingsStore.rootPaths.length === 0}
        <div class="text-zinc-500 text-sm py-4 text-center">
          Click the + icon to add a folder
        </div>
      {:else}
        {#each settingsStore.rootPaths as rootPath}
          <div class="mb-2 group/root relative">
            <FolderTree
              rootId={rootPath}
              path={rootPath}
              {selectedPath}
              {selectedTreeItemId}
              onSelect={handleFolderSelect}
            />
            <button
              class="absolute top-0 right-0 w-6 h-6 flex items-center justify-center
                     text-zinc-500 hover:text-red-400 rounded
                     opacity-0 group-hover/root:opacity-100 transition-opacity
                     text-xs"
              onclick={() => removeRootPath(rootPath)}
              title="Remove directory"
            >
              ✕
            </button>
          </div>
        {/each}
      {/if}
    </nav>
  </aside>

  <!-- Resizer Handle: drag-only interaction, keyboard resize not implemented -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="w-1 cursor-col-resize hover:bg-amber-500 transition-colors z-10 {isSidebarDragging
      ? 'bg-amber-500'
      : 'bg-transparent'}"
    onmousedown={handleSidebarDragStart}
    role="separator"
    tabindex="-1"
  ></div>

  <!-- Main content area -->
  <main class="flex-1 flex flex-col overflow-hidden">
    <div class="flex-1 overflow-auto">
      {#if viewingFile}
        <MediaViewer
          file={viewingFile}
          files={mediaFiles}
          onClose={handleViewerClose}
          onFileChange={handleViewerFileChange}
        />
      {:else}
        <MediaGrid
          path={selectedPath}
          thumbnailSize={settingsStore.thumbnailSize}
          bind:itemCount={mediaItemCount}
          bind:mediaFiles
          onImageOpen={handleImageOpen}
        />
      {/if}
    </div>
    <StatusBar
      bind:thumbnailSize={settingsStore.thumbnailSize}
      itemCount={mediaItemCount}
    />
    {@render children()}
  </main>
</div>

{#if showSettings}
  <SettingsModal onClose={() => (showSettings = false)} />
{/if}
