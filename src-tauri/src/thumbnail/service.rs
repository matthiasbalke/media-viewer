use super::cache;
use super::normalize_path;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;

const THUMBNAIL_SIZE: u32 = 512;
const MAX_WORKERS: usize = 4;

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff", "tif", "ico",
];

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "webm", "mkv", "avi", "mov", "wmv", "flv", "m4v"];

const HEIC_EXTENSIONS: &[&str] = &["heic", "heif"];

const SUPPORTED_FORMATS: &[image::ImageFormat] = &[
    image::ImageFormat::Jpeg,
    image::ImageFormat::Png,
    image::ImageFormat::Gif,
    image::ImageFormat::Bmp,
    image::ImageFormat::WebP,
    image::ImageFormat::Tiff,
    image::ImageFormat::Ico,
];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ThumbnailUpdate {
    path: String,
    status: String,
    thumbnail_path: Option<String>,
    session_id: u64,
}

pub struct ThumbnailService;

impl ThumbnailService {
    /// Returns true if the file is a supported image format by checking its magic bytes.
    fn is_supported(path: &Path) -> bool {
        // First try to infer type from the file contents (magic bytes)
        // If that fails (e.g. permission error, file doesn't exist), fall back to checking the extension.
        if let Ok(reader) = Self::get_image_reader(path) {
            if let Some(format) = reader.format() {
                if SUPPORTED_FORMATS.contains(&format) {
                    return true;
                }
            }
        }

        // Fallback for files infer might miss but image crate might support
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
            .unwrap_or(false)
    }

    fn is_video(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| VIDEO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
            .unwrap_or(false)
    }

    fn is_heic(path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| HEIC_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
            .unwrap_or(false)
    }

    /// Opens an image, parses magic bytes to guess the format, and returns the reader.
    fn get_image_reader(
        source: &Path,
    ) -> Result<image::ImageReader<std::io::BufReader<std::fs::File>>, String> {
        image::ImageReader::open(source)
            .map_err(|e| {
                format!(
                    "Failed to open image {}: {}",
                    normalize_path(&source.to_string_lossy()),
                    e
                )
            })?
            .with_guessed_format()
            .map_err(|e| {
                format!(
                    "Failed to guess format for {}: {}",
                    normalize_path(&source.to_string_lossy()),
                    e
                )
            })
    }

    /// Loads an image from a path, using magic bytes to correctly guess the format.
    fn load_image(source: &Path) -> Result<image::DynamicImage, String> {
        Self::get_image_reader(source)?.decode().map_err(|e| {
            format!(
                "Failed to decode image {}: {}",
                normalize_path(&source.to_string_lossy()),
                e
            )
        })
    }

