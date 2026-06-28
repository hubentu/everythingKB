use std::io::BufReader;
use std::path::Path;

use anyhow::Result;
use exif::{In, Reader, Tag};

pub fn profile_stub(path: &Path) -> Result<String> {
    let meta = std::fs::metadata(path)?;
    let mime = infer::get_from_path(path)
        .ok()
        .flatten()
        .map(|k| k.mime_type().to_string())
        .unwrap_or_else(|| "application/octet-stream".into());

    let mut lines = vec![
        format!(
            "# {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ),
        format!("- path: {}", path.display()),
        format!("- type: {mime}"),
        format!("- size: {} bytes", meta.len()),
    ];

    if let Some(parent) = path.parent().and_then(|p| p.file_name()) {
        lines.push(format!("- parent: {}", parent.to_string_lossy()));
    }

    if mime.starts_with("image/") {
        if let Ok(file) = std::fs::File::open(path) {
            let mut buf = BufReader::new(file);
            if let Ok(exif) = Reader::new().read_from_container(&mut buf) {
                append_exif(&exif, &mut lines);
            }
        }
    }

    if mime.starts_with("video/") || mime.starts_with("audio/") {
        if let Some(probe) = ffprobe_metadata(path) {
            lines.push(format!("- duration: {}", probe.0));
            if let Some(codec) = probe.1 {
                lines.push(format!("- codec: {codec}"));
            }
        }
    }

    lines.push(String::new());
    lines.push("Indexed as user-profile metadata stub.".into());
    Ok(lines.join("\n"))
}

fn append_exif(exif: &exif::Exif, lines: &mut Vec<String>) {
    for tag in [Tag::DateTime, Tag::Make, Tag::Model, Tag::GPSLatitude, Tag::GPSLongitude] {
        if let Some(field) = exif.get_field(tag, In::PRIMARY) {
            lines.push(format!("- {}: {}", tag, field.display_value().with_unit(exif)));
        }
    }
}

fn ffprobe_metadata(path: &Path) -> Option<(String, Option<String>)> {
    use std::process::Command;
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            path.to_str()?,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let duration = v["format"]["duration"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    let codec = v["streams"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|s| s["codec_name"].as_str())
        .map(String::from);
    Some((duration, codec))
}
