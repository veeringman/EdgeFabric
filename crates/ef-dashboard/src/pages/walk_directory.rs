use wasm_bindgen_futures::JsFuture;
use wasm_bindgen::prelude::*;
use crate::api::WorkspaceProjectFile;

/// Processes dropped files from a DataTransfer object and extracts text files.
pub async fn process_dropped_files(
    files: &web_sys::FileList,
) -> Vec<WorkspaceProjectFile> {
    let mut result = Vec::new();

    for i in 0..files.length() {
        if let Some(file) = files.get(i) {
            let name = file.name();
            if is_text_file_name(&name) {
                if let Ok(text) = read_file(&file).await {
                    result.push(WorkspaceProjectFile {
                        path: name,
                        content: text,
                    });
                }
            }
        }
    }

    result
}

/// Reads a file and returns its text content
async fn read_file(file: &web_sys::File) -> Result<String, JsValue> {
    let promise = file.text();
    match JsFuture::from(promise).await {
        Ok(value) => {
            let text = value.as_string().unwrap_or_default();
            Ok(text)
        }
        Err(e) => Err(e),
    }
}

/// Determines if a file should be included based on its name
fn is_text_file_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    
    // Skip hidden files
    if name.starts_with('.') {
        return false;
    }
    
    // Skip common binary extensions
    let skip_extensions = [
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".svg", ".ico",
        ".mp3", ".mp4", ".avi", ".mov", ".wav", ".flac",
        ".zip", ".tar", ".gz", ".rar", ".7z",
        ".exe", ".dll", ".so", ".dylib", ".a",
        ".o", ".obj", ".pyc", ".pyo", ".class", ".jar",
        ".wasm", ".wasm32",
    ];
    
    for ext in skip_extensions.iter() {
        if lower.ends_with(ext) {
            return false;
        }
    }
    
    // Include known text extensions
    let text_extensions = [
        ".rs", ".ts", ".js", ".py", ".java", ".c", ".cpp", ".h", ".hpp",
        ".go", ".rb", ".php", ".swift", ".kt", ".scala", ".sh", ".bash",
        ".txt", ".md", ".json", ".yaml", ".yml", ".toml", ".xml", ".html",
        ".css", ".scss", ".less", ".sql", ".sh", ".env", ".properties",
        ".gradle", ".maven", ".config", ".conf", ".ini", ".cfg",
        ".lock", ".log", ".gitignore", ".dockerignore", ".editorconfig",
        ".eslintrc", ".prettierrc", ".nvmrc", ".babelrc", ".vscode",
        ".makefile", ".dockerfile", ".mk",
    ];
    
    for ext in text_extensions.iter() {
        if lower.ends_with(ext) {
            return true;
        }
    }
    
    // Include if no extension (common for some text files like Makefile, README, etc)
    if !name.contains('.') {
        return true;
    }
    
    false
}
