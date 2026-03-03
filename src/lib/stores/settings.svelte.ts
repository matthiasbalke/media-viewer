import { load } from "@tauri-apps/plugin-store";
import { appLocalDataDir, join } from '@tauri-apps/api/path';

const DEFAULT_THUMBNAIL_SIZE = 128;
const DEFAULT_SIDEBAR_WIDTH = 256;
const STORE_NAME = "settings.json";

const storeOptions = {
    defaults: {
        thumbnailSize: DEFAULT_THUMBNAIL_SIZE,
        sidebarWidth: DEFAULT_SIDEBAR_WIDTH,
        rootPaths: [] as string[],
        treeNavModifier: "Alt",
    },
    autoSave: true as const,
    overrideDefaults: false,
};

class SettingsStore {
    thumbnailSize = $state(DEFAULT_THUMBNAIL_SIZE);
    sidebarWidth = $state(DEFAULT_SIDEBAR_WIDTH);
    rootPaths = $state<string[]>([]);
    cacheBaseDir = $state<string | null>(null);
    treeNavModifier = $state("Alt");
    ready = $state(false);

    private store: any = null;
    private saveTimeout: ReturnType<typeof setTimeout> | null = null;

    constructor() {
        this.init();
    }

    async init() {
        try {
            this.store = await load(STORE_NAME, storeOptions);

            const savedSize = await this.store.get("thumbnailSize") as number | null | undefined;
            if (savedSize !== null && savedSize !== undefined) {
                this.thumbnailSize = savedSize;
            }

            const savedWidth = await this.store.get("sidebarWidth") as number | null | undefined;
            if (savedWidth !== null && savedWidth !== undefined) {
                this.sidebarWidth = savedWidth;
            }

            const savedPaths = await this.store.get("rootPaths") as string[] | null | undefined;
            if (savedPaths && savedPaths.length > 0) {
                this.rootPaths = savedPaths;
            }

            let savedCacheDir = await this.store.get("cacheBaseDir") as string | null | undefined;
            if (savedCacheDir) {
                this.cacheBaseDir = savedCacheDir;
            } else {
                // Determine a safe cross-platform default for the cache directory:
                // Usually this maps to:
                // Linux:   ~/.local/share/<bundle-identifier>/thumbnails
                // macOS:   ~/Library/Application Support/<bundle-identifier>/thumbnails
                // Windows: C:\Users\<user>\AppData\Local\<bundle-identifier>\thumbnails
                const baseDir = await appLocalDataDir();
                this.cacheBaseDir = await join(baseDir, "thumbnails");
                await this.store.set("cacheBaseDir", this.cacheBaseDir);
            }

            const savedTreeNavModifier = await this.store.get("treeNavModifier") as string | null | undefined;
            if (savedTreeNavModifier !== null && savedTreeNavModifier !== undefined) {
                this.treeNavModifier = savedTreeNavModifier;
            }

        } catch (error) {
            console.error("Failed to load settings:", error);
        } finally {
            this.ready = true;
        }
    }

    // Debounced save for rapidly changing values like slider
    async saveSize(size: number) {
        this.thumbnailSize = size;
        this.debouncedSave("thumbnailSize", size);
    }

    async setSidebarWidth(width: number) {
        this.sidebarWidth = width;
        await this.saveNow("sidebarWidth", width);
    }

    async addRootPath(path: string) {
        if (!this.rootPaths.includes(path)) {
            this.rootPaths = [...this.rootPaths, path];
            await this.saveNow("rootPaths", this.rootPaths);
        }
    }

    async removeRootPath(path: string) {
        this.rootPaths = this.rootPaths.filter(p => p !== path);
        await this.saveNow("rootPaths", this.rootPaths);
    }

    async setCacheBaseDir(path: string) {
        this.cacheBaseDir = path;
        await this.saveNow("cacheBaseDir", path);
    }

    async setTreeNavModifier(modifier: string) {
        this.treeNavModifier = modifier;
        await this.saveNow("treeNavModifier", modifier);
    }

    private debouncedSave(key: string, value: any) {
        if (!this.ready || !this.store) return;

        if (this.saveTimeout) {
            clearTimeout(this.saveTimeout);
        }

        this.saveTimeout = setTimeout(async () => {
            try {
                await this.store.set(key, value);
            } catch (error) {
                console.error(`Failed to save ${key}:`, error);
            }
        }, 500); // 500ms debounce
    }

    private async saveNow(key: string, value: any) {
        if (!this.ready || !this.store) return;
        try {
            await this.store.set(key, value);
        } catch (error) {
            console.error(`Failed to save ${key}:`, error);
        }
    }
}

export const settingsStore = new SettingsStore();