    /// Attempts to extract an embedded thumbnail image from a video file (e.g. iPhone .MOV / Live Photo).
    /// QuickTime containers from iOS devices have a `thmb` track with a JPEG preview.
    /// Returns the raw JPEG/image bytes if found, or None.
    fn extract_embedded_video_thumbnail(path: &Path) -> Option<Vec<u8>> {
        use mp4::{Mp4Reader, TrackType};

        let file = std::fs::File::open(path).ok()?;
        let size = file.metadata().ok()?.len();
        let reader = std::io::BufReader::new(file);
        let mut mp4 = Mp4Reader::read_header(reader, size).ok()?;

        // iPhone MOV files embed a small JPEG thumbnail track detectable by small frame dimensions.
        // Collect IDs + dimensions of all video tracks.
        let mut video_tracks: Vec<(u32, u16, u16)> = mp4
            .tracks()
            .iter()
            .filter_map(|(&id, track)| {
                if matches!(track.track_type(), Ok(TrackType::Video)) {
                    let w = track.width();
                    let h = track.height();
                    if w > 0 && h > 0 {
                        Some((id, w, h))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Sort ascending by width — the thumbnail track is always the smallest
        video_tracks.sort_by_key(|&(_, w, _)| w);

        // Try tracks <= 320px wide first (thumbnail tracks), then fall through
        for &(track_id, w, _) in &video_tracks {
            if w > 320 {
                break;
            }
            if let Ok(Some(sample)) = mp4.read_sample(track_id, 1) {
                let bytes = sample.bytes.to_vec();
                if !bytes.is_empty() {
                    return Some(bytes);
                }
            }
        }

        None
    }

    /// Extracts the embedded JPEG thumbnail from a HEIC/HEIF file using EXIF IFD1 data.
    /// iPhone HEIC files always contain a small JPEG preview in their EXIF block.
    /// Returns the raw JPEG bytes if found, or None.
    fn extract_heic_thumbnail(path: &Path) -> Option<Vec<u8>> {
        use exif::{In, Reader, Tag, Value};
        use std::io::BufReader;

        let file = std::fs::File::open(path).ok()?;
        let mut buf = BufReader::new(file);

        // kamadak-exif supports reading EXIF from HEIF/HEIC containers directly
        let exif = Reader::new().read_from_container(&mut buf).ok()?;

        // IFD1 contains the embedded thumbnail image reference
        let jpeg_offset = match exif.get_field(Tag::JPEGInterchangeFormat, In::THUMBNAIL) {
            Some(f) => match &f.value {
                Value::Long(v) => match v.first() {
                    Some(&o) => o as usize,
                    None => return None,
                },
                _ => return None,
            },
            None => return None,
        };

        let jpeg_length = match exif.get_field(Tag::JPEGInterchangeFormatLength, In::THUMBNAIL) {
            Some(f) => match &f.value {
                Value::Long(v) => match v.first() {
                    Some(&l) => l as usize,
                    None => return None,
                },
                _ => return None,
            },
            None => return None,
        };

        if jpeg_length == 0 {
            return None;
        }

        // exif.buf() returns the raw TIFF-format EXIF bytes.
        // JPEGInterchangeFormat offset is relative to the TIFF header (buf start).
        let raw = exif.buf();
        let end = jpeg_offset.saturating_add(jpeg_length);
        if end > raw.len() {
            return None;
        }

        let thumb_bytes = raw[jpeg_offset..end].to_vec();

        // Validate JPEG magic bytes (0xFF 0xD8 0xFF)
        if thumb_bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(thumb_bytes)
        } else {
            None
        }
    }

    /// Extracts a single video frame as JPEG bytes using the system `ffmpeg` binary.

    /// Searches common Homebrew installation paths on macOS.
    /// Returns the JPEG bytes on success, or None if ffmpeg is not available/fails.
    fn extract_video_frame_ffmpeg(path: &Path) -> Option<Vec<u8>> {
        // Unique temp file path per video to avoid collisions
        let file_stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "video".to_string());
        let temp_out = std::env::temp_dir().join(format!("miru_thumb_{}.jpg", file_stem));

        // Try common ffmpeg locations: PATH first, then Homebrew paths
        let ffmpeg_candidates = [
            "ffmpeg",
            "/opt/homebrew/bin/ffmpeg",
            "/usr/local/bin/ffmpeg",
        ];

        let video_duration_secs: f64 = {
            // Quick duration estimate from file size heuristic; default 10s
            // We seek to min(1s, duration/2) to get a representative frame
            1.0
        };

        for ffmpeg in &ffmpeg_candidates {
            let result = std::process::Command::new(ffmpeg)
                .args([
                    "-ss",
                    &format!("{}", video_duration_secs),
                    "-i",
                    &path.to_string_lossy().to_string(),
                    "-vframes",
                    "1",
                    "-q:v",
                    "3",
                    "-update",
                    "1",
                    "-y",
                    &temp_out.to_string_lossy().to_string(),
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();

            if let Ok(status) = result {
                if status.success() {
                    if let Ok(bytes) = std::fs::read(&temp_out) {
                        let _ = std::fs::remove_file(&temp_out);
                        if !bytes.is_empty() {
                            println!(
                                "[thumbnail] ffmpeg extracted {} bytes from: {}",
                                bytes.len(),
                                path.display()
                            );
                            return Some(bytes);
                        }
                    }
                } else {
                    // no-op: fall through to retry below
                }

                // Retry without -ss: catches both non-zero exit (seek past EOF) and
                // exit-0 with empty output (e.g. still images like HEIC where -ss produces nothing)
                let needs_retry = std::fs::metadata(&temp_out)
                    .map(|m| m.len() == 0)
                    .unwrap_or(true); // file missing → also retry

                if needs_retry {
                    let result2 = std::process::Command::new(ffmpeg)
                        .args([
                            "-i",
                            &path.to_string_lossy().to_string(),
                            "-vframes",
                            "1",
                            "-q:v",
                            "3",
                            "-update",
                            "1",
                            "-y",
                            &temp_out.to_string_lossy().to_string(),
                        ])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();

                    if let Ok(s2) = result2 {
                        if s2.success() {
                            if let Ok(bytes) = std::fs::read(&temp_out) {
                                let _ = std::fs::remove_file(&temp_out);
                                if !bytes.is_empty() {
                                    println!(
                                        "[thumbnail] ffmpeg (no-seek) extracted {} bytes from: {}",
                                        bytes.len(),
                                        path.display()
                                    );
                                    return Some(bytes);
                                }
                            }
                        }
                    }
                }
                // ffmpeg was found (no NotFound error), stop searching paths
                break;
            }
        }

        let _ = std::fs::remove_file(&temp_out);
        println!(
            "[thumbnail] ffmpeg could not extract frame from: {}",
            path.display()
        );
        None
    }

    /// Generates a thumbnail for a single file.
    /// Returns the thumbnail path on success.
    fn generate_single(source: &Path, cache_base_dir: &Path) -> Result<String, String> {
        let thumb_path = cache::thumbnail_path(source, cache_base_dir)?;

        // Check if cached thumbnail is still valid
        if thumb_path.exists() && !cache::is_stale(source, &thumb_path) {
            return Ok(thumb_path.to_string_lossy().to_string());
        }

        // Ensure cache directory exists
        cache::ensure_cache_dir(cache_base_dir)?;

        // Open and resize the image, ignoring file extension and inferring from magic bytes
        let img = Self::load_image(source)?;

        let (width, height) = (img.width(), img.height());

        let thumbnail = if width <= THUMBNAIL_SIZE && height <= THUMBNAIL_SIZE {
            img
        } else {
            img.thumbnail(THUMBNAIL_SIZE, THUMBNAIL_SIZE)
        };

        // Save as JPEG
        thumbnail
            .save(&thumb_path)
            .map_err(|e| format!("Failed to save thumbnail: {}", e))?;

        // Register in manifest for cleanup tracking
        cache::register_thumbnail(source, cache_base_dir)?;

        Ok(thumb_path.to_string_lossy().to_string())
    }

    /// Generates thumbnails for all media files in a directory.
    /// Emits `thumbnail-update` events to the frontend as each file is processed.
    pub async fn generate_for_dir(
        dir: String,
        session_id: u64,
        cache_base_dir: String,
        app_handle: AppHandle,
    ) -> Result<(), String> {
        let dir_path = Path::new(&dir);
        if !dir_path.is_dir() {
            return Err(format!("Not a directory: {}", dir));
        }

        // Read directory entries
        let entries: Vec<_> = std::fs::read_dir(dir_path)
            .map_err(|e| format!("Failed to read directory: {}", e))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .collect();

        let semaphore = Arc::new(Semaphore::new(MAX_WORKERS));
        let mut handles = Vec::new();

        for entry in entries {
            let path = entry.path();
            let app = app_handle.clone();
            let sem = semaphore.clone();
            let cache_base_dir_worker = cache_base_dir.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();

                let path_str = normalize_path(&path.to_string_lossy());

                if Self::is_video(&path) {
                    let cache_path =
                        cache::thumbnail_path(&path, Path::new(&cache_base_dir_worker));

                    if let Ok(tp) = cache_path {
                        if tp.exists() && !cache::is_stale(&path, &tp) {
                            let _ = app.emit(
                                "thumbnail-update",
                                ThumbnailUpdate {
                                    path: path_str,
                                    status: "ready".to_string(),
                                    thumbnail_path: Some(normalize_path(&tp.to_string_lossy())),
                                    session_id,
                                },
                            );
                            return;
                        }

                        // Try to extract embedded thumbnail from the container (e.g. iPhone Live Photo thmb track)
                        let cache_base = PathBuf::from(&cache_base_dir_worker);

                        // Helper closure to save raw bytes as a thumbnail and emit ready
                        let try_save = |thumb_bytes: Vec<u8>| -> bool {
                            if cache::ensure_cache_dir(&cache_base).is_ok() {
                                if std::fs::write(&tp, &thumb_bytes).is_ok() {
                                    let _ = cache::register_thumbnail(&path, &cache_base);
                                    return true;
                                }
                            }
                            false
                        };

                        // 1. Try embedded thumbnail track (e.g. QuickTime thmb for some MOV files)
                        let mut resolved_bytes: Option<Vec<u8>> =
                            Self::extract_embedded_video_thumbnail(&path);

                        // 2. Fallback: use system ffmpeg to extract a frame
                        if resolved_bytes.is_none() {
                            resolved_bytes = tokio::task::block_in_place(|| {
                                Self::extract_video_frame_ffmpeg(&path)
                            });
                        }

                        if let Some(thumb_bytes) = resolved_bytes {
                            if try_save(thumb_bytes) {
                                let _ = app.emit(
                                    "thumbnail-update",
                                    ThumbnailUpdate {
                                        path: path_str,
                                        status: "ready".to_string(),
                                        thumbnail_path: Some(normalize_path(&tp.to_string_lossy())),
                                        session_id,
                                    },
                                );
                                return;
                            }
                        }
                    }

                    // No embedded thumbnail found, tell frontend to render it
                    let _ = app.emit(
                        "thumbnail-update",
                        ThumbnailUpdate {
                            path: path_str,
                            status: "frontend-render".to_string(),
                            thumbnail_path: None,
                            session_id,
                        },
                    );
                    return;
                }

                // HEIC/HEIF: try embedded EXIF thumbnail first, then ffmpeg
                if Self::is_heic(&path) {
                    let cache_path =
                        cache::thumbnail_path(&path, Path::new(&cache_base_dir_worker));

                    if let Ok(tp) = cache_path {
                        if tp.exists() && !cache::is_stale(&path, &tp) {
                            let _ = app.emit(
                                "thumbnail-update",
                                ThumbnailUpdate {
                                    path: path_str,
                                    status: "ready".to_string(),
                                    thumbnail_path: Some(normalize_path(&tp.to_string_lossy())),
                                    session_id,
                                },
                            );
                            return;
                        }

                        let cache_base = PathBuf::from(&cache_base_dir_worker);

                        // 1. Try EXIF IFD1 embedded JPEG thumbnail
                        let mut resolved =
                            tokio::task::block_in_place(|| Self::extract_heic_thumbnail(&path));

                        // 2. Fallback to ffmpeg
                        if resolved.is_none() {
                            resolved = tokio::task::block_in_place(|| {
                                Self::extract_video_frame_ffmpeg(&path)
                            });
                        }

                        if let Some(thumb_bytes) = resolved {
                            if cache::ensure_cache_dir(&cache_base).is_ok()
                                && std::fs::write(&tp, &thumb_bytes).is_ok()
                            {
                                let _ = cache::register_thumbnail(&path, &cache_base);
                                let _ = app.emit(
                                    "thumbnail-update",
                                    ThumbnailUpdate {
                                        path: path_str,
                                        status: "ready".to_string(),
                                        thumbnail_path: Some(normalize_path(&tp.to_string_lossy())),
                                        session_id,
                                    },
                                );
                                return;
                            }
                        }
                    }

                    // No thumbnail could be generated
                    let _ = app.emit(
                        "thumbnail-update",
                        ThumbnailUpdate {
                            path: path_str,
                            status: "unsupported".to_string(),
                            thumbnail_path: None,
                            session_id,
                        },
                    );
                    return;
                }

                if !Self::is_supported(&path) {
                    let _ = app.emit(
                        "thumbnail-update",
                        ThumbnailUpdate {
                            path: path_str,
                            status: "unsupported".to_string(),
                            thumbnail_path: None,
                            session_id,
                        },
                    );
                    return;
                }

                // Run blocking image work off the async thread
                let result = tokio::task::spawn_blocking({
                    let path = path.clone();
                    let cache_base_dir_owned = PathBuf::from(cache_base_dir_worker);
                    move || Self::generate_single(&path, &cache_base_dir_owned)
                })
                .await;

                match result {
                    Ok(Ok(thumb_path)) => {
                        let _ = app.emit(
                            "thumbnail-update",
                            ThumbnailUpdate {
                                path: path_str,
                                status: "ready".to_string(),
                                thumbnail_path: Some(normalize_path(&thumb_path)),
                                session_id,
                            },
                        );
                    }
                    Ok(Err(err)) => {
                        eprintln!("Thumbnail error for {}: {}", path_str, err);
                        let _ = app.emit(
                            "thumbnail-update",
                            ThumbnailUpdate {
                                path: path_str,
                                status: "error".to_string(),
                                thumbnail_path: None,
                                session_id,
                            },
                        );
                    }
                    Err(err) => {
                        eprintln!("Task join error for {}: {}", path_str, err);
                        let _ = app.emit(
                            "thumbnail-update",
                            ThumbnailUpdate {
                                path: path_str,
                                status: "error".to_string(),
                                thumbnail_path: None,
                                session_id,
                            },
                        );
                    }
                }
            });

            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            let _ = handle.await;
        }

        Ok(())
    }

    /// Saves a base64 encoded thumbnail to the cache directory
    pub fn save_video_thumbnail(
        source_path: String,
        base64_data: String,
        cache_base_dir: String,
    ) -> Result<String, String> {
        let source = Path::new(&source_path);
        let cache_dir = Path::new(&cache_base_dir);

        cache::ensure_cache_dir(cache_dir)?;

        let thumb_path = cache::thumbnail_path(source, cache_dir)?;

        // Strip the data URI prefix if it exists (e.g. data:image/jpeg;base64,)
        let b64_contents = if let Some(idx) = base64_data.find(',') {
            &base64_data[idx + 1..]
        } else {
            &base64_data
        };

        use base64::{engine::general_purpose, Engine as _};
        let image_data = general_purpose::STANDARD
            .decode(b64_contents)
            .map_err(|e| format!("Failed to decode base64: {}", e))?;

        std::fs::write(&thumb_path, image_data)
            .map_err(|e| format!("Failed to write thumbnail file: {}", e))?;

        cache::register_thumbnail(source, cache_dir)?;

        Ok(thumb_path.to_string_lossy().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_is_supported_valid_extensions() {
        // Valid extensions
        assert!(ThumbnailService::is_supported(&PathBuf::from("image.jpg")));
        assert!(ThumbnailService::is_supported(&PathBuf::from("image.JPEG")));
        assert!(ThumbnailService::is_supported(&PathBuf::from("image.png")));
        assert!(ThumbnailService::is_supported(&PathBuf::from("photo.gif")));
        assert!(ThumbnailService::is_supported(&PathBuf::from("PIC.BMP")));
        assert!(ThumbnailService::is_supported(&PathBuf::from("test.webp")));
        assert!(ThumbnailService::is_supported(&PathBuf::from("test.tiff")));
        assert!(ThumbnailService::is_supported(&PathBuf::from("test.ico")));
    }

    #[test]
    fn test_is_supported_invalid_extensions() {
        // Invalid extensions
        assert!(!ThumbnailService::is_supported(&PathBuf::from("doc.txt")));
        assert!(!ThumbnailService::is_supported(&PathBuf::from("doc.pdf")));
        assert!(!ThumbnailService::is_supported(&PathBuf::from(
            "image.heic"
        ))); // Not currently in SUPPORTED_EXTENSIONS
        assert!(!ThumbnailService::is_supported(&PathBuf::from("video.mp4")));
    }

    #[test]
    fn test_is_supported_edge_cases() {
        // Edge cases
        assert!(!ThumbnailService::is_supported(&PathBuf::from(
            "no_extension_file"
        )));
        assert!(!ThumbnailService::is_supported(&PathBuf::from(
            ".hidden_no_ext"
        )));
    }

    fn test_generate_single_fixture(fixture_path: &str, test_name: &str) {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(format!("fixtures/{}", fixture_path));

        let cache_base_dir =
            std::env::temp_dir().join(format!("media_viewer_test_cache_{}", test_name));

        // Clean up previous test cache if it exists
        if cache_base_dir.exists() {
            let _ = std::fs::remove_dir_all(&cache_base_dir);
        }

        // Execute the function
        let result = ThumbnailService::generate_single(&d, &cache_base_dir);

        // Assertions
        assert!(
            result.is_ok(),
            "generate_single failed for {}: {:?}",
            d.display(),
            result.err()
        );

        let thumb_path_str = result.unwrap();
        let thumb_path = PathBuf::from(thumb_path_str);

        assert!(
            thumb_path.exists(),
            "Thumbnail file does not exist at expected path"
        );

        // Ensure that the background image matches its dimensions.
        let img = image::open(&thumb_path).expect("Failed to open generated thumbnail");
        assert!(
            img.width() <= super::THUMBNAIL_SIZE,
            "Thumbnail width exceeds maximum"
        );
        assert!(
            img.height() <= super::THUMBNAIL_SIZE,
            "Thumbnail height exceeds maximum"
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&cache_base_dir);
    }

    #[test]
    fn test_generate_single_jpg() {
        test_generate_single_fixture("file-examples.com/file_example_JPG_100kB.jpg", "jpg");
    }

    #[test]
    fn test_generate_single_png() {
        test_generate_single_fixture("file-examples.com/file_example_PNG_500kB.png", "png");
    }

    #[test]
    fn test_generate_single_gif() {
        test_generate_single_fixture("file-examples.com/file_example_GIF_500kB.gif", "gif");
    }

    #[test]
    fn test_generate_single_webp() {
        test_generate_single_fixture("file-examples.com/file_example_WEBP_250kB.webp", "webp");
    }

    #[test]
    fn test_generate_single_ico() {
        test_generate_single_fixture("file-examples.com/file_example_favicon.ico", "ico");
    }

    #[test]
    fn test_generate_single_small_image() {
        // Specifically test that we don't upscale a small file (e.g. 16x16 or 32x32 ico)
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("fixtures/file-examples.com/file_example_favicon.ico");

        let cache_base_dir = std::env::temp_dir().join("media_viewer_test_cache_small_image");

        // Clean up previous test cache if it exists
        if cache_base_dir.exists() {
            let _ = std::fs::remove_dir_all(&cache_base_dir);
        }

        // Execute the function
        let result = ThumbnailService::generate_single(&d, &cache_base_dir);
        assert!(
            result.is_ok(),
            "generate_single failed for {}: {:?}",
            d.display(),
            result.err()
        );

        let thumb_path_str = result.unwrap();
        let thumb_path = PathBuf::from(thumb_path_str);

        let original_img = ThumbnailService::load_image(&d).expect("Failed to open original image");
        let img = image::open(&thumb_path).expect("Failed to open generated thumbnail");

        assert_eq!(
            img.width(),
            original_img.width(),
            "Thumbnail width should equal original width for small images"
        );
        assert_eq!(
            img.height(),
            original_img.height(),
            "Thumbnail height should equal original height for small images"
        );

        let _ = std::fs::remove_dir_all(&cache_base_dir);
    }

    #[test]
    fn test_generate_single_magic_bytes() {
        // Test that infer handles files correctly even if extension is wrong
        // jpg-with-png-extension.png is actually a JPEG file
        test_generate_single_fixture(
            "problematic-files/jpg-with-png-extension.png",
            "magic_bytes",
        );
    }

    #[test]
    fn test_is_supported_real_files_magic_bytes() {
        // Test that infer handles files correctly even if extension is wrong
        // jpg_with_png_extension.png is actually a JPEG file
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("fixtures/problematic-files/jpg-with-png-extension.png");

        // This should be true because image crate detects it as a JPEG (image/jpeg)
        assert!(ThumbnailService::is_supported(&d));
    }

    // ---------------------------------------------------------------------------
    // Helpers shared by save_video_thumbnail tests
    // ---------------------------------------------------------------------------

    /// Returns a minimal but valid 1×1 JPEG as raw bytes.
    fn minimal_jpeg_bytes() -> Vec<u8> {
        vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06,
            0x07, 0x06, 0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D,
            0x0C, 0x0B, 0x0B, 0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D,
            0x1A, 0x1C, 0x1C, 0x20, 0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28,
            0x37, 0x29, 0x2C, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32,
            0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01,
            0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02,
            0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xDA, 0x00, 0x08, 0x01,
            0x01, 0x00, 0x00, 0x3F, 0x00, 0xFB, 0xD2, 0x8A, 0x28, 0x03, 0xFF, 0xD9,
        ]
    }

    fn minimal_jpeg_b64() -> String {
        use base64::{engine::general_purpose, Engine as _};
        general_purpose::STANDARD.encode(minimal_jpeg_bytes())
    }

    // ---------------------------------------------------------------------------
    // save_video_thumbnail — Happy path
    // ---------------------------------------------------------------------------

    /// #1 — Valid base64 with a data URI prefix (the common browser canvas output).
    #[test]
    fn test_save_video_thumbnail_with_data_uri_prefix() {
        use tempfile::tempdir;
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path().to_str().unwrap().to_string();
        let source_path = "/fake/video/clip.mp4".to_string();
        let base64_data = format!("data:image/jpeg;base64,{}", minimal_jpeg_b64());

        let result = ThumbnailService::save_video_thumbnail(
            source_path.clone(),
            base64_data,
            cache_dir.clone(),
        );

        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
        let thumb_path = PathBuf::from(result.unwrap());
        assert!(thumb_path.exists(), "Thumbnail file should exist on disk");
        let written = std::fs::read(&thumb_path).unwrap();
        assert_eq!(
            written,
            minimal_jpeg_bytes(),
            "File contents should match decoded JPEG bytes"
        );
    }

    /// #2 — Valid raw base64 without any data URI prefix.
    #[test]
    fn test_save_video_thumbnail_without_data_uri_prefix() {
        use tempfile::tempdir;
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path().to_str().unwrap().to_string();

        let result = ThumbnailService::save_video_thumbnail(
            "/fake/video/clip.mp4".to_string(),
            minimal_jpeg_b64(),
            cache_dir.clone(),
        );

        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
        let written = std::fs::read(result.unwrap()).unwrap();
        assert_eq!(written, minimal_jpeg_bytes());
    }

    /// #3 — Cache directory is created automatically when it doesn't exist yet.
    #[test]
    fn test_save_video_thumbnail_creates_cache_dir() {
        use tempfile::tempdir;
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path().join("new_cache_subdir");
        assert!(!cache_dir.exists(), "Pre-condition: dir must not exist");

        let result = ThumbnailService::save_video_thumbnail(
            "/fake/video/clip.mp4".to_string(),
            minimal_jpeg_b64(),
            cache_dir.to_str().unwrap().to_string(),
        );

        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
        assert!(
            cache_dir.exists(),
            "Cache directory should have been created"
        );
    }

    /// #4 — Manifest is updated after a successful save.
    #[test]
    fn test_save_video_thumbnail_registers_in_manifest() {
        use tempfile::tempdir;
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path().to_str().unwrap().to_string();

        ThumbnailService::save_video_thumbnail(
            "/fake/video/clip.mp4".to_string(),
            minimal_jpeg_b64(),
            cache_dir.clone(),
        )
        .expect("save should succeed");

        let manifest_path = tmp.path().join("manifest.json");
        assert!(manifest_path.exists(), "manifest.json should exist");
        let manifest_json = std::fs::read_to_string(&manifest_path).unwrap();
        assert!(
            manifest_json.contains("clip.mp4"),
            "Manifest should contain the source filename, got: {}",
            manifest_json
        );
    }

    /// #5 — A second call for the same source path overwrites the cached file.
    #[test]
    fn test_save_video_thumbnail_overwrites_existing() {
        use base64::{engine::general_purpose, Engine as _};
        use tempfile::tempdir;
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path().to_str().unwrap().to_string();
        let source_path = "/fake/video/clip.mp4".to_string();

        // First save — original JPEG bytes
        ThumbnailService::save_video_thumbnail(
            source_path.clone(),
            minimal_jpeg_b64(),
            cache_dir.clone(),
        )
        .expect("first save should succeed");

        // Second save — different (arbitrary) content
        let different_bytes = b"DIFFERENT_CONTENT".to_vec();
        let different_b64 = general_purpose::STANDARD.encode(&different_bytes);

        let result2 = ThumbnailService::save_video_thumbnail(
            source_path.clone(),
            different_b64,
            cache_dir.clone(),
        );

        assert!(result2.is_ok(), "Second save should succeed");
        let written = std::fs::read(result2.unwrap()).unwrap();
        assert_eq!(
            written, different_bytes,
            "Second save should overwrite with new content"
        );
    }

    // ---------------------------------------------------------------------------
    // save_video_thumbnail — Error paths
    // ---------------------------------------------------------------------------

    /// #6 — An invalid base64 payload produces an Err with a descriptive message.
    #[test]
    fn test_save_video_thumbnail_invalid_base64() {
        use tempfile::tempdir;
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path().to_str().unwrap().to_string();

        let result = ThumbnailService::save_video_thumbnail(
            "/fake/video/clip.mp4".to_string(),
            "data:image/jpeg;base64,!!!NOT_VALID_BASE64!!!".to_string(),
            cache_dir,
        );

        assert!(result.is_err(), "Expected Err for invalid base64");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("Failed to decode base64"),
            "Error message should mention base64 decode failure, got: {}",
            msg
        );
    }

    /// #7 — If cache_base_dir points to an existing file the call fails.
    #[test]
    fn test_save_video_thumbnail_cache_dir_is_file() {
        use tempfile::tempdir;
        let tmp = tempdir().unwrap();
        let file_path = tmp.path().join("not_a_dir");
        std::fs::write(&file_path, b"i am a file").unwrap();

        let result = ThumbnailService::save_video_thumbnail(
            "/fake/video/clip.mp4".to_string(),
            minimal_jpeg_b64(),
            file_path.to_str().unwrap().to_string(),
        );

        assert!(result.is_err(), "Expected Err when cache path is a file");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("not a directory"),
            "Error should mention 'not a directory', got: {}",
            msg
        );
    }

    // ---------------------------------------------------------------------------
    // save_video_thumbnail — Edge cases
    // ---------------------------------------------------------------------------

    /// #8 — Empty string decodes to zero bytes; the function writes an empty file.
    #[test]
    fn test_save_video_thumbnail_empty_base64() {
        use tempfile::tempdir;
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path().to_str().unwrap().to_string();

        let result = ThumbnailService::save_video_thumbnail(
            "/fake/video/clip.mp4".to_string(),
            "".to_string(),
            cache_dir,
        );

        // Empty string is valid base64 (yields no bytes) — write succeeds.
        assert!(
            result.is_ok(),
            "Expected Ok for empty base64, got: {:?}",
            result.err()
        );
        let written = std::fs::read(result.unwrap()).unwrap();
        assert!(written.is_empty(), "Written file should be zero bytes");
    }

    /// #9 — Data URI with an unusual MIME type is still stripped by the comma-split logic.
    #[test]
    fn test_save_video_thumbnail_alternative_mime_prefix() {
        use tempfile::tempdir;
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path().to_str().unwrap().to_string();
        let base64_data = format!("data:image/png;base64,{}", minimal_jpeg_b64());

        let result = ThumbnailService::save_video_thumbnail(
            "/fake/video/clip.mp4".to_string(),
            base64_data,
            cache_dir,
        );

        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
        let written = std::fs::read(result.unwrap()).unwrap();
        assert_eq!(
            written,
            minimal_jpeg_bytes(),
            "Content should be correct regardless of MIME type in prefix"
        );
    }

    /// #10 — Source paths with spaces and special characters work because the
    ///        cache key is a hash of the normalized path, never the path itself.
    #[test]
    fn test_save_video_thumbnail_special_chars_in_source_path() {
        use tempfile::tempdir;
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path().to_str().unwrap().to_string();
        let source_path = "/my videos/Vacación (2024)/clip #1.mp4".to_string();

        let result =
            ThumbnailService::save_video_thumbnail(source_path, minimal_jpeg_b64(), cache_dir);

        assert!(
            result.is_ok(),
            "Expected Ok for path with special chars, got: {:?}",
            result.err()
        );
        assert!(PathBuf::from(result.unwrap()).exists());
    }

    /// #11 — The returned path string is a child of cache_base_dir.
    #[test]
    fn test_save_video_thumbnail_returned_path_inside_cache_dir() {
        use tempfile::tempdir;
        let tmp = tempdir().unwrap();
        let cache_dir = tmp.path().to_str().unwrap().to_string();

        let result = ThumbnailService::save_video_thumbnail(
            "/fake/video/clip.mp4".to_string(),
            minimal_jpeg_b64(),
            cache_dir.clone(),
        );

        assert!(result.is_ok(), "Expected Ok, got: {:?}", result.err());
        let returned = result.unwrap();
        assert!(
            returned.starts_with(&cache_dir),
            "Returned path '{}' should be inside cache_dir '{}'",
            returned,
            cache_dir
        );
    }
}
