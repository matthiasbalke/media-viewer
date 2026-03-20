# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Media Viewer is a Tauri v2 desktop application for browsing media collections (a Google Picasa replacement). The Rust backend handles thumbnail generation and caching; the SvelteKit frontend provides the UI with real-time updates via Tauri IPC events.

## Commands

### Development
```bash
bun run tauri dev       # Start full app (frontend + backend)
bun run dev             # Frontend dev server only (http://localhost:1420)
bun run check           # TypeScript/Svelte type checking
bun run check:watch     # Watch mode type checking
```

### Build
```bash
bun run tauri build     # Production build + package
bun run build           # Frontend build only
```

### Testing (Rust)
```bash
cd src-tauri && cargo test                        # Run all tests
cd src-tauri && cargo test thumbnail              # Run thumbnail module tests
cd src-tauri && cargo test test_name -- --nocapture  # Run single test with output
```

## Architecture

### Frontend → Backend Communication
- Frontend invokes Tauri commands via `invoke()` from `@tauri-apps/api/core`
- Backend streams results back via `emit()` events (e.g., `thumbnail-update`)
- Session IDs are used to track/cancel in-progress thumbnail batches when the folder changes

### Thumbnail Pipeline
The core feature. Flow:
1. `MediaGrid.svelte` calls `generate_thumbnails(dir, sessionId, cacheDir)`
2. `src-tauri/src/thumbnail/service.rs` processes files concurrently (semaphore, MAX_WORKERS=4)
3. For each file, determines type via magic bytes (not file extension):
   - **Standard images** (jpg, png, gif, bmp, webp, tiff, ico): resize to ≤512px → save as JPEG
   - **HEIC/HEIF**: extract embedded EXIF IFD1 thumbnail → ffmpeg fallback
   - **Video** (mp4, webm, mkv, avi, mov, etc.): extract embedded thumbnail → ffmpeg frame extraction → emit `frontend-render` status for browser-side rendering
4. Each result emits a `thumbnail-update` event; frontend updates the grid as events arrive
5. Thumbnails cached by hash of normalized path; staleness tracked via manifest + mtime

### Thumbnail Cache
- Location: platform app data dir, configurable in settings
  - macOS: `~/Library/Application Support/com.github.matthiasbalke.media-viewer/thumbnails`
  - Linux: `~/.local/share/com.github.matthiasbalke.media-viewer/thumbnails`
- Named `{hash}.jpg` using `DefaultHasher` on the normalized file path

### Key Tauri Commands (src-tauri/src/lib.rs)
- `generate_thumbnails` — main entry point for thumbnail batch
- `save_video_thumbnail` — saves browser-rendered video frame to cache
- `cleanup_thumbnails_for_dir`, `cleanup_orphan_thumbnails`, `delete_all_thumbnails`

### Frontend State
- Svelte 5 runes (`$state`, `$bindable`) throughout
- Settings persisted via `@tauri-apps/plugin-store` (see `src/lib/stores/settings.svelte.ts`)
- Window starts hidden, shown after first paint (avoids flicker)

### External Dependency
- **ffmpeg** — optional, searched at runtime in `$PATH`, `/opt/homebrew/bin/ffmpeg`, `/usr/local/bin/ffmpeg`; required for video and HEIC fallback thumbnail extraction

## Workflow Guidelines (from AGENTS.md)

- Always create a plan and get approval before changing files
- Work step by step when implementing
- Check licenses before adding new dependencies and get approval
- Always verify which library version is in use before using its API
