//! Public catalog of host installers and other downloadable EdgeFabric bits.

use axum::{
    Json,
    extract::Path,
    http::{HeaderValue, StatusCode, header},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::path::PathBuf;

use crate::middleware::error_response::ApiError;

#[derive(Clone, Serialize)]
struct ResourceItem {
    id: String,
    name: String,
    kind: String,
    platform: String,
    arch: String,
    filename: String,
    summary: String,
    install_hint: String,
    available: bool,
    size_bytes: Option<u64>,
    download_path: String,
}

fn catalog_spec() -> Vec<ResourceItem> {
    vec![
        item(
            "host-macos-arm64",
            "Host worker — macOS Apple Silicon",
            "host-package",
            "macos",
            "aarch64",
            "edgefabric-host-aarch64-apple-darwin.tar.gz",
            "Install on a Mac mini or MacBook: OpenSSH, VeerSetu overlay, and `ef host join`.",
            "tar xzf edgefabric-host-aarch64-apple-darwin.tar.gz && export EF_API_KEY=… && ./install.sh",
        ),
        item(
            "host-linux-x64",
            "Host worker — Linux x86_64",
            "host-package",
            "linux",
            "x86_64",
            "edgefabric-host-x86_64-unknown-linux-gnu.tar.gz",
            "Same generic SSH worker for Linux servers (builds, deploy targets, ML).",
            "tar xzf edgefabric-host-x86_64-unknown-linux-gnu.tar.gz && export EF_API_KEY=… && ./install.sh",
        ),
        item(
            "host-linux-arm64",
            "Host worker — Linux ARM64",
            "host-package",
            "linux",
            "aarch64",
            "edgefabric-host-aarch64-unknown-linux-gnu.tar.gz",
            "ARM Linux servers and cloud instances.",
            "tar xzf edgefabric-host-aarch64-unknown-linux-gnu.tar.gz && export EF_API_KEY=… && ./install.sh",
        ),
        item(
            "host-windows-x64",
            "Host worker — Windows x64",
            "host-package",
            "windows",
            "x86_64",
            "edgefabric-host-x86_64-pc-windows-msvc.zip",
            "Windows OpenSSH Server + `ef host join`. Build with the MSVC target.",
            "Expand the zip, add bin to PATH, then: ef --server https://edgefabric.veerlabs.solutions host join --api-key …",
        ),
    ]
}

fn item(
    id: &str,
    name: &str,
    kind: &str,
    platform: &str,
    arch: &str,
    filename: &str,
    summary: &str,
    install_hint: &str,
) -> ResourceItem {
    ResourceItem {
        id: id.into(),
        name: name.into(),
        kind: kind.into(),
        platform: platform.into(),
        arch: arch.into(),
        filename: filename.into(),
        summary: summary.into(),
        install_hint: install_hint.into(),
        available: false,
        size_bytes: None,
        download_path: format!("/api/v1/resources/files/{filename}"),
    }
}

fn resource_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(p) = std::env::var("EF_RESOURCES_DIR") {
        dirs.push(PathBuf::from(p));
    }
    dirs.push(PathBuf::from("/var/www/edgefabric-dashboard/downloads"));
    dirs.push(PathBuf::from("dist/host"));
    dirs.push(PathBuf::from("/opt/edgefabric/dist/host"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.join("resources"));
            dirs.push(parent.join("../../dist/host"));
        }
    }
    dirs
}

fn find_file(filename: &str) -> Option<PathBuf> {
    for dir in resource_dirs() {
        let candidate = dir.join(filename);
        if candidate.is_file() {
            return Some(candidate);
        }
        // packaged layout: dist/host/<triple>/ is not used; tarball sits next to triples
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.file_name().and_then(|n| n.to_str()) == Some(filename) {
                    return Some(path);
                }
            }
        }
    }
    None
}

fn is_safe_filename(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 160
        && !name.contains("..")
        && !name.contains('/')
        && !name.contains('\\')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

pub async fn list_resources() -> Json<Value> {
    let mut items = catalog_spec();
    for item in &mut items {
        if let Some(path) = find_file(&item.filename) {
            item.available = true;
            item.size_bytes = std::fs::metadata(&path).ok().map(|m| m.len());
        }
    }

    let extra = extra_files(&items);
    items.extend(extra);

    Json(json!({
        "resources": items,
        "join": {
            "macos_linux": "ef --server https://edgefabric.veerlabs.solutions host join --api-key <tenant-key> [--with-ml]",
            "veersetu": "veersetu-agent publishes local :22 as service ssh-<hostname>",
        }
    }))
}

fn extra_files(known: &[ResourceItem]) -> Vec<ResourceItem> {
    let known_names: Vec<&str> = known.iter().map(|i| i.filename.as_str()).collect();
    let mut extras = Vec::new();
    for dir in resource_dirs() {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !path.is_file() {
                continue;
            }
            if !(name.starts_with("edgefabric-host-") && (name.ends_with(".tar.gz") || name.ends_with(".zip")))
            {
                continue;
            }
            if known_names.iter().any(|k| *k == name) || extras.iter().any(|e: &ResourceItem| e.filename == name)
            {
                continue;
            }
            extras.push(ResourceItem {
                id: format!("host-{name}"),
                name: format!("Host worker — {name}"),
                kind: "host-package".into(),
                platform: "unknown".into(),
                arch: "unknown".into(),
                filename: name.to_string(),
                summary: "Packaged host worker for this architecture.".into(),
                install_hint: format!("Unpack {name} and run install.sh (or ef host join)."),
                available: true,
                size_bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
                download_path: format!("/api/v1/resources/files/{name}"),
            });
        }
    }
    extras
}

pub async fn download_resource(
    Path(name): Path<String>,
) -> Result<([(header::HeaderName, HeaderValue); 2], Vec<u8>), (StatusCode, Json<ApiError>)> {
    if !is_safe_filename(&name) {
        return Err(ApiError::bad_request("invalid filename"));
    }
    let path = find_file(&name).ok_or_else(|| ApiError::not_found(format!("{name} is not published yet")))?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to read {name}: {e}")))?;
    let ctype = if name.ends_with(".zip") {
        "application/zip"
    } else if name.ends_with(".tar.gz") {
        "application/gzip"
    } else {
        "application/octet-stream"
    };
    let content_disposition = HeaderValue::from_str(&format!("attachment; filename=\"{name}\""))
        .map_err(|e| ApiError::internal(format!("Invalid content-disposition: {e}")))?;
    let content_type = HeaderValue::from_str(ctype)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CONTENT_DISPOSITION, content_disposition),
        ],
        bytes,
    ))
}
