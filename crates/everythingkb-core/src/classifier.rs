use std::path::Path;

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Document,
    LongDocument,
    /// Image summarized via multimodal LLM (`image = true`).
    Image,
    /// Video/audio metadata + optional software user-profile paths.
    UserProfile,
    Skip,
}

/// Extensions we can convert today: mdkit (pdf, calamine, csv, html) + undocx + plain text.
pub const DOCUMENT_EXTENSIONS: &[&str] = &[
    "md", "markdown", "txt", "pdf", "docx", "doc", "xlsx", "xls", "csv", "html", "htm",
];

pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "webp", "heic", "avif",
];

pub const MEDIA_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "mp3", "wav", "flac", "ogg",
];

/// Paths that look like per-app user data (saves, configs, mods, userdata).
const USER_PROFILE_MARKERS: &[&str] = &[
    "/Saves/",
    "/save/",
    "/Save/",
    "/userdata/",
    "/Mods/",
    "/mods/",
    "/.config/",
    "/AppData/",
    "/Application Support/",
];

pub fn is_document_extension(ext: &str) -> bool {
    DOCUMENT_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

pub fn is_image_extension(ext: &str) -> bool {
    IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

pub fn is_media_extension(ext: &str) -> bool {
    MEDIA_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

pub fn should_scan_file(path: &Path, config: &Config) -> bool {
    if path.is_dir() {
        return false;
    }
    if config.index_user_profiles {
        return true;
    }
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    is_document_extension(ext)
        || (config.image && is_image_extension(ext))
        || (config.index_media && is_media_extension(ext))
}

pub fn classify(path: &Path, config: &Config) -> FileKind {
    if path.is_dir() {
        return FileKind::Skip;
    }

    let name = path.to_string_lossy();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if is_document_extension(&ext) {
        if ext == "pdf" && pdf_page_count(path) >= config.pageindex_threshold {
            return FileKind::LongDocument;
        }
        return FileKind::Document;
    }

    if config.image && is_image_extension(&ext) {
        return FileKind::Image;
    }

    if config.index_media && is_media_extension(&ext) {
        return FileKind::UserProfile;
    }

    if config.index_user_profiles && is_profile_path(&name) {
        return FileKind::UserProfile;
    }

    FileKind::Skip
}

fn is_profile_path(name: &str) -> bool {
    USER_PROFILE_MARKERS.iter().any(|m| name.contains(m))
}

fn pdf_page_count(path: &Path) -> u32 {
    // ponytail: pdfium only; pdf-extract panics on some encodings (PDFDocEncoding)
    crate::convert::pdf_page_count(path).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn classifies_markdown() {
        let dir = std::env::temp_dir().join("everythingkb_test");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("sample.md");
        fs::write(&path, "# hi").unwrap();
        assert_eq!(classify(&path, &Config::default()), FileKind::Document);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn skips_media_by_default() {
        assert_eq!(
            classify(Path::new("/home/u/photo.jpg"), &Config::default()),
            FileKind::Skip
        );
        assert_eq!(
            classify(Path::new("/home/u/clip.mp4"), &Config::default()),
            FileKind::Skip
        );
    }

    #[test]
    fn indexes_images_when_enabled() {
        let mut config = Config::default();
        config.image = true;
        assert_eq!(
            classify(Path::new("/home/u/photo.jpg"), &config),
            FileKind::Image
        );
    }

    #[test]
    fn indexes_av_media_when_enabled() {
        let mut config = Config::default();
        config.index_media = true;
        assert_eq!(
            classify(Path::new("/home/u/clip.mp4"), &config),
            FileKind::UserProfile
        );
    }

    #[test]
    fn skips_profile_paths_by_default() {
        assert_eq!(
            classify(
                Path::new("/home/u/.steam/userdata/1/2/remote/save.sav"),
                &Config::default()
            ),
            FileKind::Skip
        );
        assert_eq!(
            classify(
                Path::new("/home/u/.config/myapp/settings.json"),
                &Config::default()
            ),
            FileKind::Skip
        );
    }

    #[test]
    fn indexes_profile_paths_when_enabled() {
        let mut config = Config::default();
        config.index_user_profiles = true;
        assert_eq!(
            classify(
                Path::new("/home/u/.config/myapp/settings.json"),
                &config
            ),
            FileKind::UserProfile
        );
    }

    #[test]
    fn scan_skips_media_by_default() {
        let config = Config::default();
        assert!(should_scan_file(Path::new("/a/b/note.pdf"), &config));
        assert!(!should_scan_file(Path::new("/a/b/photo.jpg"), &config));
    }
}
