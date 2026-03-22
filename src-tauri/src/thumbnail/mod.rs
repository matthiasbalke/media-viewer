pub mod cache;
pub mod service;

pub use cache::{cleanup_for_prefix, cleanup_orphans, delete_all};
pub use service::ThumbnailService;

/// Normalizes a file path to use forward slashes.
/// This ensures consistent paths across platforms.
pub(crate) fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_unix() {
        // Unix style path remains unchanged
        assert_eq!(normalize_path("/foo/bar/image.jpg"), "/foo/bar/image.jpg");
    }

    #[test]
    fn test_normalize_path_windows() {
        // Windows style path is converted
        assert_eq!(
            normalize_path("C:\\foo\\bar\\image.jpg"),
            "C:/foo/bar/image.jpg"
        );
    }

    #[test]
    fn test_normalize_path_mixed() {
        // Mixed style
        assert_eq!(
            normalize_path("C:\\foo\\bar/image.jpg"),
            "C:/foo/bar/image.jpg"
        );
    }
}
