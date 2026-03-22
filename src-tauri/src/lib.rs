mod thumbnail;

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    Emitter,
};
use tauri_plugin_updater::UpdaterExt;
use thumbnail::ThumbnailService;

#[tauri::command]
async fn generate_thumbnails(
    dir: String,
    session_id: u64,
    cache_base_dir: String,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    ThumbnailService::generate_for_dir(dir, session_id, cache_base_dir, app_handle).await
}

#[tauri::command]
async fn cleanup_thumbnails_for_dir(dir: String, cache_base_dir: String) -> Result<u32, String> {
    tokio::task::spawn_blocking(move || thumbnail::cleanup_for_prefix(&dir, &cache_base_dir))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
async fn cleanup_orphan_thumbnails(cache_base_dir: String) -> Result<u32, String> {
    tokio::task::spawn_blocking(move || thumbnail::cleanup_orphans(&cache_base_dir))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
async fn delete_all_thumbnails(cache_base_dir: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || thumbnail::delete_all(&cache_base_dir))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

#[tauri::command]
async fn get_image_preview(path: String, cache_base_dir: String) -> Result<String, String> {
    let source = std::path::PathBuf::from(&path);
    let base = std::path::PathBuf::from(&cache_base_dir);
    let dest = thumbnail::cache::preview_path(&source, &base, "jpg");
    std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    if !dest.exists() {
        tokio::task::spawn_blocking({
            let s = source.clone();
            let d = dest.clone();
            move || thumbnail::service::convert_to_jpeg_ffmpeg(&s, &d)
        })
        .await
        .map_err(|e| e.to_string())??;
    }
    Ok(dest.to_string_lossy().into_owned())
}

#[tauri::command]
async fn get_video_preview(path: String, cache_base_dir: String) -> Result<String, String> {
    let source = std::path::PathBuf::from(&path);
    let base = std::path::PathBuf::from(&cache_base_dir);
    let dest = thumbnail::cache::preview_path(&source, &base, "mp4");
    std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    if !dest.exists() {
        tokio::task::spawn_blocking({
            let s = source.clone();
            let d = dest.clone();
            move || thumbnail::service::remux_to_mp4_ffmpeg(&s, &d)
        })
        .await
        .map_err(|e| e.to_string())??;
    }
    Ok(dest.to_string_lossy().into_owned())
}

#[tauri::command]
async fn save_video_thumbnail(
    path: String,
    base64_data: String,
    cache_base_dir: String,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        ThumbnailService::save_video_thumbnail(path, base64_data, cache_base_dir)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let verbose = std::env::args().any(|a| a == "--verbose" || a == "-v");
    let log_level = if verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Warn
    };
    env_logger::Builder::new()
        .filter_level(log_level)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let handle = app.handle();
            let mut menu_builder = MenuBuilder::new(handle);

            // macOS typically has an "App Name" menu as the first item
            #[cfg(target_os = "macos")]
            {
                let app_submenu = SubmenuBuilder::new(handle, "Media Viewer")
                    .about(None)
                    .separator()
                    .item(
                        &MenuItemBuilder::with_id("open-settings", "Settings...")
                            .accelerator("CmdOrCtrl+,")
                            .build(handle)?,
                    )
                    .separator()
                    .services()
                    .separator()
                    .hide()
                    .hide_others()
                    .show_all()
                    .separator()
                    .quit()
                    .build()?;

                menu_builder = menu_builder.item(&app_submenu);
            }

            // Other generic submenus
            let file_submenu = SubmenuBuilder::new(handle, "File").close_window().build()?;

            let window_submenu = SubmenuBuilder::new(handle, "Window")
                .minimize()
                .maximize()
                .separator()
                .close_window()
                .build()?;

            menu_builder = menu_builder.items(&[&file_submenu, &window_submenu]);

            #[cfg(not(target_os = "macos"))]
            {
                // For Windows/Linux, typically settings is under File or Edit
                let edit_submenu = SubmenuBuilder::new(handle, "Edit")
                    .item(
                        &MenuItemBuilder::with_id("open-settings", "Settings...")
                            .accelerator("CmdOrCtrl+,")
                            .build(handle)?,
                    )
                    .build()?;

                menu_builder = menu_builder.item(&edit_submenu);
            }

            let menu = menu_builder.build()?;
            app.set_menu(menu)?;

            Ok(())
        })
        .on_menu_event(|app, event| {
            if event.id() == "open-settings" {
                let _ = app.emit("open-settings", ());
            }
        })
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_persisted_scope::init())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            generate_thumbnails,
            cleanup_thumbnails_for_dir,
            cleanup_orphan_thumbnails,
            delete_all_thumbnails,
            save_video_thumbnail,
            get_image_preview,
            get_video_preview
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = update(handle).await {
                    log::error!("Failed to check for updates: {}", err);
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn update(app: tauri::AppHandle) -> tauri_plugin_updater::Result<()> {
    log::info!("checking for updates ...");
    if let Some(update) = app.updater()?.check().await? {
        let mut downloaded = 0;

        log::info!(
            "updating to version {} released on {}",
            update.version,
            update
                .date
                .map(|d| d.to_string())
                .unwrap_or_else(|| String::from("<unknown date>"))
        );

        // alternatively we could also call update.download() and update.install() separately
        update
            .download_and_install(
                |chunk_length, content_length| {
                    downloaded += chunk_length;
                    log::debug!("downloaded {downloaded} from {content_length:?}");
                },
                || {
                    log::info!("download finished");
                },
            )
            .await?;

        log::info!("update installed");
        app.restart();
    } else {
        log::info!("no update found.");
    }

    Ok(())
}
