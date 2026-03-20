use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[cfg(test)]
use std::cell::RefCell;

static MANIFEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
thread_local! {
    static TEST_CACHE_DIR: RefCell<Option<PathBuf>> = RefCell::new(None);
}

/// Returns the path to the manifest file.
fn manifest_path(cache_base_dir: &Path) -> PathBuf {
    cache_base_dir.join("manifest.json")
}

/// Computes the hash string for a source path.
/// Normalizes the path first to ensure consistent hashes across platforms.
fn hash_for_path(source: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    super::normalize_path(&source.to_string_lossy()).hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Returns the path to the thumbnail for a given source file.
/// Format: <cache_base_dir>/<hash>.jpg
pub fn thumbnail_path(source: &Path, cache_base_dir: &Path) -> Result<PathBuf, String> {
    let hash = hash_for_path(source);
    Ok(cache_base_dir.join(format!("{}.jpg", hash)))
}

fn get_canonicalized_path(user_provided_path: &Path) -> Result<PathBuf, String> {
    let canonical = user_provided_path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path: {}", e))?;

    // Verify the canonical path is within an allowed base directory if needed
    Ok(canonical)
}

/// Returns true if the thumbnail is stale (source was modified after the thumbnail).
pub fn is_stale(source: &Path, thumbnail: &Path) -> bool {
    let source_mtime = match get_canonicalized_path(source)
        .ok()
        .and_then(|p| fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
    {
        Some(t) => t,
        None => return true, // Can't read source → treat as stale
    };

    let thumb_mtime = match get_canonicalized_path(thumbnail)
        .ok()
        .and_then(|p| fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
    {
        Some(t) => t,
        None => return true, // Thumbnail doesn't exist → stale
    };

    source_mtime > thumb_mtime
}

/// Creates the base cache directory if it doesn't exist.
pub fn ensure_cache_dir(cache_base_dir: &Path) -> Result<PathBuf, String> {
    if cache_base_dir.exists() {
        if !cache_base_dir.is_dir() {
            return Err(format!(
                "Cache path exists but is not a directory: {}",
                cache_base_dir.display()
            ));
        }
    } else {
        fs::create_dir_all(cache_base_dir).map_err(|e| {
            format!(
                "Failed to create cache directory {}: {}",
                cache_base_dir.display(),
                e
            )
        })?;
    }

    // Verify we can write to the directory
    let test_file = cache_base_dir.join(".write_test");
    fs::write(&test_file, b"").map_err(|e| {
        format!(
            "Cache directory is not writable {}: {}",
            cache_base_dir.display(),
            e
        )
    })?;
    let _ = fs::remove_file(&test_file);

    Ok(cache_base_dir.to_path_buf())
}

// --- Manifest management ---

/// Loads the manifest (hash → source_path).
fn load_manifest(cache_base_dir: &Path) -> Result<HashMap<String, String>, String> {
    let path = manifest_path(cache_base_dir);
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("Failed to read manifest: {}", e))?;
    serde_json::from_str(&data).map_err(|e| format!("Failed to parse manifest: {}", e))
}

/// Saves the manifest to disk.
fn save_manifest(manifest: &HashMap<String, String>, cache_base_dir: &Path) -> Result<(), String> {
    let path = manifest_path(cache_base_dir);
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create manifest directory: {}", e))?;
    }
    let data = serde_json::to_string_pretty(manifest)
        .map_err(|e| format!("Failed to serialize manifest: {}", e))?;
    fs::write(&path, data).map_err(|e| format!("Failed to write manifest: {}", e))
}

/// Registers a thumbnail in the manifest after generation.
pub fn register_thumbnail(source: &Path, cache_base_dir: &Path) -> Result<(), String> {
    let _lock = MANIFEST_LOCK
        .lock()
        .map_err(|e| format!("Manifest lock error: {}", e))?;
    let hash = hash_for_path(source);
    let mut manifest = load_manifest(cache_base_dir)?;
    manifest.insert(hash, super::normalize_path(&source.to_string_lossy()));
    save_manifest(&manifest, cache_base_dir)
}

/// Deletes all thumbnails whose source path starts with the given prefix.
/// Used when a root directory is removed.
pub fn cleanup_for_prefix(prefix: &str, cache_base_dir: &str) -> Result<u32, String> {
    let _lock = MANIFEST_LOCK
        .lock()
        .map_err(|e| format!("Manifest lock error: {}", e))?;
    let base = Path::new(cache_base_dir);
    let mut manifest = load_manifest(base)?;

    let to_remove: Vec<String> = manifest
        .iter()
        .filter(|(_, source)| source.starts_with(prefix))
        .map(|(hash, _)| hash.clone())
        .collect();

    let mut removed = 0u32;
    for hash in &to_remove {
        let thumb = base.join(format!("{}.jpg", hash));
        if thumb.exists() {
            let _ = fs::remove_file(&thumb);
        }
        manifest.remove(hash);
        removed += 1;
    }

    save_manifest(&manifest, base)?;
    Ok(removed)
}

/// Scans the manifest and deletes entries whose source file no longer exists.
pub fn cleanup_orphans(cache_base_dir: &str) -> Result<u32, String> {
    let _lock = MANIFEST_LOCK
        .lock()
        .map_err(|e| format!("Manifest lock error: {}", e))?;
    let base = Path::new(cache_base_dir);
    let mut manifest = load_manifest(base)?;

    let orphans: Vec<String> = manifest
        .iter()
        .filter(|(_, source)| !Path::new(source).exists())
        .map(|(hash, _)| hash.clone())
        .collect();

    let mut removed = 0u32;
    for hash in &orphans {
        let thumb = base.join(format!("{}.jpg", hash));
        if thumb.exists() {
            let _ = fs::remove_file(&thumb);
        }
        manifest.remove(hash);
        removed += 1;
    }

    save_manifest(&manifest, base)?;
    Ok(removed)
}

/// Deletes the entire thumbnail cache directory.
pub fn delete_all(cache_base_dir: &str) -> Result<(), String> {
    let _lock = MANIFEST_LOCK
        .lock()
        .map_err(|e| format!("Manifest lock error: {}", e))?;
    let base = Path::new(cache_base_dir);
    if base.exists() {
        fs::remove_dir_all(base).map_err(|e| format!("Failed to delete cache dir: {}", e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;

    /// Helper function to create an isolated test environment
    struct TestEnvGuard {
        pub temp_dir: tempfile::TempDir,
    }

    fn setup_test_env() -> TestEnvGuard {
        let temp_dir = tempdir().expect("Failed to create temp test directory");
        TestEnvGuard { temp_dir }
    }

    #[test]
    fn test_hash_for_path_deterministic() {
        // Same logical path should yield same hash
        let hash1 = hash_for_path(&PathBuf::from("/foo/bar/image.jpg"));
        let hash2 = hash_for_path(&PathBuf::from("/foo/bar/image.jpg"));
        assert_eq!(hash1, hash2, "Hashes should be deterministic");
    }

    #[test]
    fn test_hash_for_path_different_files() {
        // Different files yield different hashes
        let hash1 = hash_for_path(&PathBuf::from("/foo/bar/image.jpg"));
        let hash3 = hash_for_path(&PathBuf::from("/foo/bar/other.jpg"));
        assert_ne!(hash1, hash3, "Different paths should have different hashes");
    }

    #[test]
    fn test_hash_for_path_cross_platform() {
        // Cross-platform logic (Windows vs Unix slash)
        let hash_win = hash_for_path(&PathBuf::from("C:\\foo\\image.jpg"));
        let hash_unix = hash_for_path(&PathBuf::from("C:/foo/image.jpg"));
        assert_eq!(
            hash_win, hash_unix,
            "Path normalization should ensure identical hashes"
        );
    }

    #[test]
    fn test_ensure_cache_dir_creates_directory() {
        let env = setup_test_env();

        let cache_dir = ensure_cache_dir(env.temp_dir.path()).expect("Failed to create cache dir");

        assert!(cache_dir.exists());
        assert!(cache_dir.is_dir());
    }

    #[test]
    fn test_ensure_cache_dir_already_exists() {
        let env = setup_test_env();

        // Manually create the directory first
        std::fs::create_dir_all(env.temp_dir.path()).unwrap();

        // Should return ok without errors
        let cache_dir =
            ensure_cache_dir(env.temp_dir.path()).expect("Failed to ensure existing cache dir");

        assert!(cache_dir.exists());
        assert!(cache_dir.is_dir());
    }

    #[test]
    fn test_ensure_cache_dir_fails_if_file_exists() {
        let env = setup_test_env();
        let file_path = env.temp_dir.path().join("fake_dir");

        // Create a file at the location where the directory should be
        std::fs::write(&file_path, "not a directory").unwrap();

        // This should fail because a file exists at that path
        let result = ensure_cache_dir(&file_path);

        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("Cache path exists but is not a directory"));
    }

    #[test]
    fn test_manifest_starts_empty() {
        let env = setup_test_env();

        // Initially empty
        let initial_manifest = load_manifest(env.temp_dir.path()).unwrap();
        assert!(initial_manifest.is_empty(), "Manifest should start empty");
    }

    #[test]
    fn test_register_thumbnail_adds_to_manifest() {
        let env = setup_test_env();

        // Add an entry
        let test_path = PathBuf::from("/test/source/image.jpg");
        register_thumbnail(&test_path, env.temp_dir.path()).expect("Failed to register thumbnail");

        // Load and verify
        let updated_manifest = load_manifest(env.temp_dir.path()).unwrap();
        assert_eq!(updated_manifest.len(), 1);

        let hash = hash_for_path(&test_path);
        assert_eq!(
            updated_manifest.get(&hash).unwrap(),
            "/test/source/image.jpg"
        );
    }

    #[test]
    fn test_is_stale_when_thumbnail_missing() {
        let _env = setup_test_env();
        let base_dir = _env.temp_dir.path();

        let source_path = base_dir.join("source.jpg");
        let thumb_path = base_dir.join("thumb.jpg");

        // Create source file
        File::create(&source_path).unwrap();

        // 1. Thumbnail missing -> should be stale
        assert!(
            is_stale(&source_path, &thumb_path),
            "Missing thumbnail should be stale"
        );
    }

    #[test]
    fn test_is_stale_when_thumbnail_newer() {
        let _env = setup_test_env();
        let base_dir = _env.temp_dir.path();

        let source_path = base_dir.join("source.jpg");
        let thumb_path = base_dir.join("thumb.jpg");

        // Create source file
        File::create(&source_path).unwrap();

        // Create thumbnail file immediately
        // (Wait slightly to ensure mtime ticks forward for older filesystems)
        thread::sleep(Duration::from_millis(50));
        File::create(&thumb_path).unwrap();

        // 2. Thumbnail newer than source -> not stale
        assert!(
            !is_stale(&source_path, &thumb_path),
            "Newer thumbnail should not be stale"
        );
    }

    #[test]
    fn test_is_stale_when_source_newer() {
        let _env = setup_test_env();
        let base_dir = _env.temp_dir.path();

        let source_path = base_dir.join("source.jpg");
        let thumb_path = base_dir.join("thumb.jpg");

        // Create source file
        File::create(&source_path).unwrap();

        // Create thumbnail file immediately
        thread::sleep(Duration::from_millis(50));
        File::create(&thumb_path).unwrap();

        // Update source file to make it newer
        thread::sleep(Duration::from_millis(50));
        fs::write(&source_path, b"updated").unwrap();

        // 3. Source newer than thumbnail -> stale
        assert!(
            is_stale(&source_path, &thumb_path),
            "Newer source should make thumbnail stale"
        );
    }

    // Must remove MV_TEST_CACHE_DIR after tests to avoid cross-contamination in other threads,
    // though `cargo test` runs in parallel, which makes full env var isolation tricky.
    // Usually tests run locally will be fine.

    // ---------------------------------------------------------------------------
    // thumbnail_path
    // ---------------------------------------------------------------------------

    #[test]
    fn test_thumbnail_path_deterministic() {
        let env = setup_test_env();
        let source = PathBuf::from("/photos/image.jpg");
        let path1 = thumbnail_path(&source, env.temp_dir.path()).unwrap();
        let path2 = thumbnail_path(&source, env.temp_dir.path()).unwrap();
        assert_eq!(path1, path2, "thumbnail_path must be deterministic");
    }

    #[test]
    fn test_thumbnail_path_inside_cache_dir() {
        let env = setup_test_env();
        let source = PathBuf::from("/photos/image.jpg");
        let path = thumbnail_path(&source, env.temp_dir.path()).unwrap();
        assert!(
            path.starts_with(env.temp_dir.path()),
            "thumbnail path should be inside the cache dir"
        );
    }

    #[test]
    fn test_thumbnail_path_ends_with_jpg() {
        let env = setup_test_env();
        let source = PathBuf::from("/photos/image.jpg");
        let path = thumbnail_path(&source, env.temp_dir.path()).unwrap();
        assert_eq!(
            path.extension().and_then(|e| e.to_str()),
            Some("jpg"),
            "thumbnail filename should end with .jpg"
        );
    }

    #[test]
    fn test_thumbnail_path_different_sources_differ() {
        let env = setup_test_env();
        let path_a = thumbnail_path(&PathBuf::from("/photos/a.jpg"), env.temp_dir.path()).unwrap();
        let path_b = thumbnail_path(&PathBuf::from("/photos/b.jpg"), env.temp_dir.path()).unwrap();
        assert_ne!(path_a, path_b, "different source paths must yield different thumbnail paths");
    }

    // ---------------------------------------------------------------------------
    // cleanup_for_prefix
    // ---------------------------------------------------------------------------

    #[test]
    fn test_cleanup_for_prefix_removes_matching_entries_and_files() {
        let env = setup_test_env();
        let cache_dir = env.temp_dir.path();
        let cache_dir_str = cache_dir.to_str().unwrap().to_string();

        let path_a = PathBuf::from("/photos/vacation/img1.jpg");
        let path_b = PathBuf::from("/photos/vacation/img2.jpg");
        let path_c = PathBuf::from("/documents/scan1.jpg");

        register_thumbnail(&path_a, cache_dir).unwrap();
        register_thumbnail(&path_b, cache_dir).unwrap();
        register_thumbnail(&path_c, cache_dir).unwrap();

        let thumb_a = thumbnail_path(&path_a, cache_dir).unwrap();
        let thumb_b = thumbnail_path(&path_b, cache_dir).unwrap();
        let thumb_c = thumbnail_path(&path_c, cache_dir).unwrap();
        std::fs::write(&thumb_a, b"fake").unwrap();
        std::fs::write(&thumb_b, b"fake").unwrap();
        std::fs::write(&thumb_c, b"fake").unwrap();

        let removed = cleanup_for_prefix("/photos/vacation", &cache_dir_str).unwrap();

        assert_eq!(removed, 2, "two matching entries should be removed");
        assert!(!thumb_a.exists(), "matching thumbnail A should be deleted from disk");
        assert!(!thumb_b.exists(), "matching thumbnail B should be deleted from disk");
        assert!(thumb_c.exists(), "non-matching thumbnail C should remain on disk");

        let manifest = load_manifest(cache_dir).unwrap();
        assert!(
            !manifest.values().any(|v| v.starts_with("/photos/vacation")),
            "manifest should not contain the removed prefix"
        );
        assert!(
            manifest.values().any(|v| v.contains("scan1")),
            "manifest should still contain the non-matching entry"
        );
    }

    #[test]
    fn test_cleanup_for_prefix_no_matches_returns_zero() {
        let env = setup_test_env();
        let cache_dir = env.temp_dir.path();
        let cache_dir_str = cache_dir.to_str().unwrap().to_string();

        register_thumbnail(&PathBuf::from("/photos/img.jpg"), cache_dir).unwrap();

        let removed = cleanup_for_prefix("/videos", &cache_dir_str).unwrap();

        assert_eq!(removed, 0, "no entries should be removed when prefix has no match");
        let manifest = load_manifest(cache_dir).unwrap();
        assert_eq!(manifest.len(), 1, "manifest should be unchanged");
    }

    // ---------------------------------------------------------------------------
    // cleanup_orphans
    // ---------------------------------------------------------------------------

    #[test]
    fn test_cleanup_orphans_removes_entries_with_missing_source() {
        let env = setup_test_env();
        let cache_dir = env.temp_dir.path();
        let cache_dir_str = cache_dir.to_str().unwrap().to_string();

        // A real file that exists on disk
        let existing_source = cache_dir.join("real_image.jpg");
        std::fs::write(&existing_source, b"fake jpg").unwrap();
        register_thumbnail(&existing_source, cache_dir).unwrap();
        let thumb_existing = thumbnail_path(&existing_source, cache_dir).unwrap();
        std::fs::write(&thumb_existing, b"fake thumb").unwrap();

        // A source path that does NOT exist on disk
        let ghost_source = PathBuf::from("/ghost/nonexistent/photo.jpg");
        register_thumbnail(&ghost_source, cache_dir).unwrap();
        let thumb_ghost = thumbnail_path(&ghost_source, cache_dir).unwrap();
        std::fs::write(&thumb_ghost, b"fake thumb").unwrap();

        let removed = cleanup_orphans(&cache_dir_str).unwrap();

        assert_eq!(removed, 1, "one orphan should be removed");
        assert!(!thumb_ghost.exists(), "orphan thumbnail should be deleted from disk");
        assert!(thumb_existing.exists(), "thumbnail for existing source should remain");

        let manifest = load_manifest(cache_dir).unwrap();
        assert_eq!(manifest.len(), 1, "only the valid entry should remain in manifest");
    }

    #[test]
    fn test_cleanup_orphans_keeps_entries_with_existing_source() {
        let env = setup_test_env();
        let cache_dir = env.temp_dir.path();
        let cache_dir_str = cache_dir.to_str().unwrap().to_string();

        let source = cache_dir.join("photo.jpg");
        std::fs::write(&source, b"fake jpg").unwrap();
        register_thumbnail(&source, cache_dir).unwrap();

        let removed = cleanup_orphans(&cache_dir_str).unwrap();

        assert_eq!(removed, 0, "no entries should be removed for existing sources");
        let manifest = load_manifest(cache_dir).unwrap();
        assert_eq!(manifest.len(), 1, "manifest should be unchanged");
    }

    // ---------------------------------------------------------------------------
    // delete_all
    // ---------------------------------------------------------------------------

    #[test]
    fn test_delete_all_removes_cache_directory() {
        let env = setup_test_env();
        let cache_dir = env.temp_dir.path().join("thumbnails");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("abc123.jpg"), b"fake thumb").unwrap();
        std::fs::write(cache_dir.join("manifest.json"), b"{}").unwrap();

        delete_all(cache_dir.to_str().unwrap()).unwrap();

        assert!(!cache_dir.exists(), "cache directory should be completely removed");
    }

    #[test]
    fn test_delete_all_idempotent_when_dir_missing() {
        let env = setup_test_env();
        let cache_dir = env.temp_dir.path().join("nonexistent");

        let result = delete_all(cache_dir.to_str().unwrap());

        assert!(result.is_ok(), "delete_all should succeed even if directory does not exist");
    }
}
