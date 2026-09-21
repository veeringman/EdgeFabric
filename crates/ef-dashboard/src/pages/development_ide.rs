use leptos::prelude::*;
use js_sys::Uint8Array;
use base64::Engine as _;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::JsFuture;

use crate::api::{
    self, CreateIosStarterProjectRequest, Device, SaveWorkspaceProjectRequest,
    SubmitIosBuildRequest, SubmitIotDeployRequest, SubmitQuantumProgramRequest,
    WorkspaceArchiveExportRequest, WorkspaceArchiveImportRequest, WorkspaceProjectFile,
    WorkspaceProjectSummary,
};

use super::walk_directory::process_dropped_files;

const DEFAULT_WORKSPACE_PATH: &str = "/Users/vijay/rnd/projects/workspace";

const NAV_MIN_WIDTH: i32 = 220;
const NAV_MAX_WIDTH: i32 = 420;
const ACTIONS_MIN_WIDTH: i32 = 260;
const ACTIONS_MAX_WIDTH: i32 = 520;
    const TERMINAL_MIN_HEIGHT: i32 = 180;
    const TERMINAL_MAX_HEIGHT: i32 = 420;
    const BUILD_RESULTS_MIN_HEIGHT: i32 = 150;
    const BUILD_RESULTS_MAX_HEIGHT: i32 = 360;

fn detect_editor_language(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".swift") {
        "swift"
    } else if lower.ends_with(".py") {
        "python"
    } else if lower.ends_with(".rs") {
        "rust"
    } else if lower.ends_with(".c") || lower.ends_with(".h") || lower.ends_with(".cpp") || lower.ends_with(".cc") || lower.ends_with(".cxx") || lower.ends_with(".hpp") || lower.ends_with(".hh") || lower.ends_with(".hxx") {
        "cpp"
    } else {
        "plain"
    }
}

fn editor_language_label(lang: &str) -> &'static str {
    match lang {
        "swift" => "Swift / SwiftUI",
        "python" => "Python",
        "cpp" => "C / C++",
        "rust" => "Rust",
        _ => "Plain Text",
    }
}

fn escape_html(text: &str) -> String {
    text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn lang_keywords(lang: &str) -> &'static [&'static str] {
    match lang {
        "swift" => &[
            "class", "struct", "enum", "protocol", "extension", "func", "let", "var", "if", "else", "guard", "switch", "case", "for", "while", "return", "import", "public", "private", "internal", "fileprivate", "open", "static", "mutating", "nonmutating", "init", "deinit", "async", "await", "try", "catch", "throw", "throws", "inout", "where", "some", "any", "Self", "self", "nil", "true", "false",
        ],
        "python" => &[
            "def", "class", "if", "elif", "else", "for", "while", "return", "import", "from", "as", "try", "except", "finally", "raise", "with", "lambda", "pass", "break", "continue", "yield", "async", "await", "global", "nonlocal", "None", "True", "False", "and", "or", "not", "in", "is",
        ],
        "cpp" => &[
            "int", "float", "double", "char", "bool", "void", "class", "struct", "enum", "template", "typename", "namespace", "using", "include", "if", "else", "switch", "case", "for", "while", "do", "return", "break", "continue", "const", "constexpr", "static", "inline", "virtual", "public", "private", "protected", "new", "delete", "this", "nullptr", "true", "false", "auto", "extern",
        ],
        "rust" => &[
            "fn", "let", "mut", "const", "static", "struct", "enum", "trait", "impl", "for", "in", "if", "else", "match", "while", "loop", "return", "break", "continue", "pub", "crate", "mod", "use", "where", "async", "await", "move", "ref", "self", "Self", "dyn", "unsafe", "as", "true", "false",
        ],
        _ => &[],
    }
}

fn lang_types(lang: &str) -> &'static [&'static str] {
    match lang {
        "swift" => &["String", "Int", "Double", "Float", "Bool", "Array", "Dictionary", "Set", "View", "Text", "VStack", "HStack", "ZStack", "NavigationStack"],
        "python" => &["str", "int", "float", "bool", "list", "dict", "set", "tuple", "object"],
        "cpp" => &["std", "string", "vector", "map", "set", "size_t"],
        "rust" => &["String", "str", "Vec", "Option", "Result", "Self"],
        _ => &[],
    }
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn highlight_code_html(lang: &str, code: &str) -> String {
    let keywords = lang_keywords(lang);
    let types = lang_types(lang);
    let mut out = String::with_capacity(code.len() + 64);
    for line in code.lines() {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            let ch = chars[i];

            let is_hash_comment = lang == "python" && ch == '#';
            let is_slash_comment = i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '/';
            if is_hash_comment || is_slash_comment {
                let comment: String = chars[i..].iter().collect();
                out.push_str("<span class=\"tok-comment\">");
                out.push_str(&escape_html(&comment));
                out.push_str("</span>");
                i = chars.len();
                continue;
            }

            if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '*' {
                let mut j = i + 2;
                while j + 1 < chars.len() {
                    if chars[j] == '*' && chars[j + 1] == '/' {
                        j += 2;
                        break;
                    }
                    j += 1;
                }
                let token: String = chars[i..j.min(chars.len())].iter().collect();
                out.push_str("<span class=\"tok-comment\">");
                out.push_str(&escape_html(&token));
                out.push_str("</span>");
                i = j.min(chars.len());
                continue;
            }

            if ch == '"' || ch == '\'' {
                let quote = ch;
                let mut j = i + 1;
                while j < chars.len() {
                    if chars[j] == '\\' {
                        j += 2;
                        continue;
                    }
                    if chars[j] == quote {
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                let token: String = chars[i..j.min(chars.len())].iter().collect();
                out.push_str("<span class=\"tok-string\">");
                out.push_str(&escape_html(&token));
                out.push_str("</span>");
                i = j.min(chars.len());
                continue;
            }

            if ch.is_ascii_digit() {
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.' || chars[j] == '_') {
                    j += 1;
                }
                let token: String = chars[i..j].iter().collect();
                out.push_str("<span class=\"tok-number\">");
                out.push_str(&escape_html(&token));
                out.push_str("</span>");
                i = j;
                continue;
            }

            if is_ident_start(ch) {
                let mut j = i + 1;
                while j < chars.len() && is_ident_continue(chars[j]) {
                    j += 1;
                }
                let token: String = chars[i..j].iter().collect();
                if keywords.contains(&token.as_str()) {
                    out.push_str("<span class=\"tok-keyword\">");
                    out.push_str(&escape_html(&token));
                    out.push_str("</span>");
                } else if types.contains(&token.as_str()) {
                    out.push_str("<span class=\"tok-type\">");
                    out.push_str(&escape_html(&token));
                    out.push_str("</span>");
                } else {
                    out.push_str(&escape_html(&token));
                }
                i = j;
                continue;
            }

            if "{}[]();:,.<>+-=*/!&|%^~?".contains(ch) {
                out.push_str("<span class=\"tok-op\">");
                out.push_str(&escape_html(&ch.to_string()));
                out.push_str("</span>");
                i += 1;
                continue;
            }

            out.push_str(&escape_html(&ch.to_string()));
            i += 1;
        }
        out.push('\n');
    }
    out
}

#[component]
pub fn DevelopmentIdePage() -> impl IntoView {
    let (platform, set_platform) = signal("ios".to_string());
    let (source_ref, set_source_ref) = signal("project-main".to_string());
    let (project_path, set_project_path) = signal(DEFAULT_WORKSPACE_PATH.to_string());
    let (region, set_region) = signal(String::new());
    let (ssh_user, set_ssh_user) = signal("builder".to_string());

    let (ios_target_device_id, set_ios_target_device_id) = signal(String::new());
    let (ios_scheme, set_ios_scheme) = signal("App".to_string());
    let (ios_configuration, set_ios_configuration) = signal("Debug".to_string());

    let (iot_target_device_id, set_iot_target_device_id) = signal(String::new());
    let (iot_board_profile, set_iot_board_profile) = signal("esp32c6".to_string());
    let (iot_deploy_script, set_iot_deploy_script) = signal(String::new());

    let (android_application_id, set_android_application_id) = signal("com.example.edgefabric".to_string());
    let (android_gradle_task, set_android_gradle_task) = signal("assembleDebug".to_string());

    let (quantum_backend_kind, set_quantum_backend_kind) = signal("virtual".to_string());
    let (quantum_backend_name, set_quantum_backend_name) = signal(String::new());
    let (quantum_veeros_host_hint, set_quantum_veeros_host_hint) = signal(String::new());
    let (quantum_run_script, set_quantum_run_script) = signal(String::new());
    let (source_file_name, set_source_file_name) = signal("source.txt".to_string());
    let (source_code, set_source_code) = signal(String::new());

    let (submitting, set_submitting) = signal(false);
    let (message, set_message) = signal(Option::<String>::None);
    let (last_response, set_last_response) = signal(String::new());
    let (ide_fullscreen, set_ide_fullscreen) = signal(false);
    let (workspace_expanded, set_workspace_expanded) = signal(false);
    let (navigator_visible, set_navigator_visible) = signal(true);
    let (actions_visible, set_actions_visible) = signal(true);
    let (navigator_width, set_navigator_width) = signal(280i32);
    let (actions_width, set_actions_width) = signal(320i32);

    // Fetch all devices once on load for device-selection dropdowns
    let devices_resource = LocalResource::new(|| async move {
        api::fetch_all_devices().await
    });

    let (starter_name, set_starter_name) = signal("VijayStarter".to_string());
    let (starter_organization_id, set_starter_organization_id) = signal("com.vijay.edgefabric".to_string());
    let (starter_bundle_id, set_starter_bundle_id) = signal(String::new());
    let (starter_team_id, set_starter_team_id) = signal(String::new());
    let (starter_interface_style, set_starter_interface_style) = signal("swiftui".to_string());
    let (starter_deployment_target, set_starter_deployment_target) = signal("17.0".to_string());
    let (starter_running, set_starter_running) = signal(false);
    let (starter_message, set_starter_message) = signal(Option::<String>::None);
    let (starter_project_dir, set_starter_project_dir) = signal(String::new());
    let (starter_archive_path, set_starter_archive_path) = signal(String::new());
    let (starter_xcode_info, set_starter_xcode_info) = signal(String::new());
    let (starter_default_username, set_starter_default_username) = signal(String::new());

    let (workspace_files, set_workspace_files) = signal(Vec::<WorkspaceProjectFile>::new());
    let (workspace_active_path, set_workspace_active_path) = signal(String::new());
    let (workspace_new_path, set_workspace_new_path) = signal(String::new());
    let (workspace_context_path, set_workspace_context_path) = signal(String::new());
    let (expanded_folders, set_expanded_folders) = signal(Vec::<String>::new());
    let (editor_language_mode, set_editor_language_mode) = signal("auto".to_string());
    let (editor_theme, set_editor_theme) = signal("edge".to_string());

    // Server-persisted workspace projects ("Save Project" feature)
    let (saved_project_name, set_saved_project_name) = signal(String::new());
    let (saved_projects, set_saved_projects) = signal(Vec::<WorkspaceProjectSummary>::new());
    let (saved_projects_visible, set_saved_projects_visible) = signal(false);

    let active_editor_language = Memo::new(move |_| {
        let mode = editor_language_mode.get();
        if mode != "auto" {
            return mode;
        }
        let active_path = workspace_active_path.get();
        if !active_path.is_empty() {
            return detect_editor_language(&active_path).to_string();
        }
        detect_editor_language(&source_file_name.get()).to_string()
    });

    let active_workspace_content = Memo::new(move |_| {
        let active = workspace_active_path.get();
        workspace_files
            .get()
            .into_iter()
            .find(|f| f.path == active)
            .map(|f| f.content)
            .unwrap_or_default()
    });

    let highlighted_workspace_content = Memo::new(move |_| {
        highlight_code_html(&active_editor_language.get(), &active_workspace_content.get())
    });

    let workspace_grid_style = Memo::new(move |_| {
        if workspace_expanded.get() {
            "display:flex; gap:0.75rem; align-items:stretch; min-width:0;".to_string()
        } else {
            match (navigator_visible.get(), actions_visible.get()) {
                (true, true) => format!(
                    "display:grid; grid-template-columns: minmax({nav_min}px, {nav}px) minmax(0, 1fr) minmax({actions_min}px, {actions}px); gap:1rem;",
                    nav_min = NAV_MIN_WIDTH,
                    nav = navigator_width.get(),
                    actions_min = ACTIONS_MIN_WIDTH,
                    actions = actions_width.get(),
                ),
                (true, false) => format!(
                    "display:grid; grid-template-columns: minmax({nav_min}px, {nav}px) minmax(0, 1fr); gap:1rem;",
                    nav_min = NAV_MIN_WIDTH,
                    nav = navigator_width.get(),
                ),
                (false, true) => format!(
                    "display:grid; grid-template-columns: minmax(0, 1fr) minmax({actions_min}px, {actions}px); gap:1rem;",
                    actions_min = ACTIONS_MIN_WIDTH,
                    actions = actions_width.get(),
                ),
                (false, false) => "display:grid; grid-template-columns: minmax(0, 1fr); gap:1rem;".to_string(),
            }
        }
    });

    let highlight_scroll_ref = NodeRef::<leptos::html::Pre>::new();

    let (terminal_visible, set_terminal_visible) = signal(false);
    let (terminal_nonce, set_terminal_nonce) = signal(0u64);
    let (terminal_height, set_terminal_height) = signal(260i32);
    let (build_results_visible, set_build_results_visible) = signal(false);
    let (build_results, _set_build_results) = signal(String::new());
    let (build_results_height, set_build_results_height) = signal(200i32);
    let (terminal_host, set_terminal_host) = signal("127.0.0.1".to_string());
    let (terminal_port, set_terminal_port) = signal("22".to_string());
    let (terminal_agent_id, set_terminal_agent_id) = signal(String::new());
    let (terminal_ws_path, set_terminal_ws_path) = signal(String::new());

    let trim_opt = |value: String| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    };

    Effect::new(move |_| {
        leptos::task::spawn_local(async move {
            if let Ok(defaults) = api::get_ios_starter_defaults().await {
                if !defaults.suggested_app_name.trim().is_empty() {
                    set_starter_name.set(defaults.suggested_app_name.clone());
                }
                if !defaults.suggested_organization_id.trim().is_empty() {
                    set_starter_organization_id.set(defaults.suggested_organization_id.clone());
                }
                if let Some(team) = defaults.default_team_id {
                    if !team.trim().is_empty() {
                        set_starter_team_id.set(team);
                    }
                }

                let mut xcode_summary = String::new();
                if let Some(version) = defaults.xcode_version {
                    if !version.trim().is_empty() {
                        xcode_summary.push_str(version.trim());
                    }
                }
                if let Some(path) = defaults.xcode_developer_dir {
                    if !path.trim().is_empty() {
                        if !xcode_summary.is_empty() {
                            xcode_summary.push_str(" | ");
                        }
                        xcode_summary.push_str(path.trim());
                    }
                }
                set_starter_xcode_info.set(xcode_summary);
                set_starter_default_username.set(defaults.username);
            }
        });
    });

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_submitting.set(true);
        set_message.set(None);
        set_last_response.set(String::new());

        let platform_value = platform.get_untracked();
        let source_ref_value = source_ref.get_untracked();
        let project_path_value = project_path.get_untracked();
        let region_value = trim_opt(region.get_untracked());
        let ssh_user_value = trim_opt(ssh_user.get_untracked());

        leptos::task::spawn_local(async move {
            let outcome = if platform_value == "ios" {
                let req = SubmitIosBuildRequest {
                    source_ref: source_ref_value,
                    project_path: project_path_value,
                    scheme: ios_scheme.get_untracked(),
                    configuration: ios_configuration.get_untracked(),
                    target_device_id: ios_target_device_id.get_untracked(),
                    region: region_value,
                    required_runner_capabilities: vec![
                        "xcode".into(),
                        "artifact_upload".into(),
                    ],
                    required_runner_labels: vec![],
                    lease_ttl_seconds: Some(1800),
                    ssh_user: ssh_user_value,
                };
                api::submit_ios_build(&req).await
            } else if platform_value == "iot" {
                let req = SubmitIotDeployRequest {
                    source_ref: source_ref_value,
                    project_path: project_path_value,
                    target_device_id: iot_target_device_id.get_untracked(),
                    board_profile: trim_opt(iot_board_profile.get_untracked()),
                    deploy_command: trim_opt(iot_deploy_script.get_untracked()),
                    region: region_value,
                    required_runner_capabilities: vec![
                        "cicd_runner".into(),
                        "linux_runner".into(),
                        "iot_toolchain".into(),
                        "iot_deploy".into(),
                        "artifact_upload".into(),
                    ],
                    required_runner_labels: vec!["iot".into()],
                    lease_ttl_seconds: Some(1800),
                    ssh_user: ssh_user_value,
                };
                api::submit_iot_deploy(&req).await
            } else if platform_value == "quantum" {
                let backend_kind = quantum_backend_kind.get_untracked();
                let mut required_runner_capabilities = vec![
                    "cicd_runner".into(),
                    "linux_runner".into(),
                    "quantum_sdk".into(),
                    "artifact_upload".into(),
                ];
                if backend_kind == "virtual" {
                    required_runner_capabilities.push("quantum_simulator".into());
                } else {
                    required_runner_capabilities.push("veeros_host".into());
                }

                let req = SubmitQuantumProgramRequest {
                    source_ref: source_ref_value,
                    project_path: project_path_value,
                    run_command: trim_opt(quantum_run_script.get_untracked()),
                    backend_kind: Some(backend_kind),
                    backend_name: trim_opt(quantum_backend_name.get_untracked()),
                    veeros_host_hint: trim_opt(quantum_veeros_host_hint.get_untracked()),
                    region: region_value,
                    required_runner_capabilities,
                    required_runner_labels: vec!["quantum".into()],
                    lease_ttl_seconds: Some(1800),
                    ssh_user: ssh_user_value,
                };
                api::submit_quantum_program(&req).await
            } else {
                Err("Android starter templates are available, but Android remote runner submission is not wired yet.".to_string())
            };

            match outcome {
                Ok(value) => {
                    set_message.set(Some(if platform_value == "ios" {
                        "iOS build job submitted.".to_string()
                    } else if platform_value == "iot" {
                        "IoT build and deploy job submitted.".to_string()
                    } else if platform_value == "quantum" {
                        "Quantum build and run job submitted.".to_string()
                    } else {
                        "Android workspace prepared.".to_string()
                    }));
                    let pretty = serde_json::to_string_pretty(&value)
                        .unwrap_or_else(|_| value.to_string());
                    set_last_response.set(pretty);
                }
                Err(err) => {
                    set_message.set(Some(format!("Submission failed: {err}")));
                }
            }

            set_submitting.set(false);
        });
    };

    let upload_source = move |ev: leptos::ev::Event| {
        let Some(target) = ev.target() else {
            set_message.set(Some("Unable to read selected file.".to_string()));
            return;
        };
        let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() else {
            set_message.set(Some("Upload control is not available.".to_string()));
            return;
        };
        let Some(files) = input.files() else {
            return;
        };
        let Some(file) = files.get(0) else {
            return;
        };

        set_source_file_name.set(file.name());

        let set_source_code = set_source_code;
        let set_message = set_message;
        let set_source_ref = set_source_ref;
        let filename = file.name();
        let promise = file.text();

        leptos::task::spawn_local(async move {
            match JsFuture::from(promise).await {
                Ok(text) => {
                    let text = text.as_string().unwrap_or_default();
                    set_source_code.set(text);
                    set_source_ref.set(format!("upload:{filename}"));
                    set_message.set(Some(format!("Loaded source file: {filename}")));
                }
                Err(err) => {
                    let msg = err
                        .as_string()
                        .unwrap_or_else(|| "Unable to read source file.".to_string());
                    set_message.set(Some(format!("Upload failed: {msg}")));
                }
            }
        });
    };

    let download_source = move |_| {
        let content = source_code.get_untracked();
        if content.trim().is_empty() {
            set_message.set(Some("Nothing to download. Add or upload source code first.".to_string()));
            return;
        }

        let mut filename = source_file_name.get_untracked();
        if filename.trim().is_empty() {
            filename = if platform.get_untracked() == "iot" {
                "iot-source.txt".to_string()
            } else if platform.get_untracked() == "ios" {
                "ContentView.swift".to_string()
            } else if platform.get_untracked() == "android" {
                "MainActivity.kt".to_string()
            } else {
                "quantum-source.qasm".to_string()
            };
            set_source_file_name.set(filename.clone());
        }

        let blob_parts = js_sys::Array::new();
        blob_parts.push(&JsValue::from_str(&content));
        let Ok(blob) = web_sys::Blob::new_with_str_sequence(&blob_parts) else {
            set_message.set(Some("Failed to prepare source file for download.".to_string()));
            return;
        };
        let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
            set_message.set(Some("Failed to create download URL.".to_string()));
            return;
        };

        let Some(window) = web_sys::window() else {
            let _ = web_sys::Url::revoke_object_url(&url);
            set_message.set(Some("Browser window unavailable for download.".to_string()));
            return;
        };
        let Some(document) = window.document() else {
            let _ = web_sys::Url::revoke_object_url(&url);
            set_message.set(Some("Document unavailable for download.".to_string()));
            return;
        };

        let Ok(el) = document.create_element("a") else {
            let _ = web_sys::Url::revoke_object_url(&url);
            set_message.set(Some("Failed to create download link.".to_string()));
            return;
        };
        let Ok(anchor) = el.dyn_into::<web_sys::HtmlAnchorElement>() else {
            let _ = web_sys::Url::revoke_object_url(&url);
            set_message.set(Some("Failed to prepare download link.".to_string()));
            return;
        };

        anchor.set_href(&url);
        anchor.set_download(&filename);
        if let Some(body) = document.body() {
            let _ = body.append_child(&anchor);
        }
        anchor.click();
        anchor.remove();
        let _ = web_sys::Url::revoke_object_url(&url);
        set_message.set(Some(format!("Downloaded source file: {filename}")));
    };

    let resolve_workspace_target_path = move |raw_path: &str| -> String {
        let input = raw_path.trim().trim_start_matches('/').to_string();
        if input.is_empty() {
            return String::new();
        }
        let context = workspace_context_path
            .get_untracked()
            .trim()
            .trim_matches('/')
            .to_string();
        if context.is_empty() {
            input
        } else if input == context || input.starts_with(&(context.clone() + "/")) {
            input
        } else {
            format!("{context}/{input}")
        }
    };

    let add_workspace_file = move |_| {
        let path = resolve_workspace_target_path(&workspace_new_path.get_untracked());
        if path.is_empty() {
            set_message.set(Some("Provide a file path before adding it to the workspace.".to_string()));
            return;
        }
        if path.ends_with('/') {
            set_message.set(Some("Folder paths should be created with the New Folder action.".to_string()));
            return;
        }

        let mut created = false;
        set_workspace_files.update(|files| {
            if files.iter().any(|f| f.path == path) {
                return;
            }
            files.push(WorkspaceProjectFile {
                path: path.clone(),
                content: String::new(),
            });
            created = true;
        });

        if created {
            set_workspace_active_path.set(path.clone());
            set_workspace_new_path.set(String::new());
            set_message.set(Some(format!("Added workspace file: {path}")));
        } else {
            set_message.set(Some(format!("File already exists in workspace: {path}")));
        }
    };

    let add_workspace_folder = move |_| {
        let folder = resolve_workspace_target_path(&workspace_new_path.get_untracked())
            .trim_matches('/')
            .to_string();
        if folder.is_empty() {
            set_message.set(Some("Enter a folder name before creating it.".to_string()));
            return;
        }

        let folder_prefix = format!("{folder}/");
        let marker_path = format!("{folder}/.gitkeep");
        let mut created = false;
        set_workspace_files.update(|files| {
            if files
                .iter()
                .any(|f| f.path == marker_path || f.path.starts_with(&folder_prefix))
            {
                return;
            }
            files.push(WorkspaceProjectFile {
                path: marker_path.clone(),
                content: String::new(),
            });
            created = true;
        });

        if created {
            set_workspace_new_path.set(String::new());
            set_message.set(Some(format!("Created workspace folder: {folder}")));
        } else {
            set_message.set(Some(format!("Folder already exists in workspace: {folder}")));
        }
    };

    let remove_workspace_file = move |path: String| {
        let is_folder_marker = path.ends_with("/.gitkeep");
        let label = if is_folder_marker {
            path.trim_end_matches("/.gitkeep")
        } else {
            path.as_str()
        };

        let confirm_msg = if is_folder_marker {
            format!("Delete folder '{label}' and all files inside it?")
        } else {
            format!("Delete file '{label}'?")
        };

        let confirmed = web_sys::window()
            .and_then(|w| w.confirm_with_message(&confirm_msg).ok())
            .unwrap_or(false);
        if !confirmed {
            return;
        }

        if is_folder_marker {
            let folder_prefix = format!("{label}/");
            set_workspace_files.update(|files| {
                files.retain(|f| !f.path.starts_with(&folder_prefix));
            });

            let active = workspace_active_path.get_untracked();
            if active.starts_with(&folder_prefix) {
                set_workspace_active_path.set(String::new());
            }
            let context = workspace_context_path.get_untracked();
            if context == label || context.starts_with(&folder_prefix) {
                set_workspace_context_path.set(String::new());
            }
            set_message.set(Some(format!("Deleted folder: {label}")));
        } else {
            set_workspace_files.update(|files| files.retain(|f| f.path != path));
            if workspace_active_path.get_untracked() == path {
                set_workspace_active_path.set(String::new());
            }
            set_message.set(Some(format!("Deleted file: {label}")));
        }
    };

    let rename_workspace_entry = move |path: String| {
        let is_folder_marker = path.ends_with("/.gitkeep");
        let current_label = if is_folder_marker {
            path.trim_end_matches("/.gitkeep").to_string()
        } else {
            path.clone()
        };

        let default_value = if is_folder_marker {
            current_label
                .rsplit('/')
                .next()
                .unwrap_or(&current_label)
                .to_string()
        } else {
            current_label
                .rsplit('/')
                .next()
                .unwrap_or(&current_label)
                .to_string()
        };

        let prompt_msg = if is_folder_marker {
            "Rename folder to"
        } else {
            "Rename file to"
        };

        let Some(window) = web_sys::window() else {
            set_message.set(Some("Window unavailable for rename prompt.".to_string()));
            return;
        };
        let Ok(next_value) = window.prompt_with_message_and_default(prompt_msg, &default_value) else {
            return;
        };
        let Some(next_value) = next_value else {
            return;
        };

        let entered = next_value.trim().trim_matches('/').to_string();
        if entered.is_empty() {
            set_message.set(Some("Rename target cannot be empty.".to_string()));
            return;
        }

        if is_folder_marker {
            let old_folder = current_label;
            let old_prefix = format!("{old_folder}/");
            let parent_prefix = old_folder
                .rsplit_once('/')
                .map(|(parent, _)| format!("{parent}/"))
                .unwrap_or_default();
            let new_folder = if entered.contains('/') {
                entered
            } else {
                format!("{parent_prefix}{entered}").trim_matches('/').to_string()
            };
            if new_folder == old_folder {
                return;
            }
            let new_prefix = format!("{new_folder}/");

            let files_snapshot = workspace_files.get_untracked();
            let mut has_conflict = false;
            for file in &files_snapshot {
                if !file.path.starts_with(&old_prefix) {
                    continue;
                }
                let suffix = file.path.trim_start_matches(&old_prefix);
                let candidate = format!("{new_prefix}{suffix}");
                if files_snapshot
                    .iter()
                    .any(|existing| existing.path == candidate && !existing.path.starts_with(&old_prefix))
                {
                    has_conflict = true;
                    break;
                }
            }
            if has_conflict {
                set_message.set(Some(format!("Cannot rename folder; destination already exists: {new_folder}")));
                return;
            }

            set_workspace_files.update(|files| {
                for file in files.iter_mut() {
                    if file.path.starts_with(&old_prefix) {
                        let suffix = file.path.trim_start_matches(&old_prefix);
                        file.path = format!("{new_prefix}{suffix}");
                    }
                }
            });

            let active = workspace_active_path.get_untracked();
            if active.starts_with(&old_prefix) {
                let suffix = active.trim_start_matches(&old_prefix);
                set_workspace_active_path.set(format!("{new_prefix}{suffix}"));
            }
            let context = workspace_context_path.get_untracked();
            if context == old_folder {
                set_workspace_context_path.set(new_folder.clone());
            } else if context.starts_with(&old_prefix) {
                let suffix = context.trim_start_matches(&old_prefix);
                set_workspace_context_path.set(format!("{new_prefix}{suffix}").trim_end_matches('/').to_string());
            }
            set_message.set(Some(format!("Renamed folder to: {new_folder}")));
        } else {
            let old_file = path;
            let parent_prefix = old_file
                .rsplit_once('/')
                .map(|(parent, _)| format!("{parent}/"))
                .unwrap_or_default();
            let new_file = if entered.contains('/') {
                entered
            } else {
                format!("{parent_prefix}{entered}").trim_matches('/').to_string()
            };

            if new_file.ends_with('/') {
                set_message.set(Some("File rename target cannot end with '/'".to_string()));
                return;
            }
            if new_file == old_file {
                return;
            }
            if workspace_files
                .get_untracked()
                .iter()
                .any(|file| file.path == new_file)
            {
                set_message.set(Some(format!("File already exists: {new_file}")));
                return;
            }

            set_workspace_files.update(|files| {
                if let Some(file) = files.iter_mut().find(|f| f.path == old_file) {
                    file.path = new_file.clone();
                }
            });

            if workspace_active_path.get_untracked() == old_file {
                set_workspace_active_path.set(new_file.clone());
            }
            set_message.set(Some(format!("Renamed file to: {new_file}")));
        }
    };

    let upload_workspace_files = move |ev: leptos::ev::Event| {
        let Some(target) = ev.target() else {
            set_message.set(Some("Unable to read selected workspace files.".to_string()));
            return;
        };
        let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() else {
            set_message.set(Some("Workspace upload control is unavailable.".to_string()));
            return;
        };
        let Some(files) = input.files() else {
            return;
        };
        let total = files.length();
        if total == 0 {
            return;
        }

        leptos::task::spawn_local(async move {
            let mut loaded_count = 0u32;
            for idx in 0..total {
                let Some(file) = files.get(idx) else {
                    continue;
                };

                let path = file.name();
                let promise = file.text();
                if let Ok(text) = JsFuture::from(promise).await {
                    let content = text.as_string().unwrap_or_default();
                    set_workspace_files.update(|workspace| {
                        if let Some(existing) = workspace.iter_mut().find(|f| f.path == path) {
                            existing.content = content.clone();
                        } else {
                            workspace.push(WorkspaceProjectFile {
                                path: path.clone(),
                                content: content.clone(),
                            });
                        }
                    });
                    loaded_count += 1;
                    if workspace_active_path.get_untracked().is_empty() {
                        set_workspace_active_path.set(path.clone());
                    }
                }
            }

            set_message.set(Some(format!("Loaded {loaded_count} workspace file(s).")));
        });
    };

    let export_workspace = move |_| {
        let files = workspace_files.get_untracked();
        if files.is_empty() {
            set_message.set(Some("Workspace is empty. Add files before exporting.".to_string()));
            return;
        }

        let req = WorkspaceArchiveExportRequest {
            project_name: Some("edgefabric-workspace".to_string()),
            files,
        };

        leptos::task::spawn_local(async move {
            match api::export_workspace_archive(&req).await {
                Ok(bytes) => {
                    let array = Uint8Array::new_with_length(bytes.len() as u32);
                    array.copy_from(&bytes);
                    let blob_parts = js_sys::Array::new();
                    blob_parts.push(&array);

                    let Ok(blob) = web_sys::Blob::new_with_u8_array_sequence(&blob_parts) else {
                        set_message.set(Some("Failed to prepare workspace ZIP for download.".to_string()));
                        return;
                    };
                    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
                        set_message.set(Some("Failed to create workspace ZIP URL.".to_string()));
                        return;
                    };

                    let Some(window) = web_sys::window() else {
                        let _ = web_sys::Url::revoke_object_url(&url);
                        set_message.set(Some("Browser window unavailable for workspace ZIP download.".to_string()));
                        return;
                    };
                    let Some(document) = window.document() else {
                        let _ = web_sys::Url::revoke_object_url(&url);
                        set_message.set(Some("Document unavailable for workspace ZIP download.".to_string()));
                        return;
                    };

                    let Ok(el) = document.create_element("a") else {
                        let _ = web_sys::Url::revoke_object_url(&url);
                        set_message.set(Some("Failed to create workspace ZIP link.".to_string()));
                        return;
                    };
                    let Ok(anchor) = el.dyn_into::<web_sys::HtmlAnchorElement>() else {
                        let _ = web_sys::Url::revoke_object_url(&url);
                        set_message.set(Some("Failed to prepare workspace ZIP link.".to_string()));
                        return;
                    };

                    anchor.set_href(&url);
                    anchor.set_download("edgefabric-workspace.zip");
                    if let Some(body) = document.body() {
                        let _ = body.append_child(&anchor);
                    }
                    anchor.click();
                    anchor.remove();
                    let _ = web_sys::Url::revoke_object_url(&url);
                    set_message.set(Some("Workspace exported as edgefabric-workspace.zip".to_string()));
                }
                Err(err) => {
                    set_message.set(Some(format!("Workspace ZIP export failed: {err}")));
                }
            }
        });
    };

    let import_workspace_zip = move |ev: leptos::ev::Event| {
        let Some(target) = ev.target() else {
            set_message.set(Some("Unable to read selected ZIP file.".to_string()));
            return;
        };
        let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() else {
            set_message.set(Some("ZIP upload control is unavailable.".to_string()));
            return;
        };
        let Some(files) = input.files() else {
            return;
        };
        let Some(file) = files.get(0) else {
            return;
        };

        let promise = file.array_buffer();
        leptos::task::spawn_local(async move {
            let Ok(buffer) = JsFuture::from(promise).await else {
                set_message.set(Some("Failed to read ZIP file.".to_string()));
                return;
            };

            let array = Uint8Array::new(&buffer);
            let mut bytes = vec![0u8; array.length() as usize];
            array.copy_to(&mut bytes);
            let archive_base64 = base64::engine::general_purpose::STANDARD.encode(bytes);

            let req = WorkspaceArchiveImportRequest { archive_base64 };
            match api::import_workspace_archive(&req).await {
                Ok(resp) => {
                    let imported_count = resp.files.len();
                    let skipped_binary_files = resp.skipped_binary_files;
                    let first_path = resp.files.first().map(|f| f.path.clone());
                    set_workspace_files.set(resp.files);
                    if let Some(first) = first_path {
                        set_workspace_active_path.set(first);
                    } else {
                        set_workspace_active_path.set(String::new());
                    }

                    if skipped_binary_files > 0 {
                        set_message.set(Some(format!(
                            "Imported {imported_count} text files from ZIP. Skipped {} binary file(s).",
                            skipped_binary_files
                        )));
                    } else {
                        set_message.set(Some(format!("Imported {imported_count} files from ZIP workspace.")));
                    }
                }
                Err(err) => {
                    set_message.set(Some(format!("Workspace ZIP import failed: {err}")));
                }
            }
        });
    };

    let handle_workspace_drop = move |ev: leptos::ev::DragEvent| {
        ev.prevent_default();
        
        let Some(data_transfer) = ev.data_transfer() else {
            set_message.set(Some("Unable to read dropped files.".to_string()));
            return;
        };

        let Some(files) = data_transfer.files() else {
            set_message.set(Some("No files in drop event.".to_string()));
            return;
        };

        let set_workspace_files = set_workspace_files;
        let set_workspace_active_path = set_workspace_active_path;
        let set_message = set_message;

        leptos::task::spawn_local(async move {
            let all_files = process_dropped_files(&files).await;

            if all_files.is_empty() {
                set_message.set(Some("No text files found in dropped items.".to_string()));
                return;
            }

            let file_count = all_files.len();
            let first_file_path = all_files.get(0).map(|f| f.path.clone()).unwrap_or_default();

            set_workspace_files.update(|files| {
                for new_file in all_files {
                    if !files.iter().any(|f| f.path == new_file.path) {
                        files.push(new_file);
                    }
                }
            });

            if !first_file_path.is_empty() {
                set_workspace_active_path.set(first_file_path);
            }

            set_message.set(Some(format!("Imported {file_count} files from dropped items.")));
        });
    };

    let handle_workspace_dragover = move |ev: leptos::ev::DragEvent| {
        ev.prevent_default();
        if let Some(data_transfer) = ev.data_transfer() {
            data_transfer.set_drop_effect("copy");
        }
    };

    let sync_workspace_to_source = move |_| {
        let active = workspace_active_path.get_untracked();
        if active.is_empty() {
            set_message.set(Some("Select a workspace file to sync into source editor.".to_string()));
            return;
        }

        let files = workspace_files.get_untracked();
        let Some(file) = files.into_iter().find(|f| f.path == active) else {
            set_message.set(Some("Selected workspace file was not found.".to_string()));
            return;
        };

        set_source_file_name.set(file.path.clone());
        set_source_code.set(file.content.clone());
        set_source_ref.set(format!("workspace:{}", file.path));
        set_message.set(Some("Active workspace file loaded into source editor.".to_string()));
    };

    // Refresh the list of saved projects from the server.
    let refresh_saved_projects = move || {
        leptos::task::spawn_local(async move {
            match api::list_workspace_projects().await {
                Ok(resp) => set_saved_projects.set(resp.projects),
                Err(err) => set_message.set(Some(format!(
                    "Failed to load saved projects: {err}"
                ))),
            }
        });
    };

    // Save the current workspace to the server, prompting for a name when
    // none has been set yet.
    let save_project_to_server = move |_| {
        let files = workspace_files.get_untracked();
        if files.is_empty() {
            set_message.set(Some(
                "Workspace is empty. Add files before saving.".to_string(),
            ));
            return;
        }

        let mut name = saved_project_name.get_untracked().trim().to_string();
        if name.is_empty() {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(value)) = window
                    .prompt_with_message_and_default(
                        "Save project as:",
                        "my-workspace",
                    )
                {
                    name = value.trim().to_string();
                }
            }
        }
        if name.is_empty() {
            set_message.set(Some(
                "Save cancelled — project name is required.".to_string(),
            ));
            return;
        }
        set_saved_project_name.set(name.clone());

        let req = SaveWorkspaceProjectRequest {
            name: name.clone(),
            files,
        };

        leptos::task::spawn_local(async move {
            match api::save_workspace_project(&req).await {
                Ok(resp) => {
                    set_message.set(Some(format!(
                        "✅ Saved project '{}' ({} file(s)) to server.",
                        resp.name, resp.file_count
                    )));
                    // Refresh list so the dropdown shows the new entry.
                    match api::list_workspace_projects().await {
                        Ok(list) => set_saved_projects.set(list.projects),
                        Err(_) => {}
                    }
                }
                Err(err) => {
                    set_message.set(Some(format!("Save project failed: {err}")));
                }
            }
        });
    };

    // Load the named project from the server into the workspace, replacing
    // the current files.
    let load_project_from_server = move |project_name: String| {
        let project_name = project_name.trim().to_string();
        if project_name.is_empty() {
            set_message.set(Some("Select a saved project to load.".to_string()));
            return;
        }
        if !workspace_files.get_untracked().is_empty() {
            if let Some(window) = web_sys::window() {
                let proceed = window
                    .confirm_with_message(
                        "Loading replaces the current workspace files. Continue?",
                    )
                    .unwrap_or(false);
                if !proceed {
                    return;
                }
            }
        }

        leptos::task::spawn_local(async move {
            match api::get_workspace_project(&project_name).await {
                Ok(detail) => {
                    let count = detail.files.len();
                    let first_path = detail.files.first().map(|f| f.path.clone());
                    set_workspace_files.set(detail.files);
                    if let Some(p) = first_path {
                        set_workspace_active_path.set(p);
                    } else {
                        set_workspace_active_path.set(String::new());
                    }
                    set_saved_project_name.set(detail.name.clone());
                    set_message.set(Some(format!(
                        "📥 Loaded project '{}' ({count} file(s)) from server.",
                        detail.name
                    )));
                }
                Err(err) => {
                    set_message.set(Some(format!("Load project failed: {err}")));
                }
            }
        });
    };

    let delete_saved_project = move |project_name: String| {
        let project_name = project_name.trim().to_string();
        if project_name.is_empty() {
            return;
        }
        if let Some(window) = web_sys::window() {
            let proceed = window
                .confirm_with_message(&format!(
                    "Delete saved project '{project_name}' from server?"
                ))
                .unwrap_or(false);
            if !proceed {
                return;
            }
        }

        leptos::task::spawn_local(async move {
            match api::delete_workspace_project(&project_name).await {
                Ok(()) => {
                    set_message.set(Some(format!(
                        "🗑 Deleted saved project '{project_name}'."
                    )));
                    if saved_project_name.get_untracked() == project_name {
                        set_saved_project_name.set(String::new());
                    }
                    match api::list_workspace_projects().await {
                        Ok(list) => set_saved_projects.set(list.projects),
                        Err(_) => {}
                    }
                }
                Err(err) => {
                    set_message.set(Some(format!("Delete project failed: {err}")));
                }
            }
        });
    };

    // Auto-load the saved project list once on mount.
    Effect::new(move |prev: Option<bool>| {
        if prev.is_none() {
            refresh_saved_projects();
        }
        true
    });

    let create_starter_project = move |download_after_create: bool| {
        set_starter_running.set(true);
        set_starter_message.set(None);

        let req = CreateIosStarterProjectRequest {
            name: starter_name.get_untracked(),
            organization_id: trim_opt(starter_organization_id.get_untracked()),
            bundle_id: trim_opt(starter_bundle_id.get_untracked()),
            team_id: trim_opt(starter_team_id.get_untracked()),
            interface_style: trim_opt(starter_interface_style.get_untracked()),
            deployment_target: trim_opt(starter_deployment_target.get_untracked()),
            archive: Some(download_after_create),
            ssh_user: trim_opt(ssh_user.get_untracked()),
        };

        leptos::task::spawn_local(async move {
            match api::create_ios_starter_project(&req).await {
                Ok(response) => {
                    let module_name = response.module_name.clone();
                    set_starter_project_dir.set(response.project_dir.clone());
                    set_starter_archive_path.set(response.archive_path.unwrap_or_default());
                    set_starter_message.set(Some(format!("Starter project created: {module_name}")));

                    if download_after_create {
                        match api::download_ios_starter_archive(&module_name).await {
                            Ok(bytes) => {
                                let array = Uint8Array::new_with_length(bytes.len() as u32);
                                array.copy_from(&bytes);

                                let blob_parts = js_sys::Array::new();
                                blob_parts.push(&array);

                                let Ok(blob) = web_sys::Blob::new_with_u8_array_sequence(&blob_parts) else {
                                    set_starter_message.set(Some("Created project, but failed to prepare archive for download.".to_string()));
                                    set_starter_running.set(false);
                                    return;
                                };
                                let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
                                    set_starter_message.set(Some("Created project, but failed to create archive URL.".to_string()));
                                    set_starter_running.set(false);
                                    return;
                                };

                                let Some(window) = web_sys::window() else {
                                    let _ = web_sys::Url::revoke_object_url(&url);
                                    set_starter_message.set(Some("Created project, but browser window is unavailable for download.".to_string()));
                                    set_starter_running.set(false);
                                    return;
                                };
                                let Some(document) = window.document() else {
                                    let _ = web_sys::Url::revoke_object_url(&url);
                                    set_starter_message.set(Some("Created project, but document is unavailable for download.".to_string()));
                                    set_starter_running.set(false);
                                    return;
                                };

                                let Ok(el) = document.create_element("a") else {
                                    let _ = web_sys::Url::revoke_object_url(&url);
                                    set_starter_message.set(Some("Created project, but failed to create download link.".to_string()));
                                    set_starter_running.set(false);
                                    return;
                                };
                                let Ok(anchor) = el.dyn_into::<web_sys::HtmlAnchorElement>() else {
                                    let _ = web_sys::Url::revoke_object_url(&url);
                                    set_starter_message.set(Some("Created project, but failed to prepare download link.".to_string()));
                                    set_starter_running.set(false);
                                    return;
                                };

                                anchor.set_href(&url);
                                anchor.set_download(&format!("{module_name}.zip"));
                                if let Some(body) = document.body() {
                                    let _ = body.append_child(&anchor);
                                }
                                anchor.click();
                                anchor.remove();
                                let _ = web_sys::Url::revoke_object_url(&url);
                                set_starter_message.set(Some(format!("Starter archive downloaded: {module_name}.zip")));
                            }
                            Err(err) => {
                                set_starter_message.set(Some(format!("Created project, but archive download failed: {err}")));
                            }
                        }
                    }
                }
                Err(err) => {
                    set_starter_message.set(Some(format!("Starter project creation failed: {err}")));
                }
            }

            set_starter_running.set(false);
        });
    };

    let open_terminal_panel = move |_| {
        let host = terminal_host.get_untracked().trim().to_string();
        let port = terminal_port.get_untracked().trim().to_string();
        if host.is_empty() || port.is_empty() {
            set_message.set(Some("Terminal host and port are required.".to_string()));
            return;
        }

        let host_q = js_sys::encode_uri_component(&host)
            .as_string()
            .unwrap_or(host);
        let port_q = js_sys::encode_uri_component(&port)
            .as_string()
            .unwrap_or(port);
        let agent_raw = terminal_agent_id.get_untracked().trim().to_string();

        let ws_path = if agent_raw.is_empty() {
            format!("/api/v1/veeros/shell/ws?host={host_q}&port={port_q}")
        } else {
            let agent_q = js_sys::encode_uri_component(&agent_raw)
                .as_string()
                .unwrap_or(agent_raw);
            format!("/api/v1/veeros/shell/ws?host={host_q}&port={port_q}&agent_id={agent_q}")
        };

        set_terminal_ws_path.set(ws_path);
        set_workspace_expanded.set(true);
        set_terminal_nonce.update(|v| *v = v.saturating_add(1));
        set_terminal_visible.set(true);
    };

    let close_terminal_panel = move |_| {
        set_terminal_visible.set(false);
    };

    let terminate_terminal_panel = move |_| {
        set_terminal_visible.set(false);
        set_terminal_nonce.update(|v| *v = v.saturating_add(1));
    };

    let sync_editor_overlay_scroll = move |ev: leptos::ev::Event| {
        let Some(target) = ev.target() else { return; };
        let Ok(textarea) = target.dyn_into::<web_sys::HtmlElement>() else {
            return;
        };
        let Some(pre) = highlight_scroll_ref.get() else {
            return;
        };
        pre.set_scroll_top(textarea.scroll_top());
        pre.set_scroll_left(textarea.scroll_left());
    };

    let reset_workspace_layout = move |_| {
        set_navigator_visible.set(true);
        set_actions_visible.set(true);
        set_navigator_width.set(280);
        set_actions_width.set(320);
        set_terminal_height.set(260);
        set_build_results_height.set(200);
    };

    let close_navigator_pane = move |_| set_navigator_visible.set(false);
    let close_actions_pane = move |_| set_actions_visible.set(false);

    let start_pane_resize = move |pane: &'static str, ev: leptos::ev::PointerEvent| {
        let Some(window) = web_sys::window() else { return; };
        let Some(document) = window.document() else { return; };

        let start_x = ev.client_x();
        let start_nav = navigator_width.get_untracked();
        let start_actions = actions_width.get_untracked();
        let document_for_up = document.clone();

        let move_handler = Closure::wrap(Box::new(move |event: web_sys::PointerEvent| {
            let dx = event.client_x() - start_x;
            if pane == "navigator" {
                let next = (start_nav + dx).clamp(NAV_MIN_WIDTH, NAV_MAX_WIDTH);
                set_navigator_width.set(next);
            } else {
                let next = (start_actions - dx).clamp(ACTIONS_MIN_WIDTH, ACTIONS_MAX_WIDTH);
                set_actions_width.set(next);
            }
        }) as Box<dyn FnMut(_)>);

        let up_handler = Closure::wrap(Box::new(move |_event: web_sys::PointerEvent| {
            document_for_up.set_onpointermove(None);
            document_for_up.set_onpointerup(None);
        }) as Box<dyn FnMut(_)>);

        document.set_onpointermove(Some(move_handler.as_ref().unchecked_ref()));
        document.set_onpointerup(Some(up_handler.as_ref().unchecked_ref()));
        move_handler.forget();
        up_handler.forget();
    };

    let start_terminal_resize = move |ev: leptos::ev::PointerEvent| {
        let Some(window) = web_sys::window() else { return; };
        let Some(document) = window.document() else { return; };

        let start_y = ev.client_y();
        let start_height = terminal_height.get_untracked();
        let document_for_up = document.clone();

        let move_handler = Closure::wrap(Box::new(move |event: web_sys::PointerEvent| {
            let dy = start_y - event.client_y();
            let next = (start_height + dy).clamp(TERMINAL_MIN_HEIGHT, TERMINAL_MAX_HEIGHT);
            set_terminal_height.set(next);
        }) as Box<dyn FnMut(_)>);

        let up_handler = Closure::wrap(Box::new(move |_event: web_sys::PointerEvent| {
            document_for_up.set_onpointermove(None);
            document_for_up.set_onpointerup(None);
        }) as Box<dyn FnMut(_)>);

        document.set_onpointermove(Some(move_handler.as_ref().unchecked_ref()));
        document.set_onpointerup(Some(up_handler.as_ref().unchecked_ref()));
        move_handler.forget();
        up_handler.forget();
    };

    let start_build_results_resize = move |ev: leptos::ev::PointerEvent| {
        let Some(window) = web_sys::window() else { return; };
        let Some(document) = window.document() else { return; };

        let start_y = ev.client_y();
        let start_height = build_results_height.get_untracked();
        let document_for_up = document.clone();

        let move_handler = Closure::wrap(Box::new(move |event: web_sys::PointerEvent| {
            let dy = start_y - event.client_y();
            let next = (start_height + dy).clamp(BUILD_RESULTS_MIN_HEIGHT, BUILD_RESULTS_MAX_HEIGHT);
            set_build_results_height.set(next);
        }) as Box<dyn FnMut(_)>);

        let up_handler = Closure::wrap(Box::new(move |_event: web_sys::PointerEvent| {
            document_for_up.set_onpointermove(None);
            document_for_up.set_onpointerup(None);
        }) as Box<dyn FnMut(_)>);

        document.set_onpointermove(Some(move_handler.as_ref().unchecked_ref()));
        document.set_onpointerup(Some(up_handler.as_ref().unchecked_ref()));
        move_handler.forget();
        up_handler.forget();
    };

    let open_platform_starter = move |target_platform: &'static str| {
        let app_name = starter_name.get_untracked();
        let org_id = starter_organization_id.get_untracked();
        let board_profile = iot_board_profile.get_untracked();
        let backend_name = quantum_backend_name.get_untracked();
        let (next_project_path, files, active_path) = build_platform_starter_template(
            target_platform,
            &app_name,
            &org_id,
            &board_profile,
            &backend_name,
            &starter_interface_style.get_untracked(),
            &android_application_id.get_untracked(),
        );

        set_platform.set(target_platform.to_string());
        set_project_path.set(next_project_path);
        set_workspace_files.set(files.clone());
        set_workspace_active_path.set(active_path.clone());

        if let Some(active) = files.iter().find(|f| f.path == active_path) {
            set_source_file_name.set(active.path.clone());
            set_source_code.set(active.content.clone());
            set_source_ref.set(format!("workspace:{}", active.path));
        }

        set_ide_fullscreen.set(true);
        set_workspace_expanded.set(true);
        set_message.set(Some(format!("{} starter template loaded into the IDE workspace.", platform_label(target_platform))));
    };

    view! {
        <div class="page-container dev-ide-page" class:ide-page-fullscreen=ide_fullscreen>
            <Show when=move || !ide_fullscreen.get()>
                <div class="page-header">
                    <h1>"Development IDE"</h1>
                    <p class="page-subtitle">
                        "Open platform-specific starter templates and work from a dedicated IDE tab for iOS, Android, IoT, and Quantum."
                    </p>
                </div>
            </Show>

            <div class="dev-ide-shell" class:ide-shell-fullscreen=ide_fullscreen>
                <div class="ide-mode-switch">
                    <button class:active=move || platform.get() == "ios" on:click=move |_| set_platform.set("ios".to_string())>
                        "iOS"
                    </button>
                    <button class:active=move || platform.get() == "android" on:click=move |_| set_platform.set("android".to_string())>
                        "Android"
                    </button>
                    <button class:active=move || platform.get() == "iot" on:click=move |_| set_platform.set("iot".to_string())>
                        "IoT"
                    </button>
                    <button class:active=move || platform.get() == "ios" on:click=move |_| set_platform.set("ios".to_string())>
                        "iOS"
                    </button>
                    <button class:active=move || platform.get() == "android" on:click=move |_| set_platform.set("android".to_string())>
                        "Android"
                    </button>
                    <button class:active=move || platform.get() == "iot" on:click=move |_| set_platform.set("iot".to_string())>
                        "IoT"
                    </button>
                    <button class:active=move || platform.get() == "quantum" on:click=move |_| set_platform.set("quantum".to_string())>
                        "Quantum"
                    </button>
                </div>
                <Show when=move || ide_fullscreen.get()>
                    <button
                        class="btn btn-ghost ide-collapse-btn"
                        type="button"
                        title="Collapse IDE"
                        on:click=move |_| set_ide_fullscreen.set(false)
                    >
                        "⊠ Exit Fullscreen"
                    </button>
                </Show>

                <Show when=move || workspace_expanded.get()>
                    <div class="ide-ws-palette">
                        <div class="ide-ws-palette-group">
                            <button class="ide-ws-icon-btn" type="button" title="Exit Workspace" on:click=move |_| set_workspace_expanded.set(false)>
                                <span class="ide-ws-icon">"⤺"</span>
                            </button>
                            <button class="ide-ws-icon-btn" type="button" title="Reset Layout" on:click=reset_workspace_layout>
                                <span class="ide-ws-icon">"↺"</span>
                            </button>
                            <div class="ide-ws-tab-strip">
                                <button
                                    class="ide-ws-tab"
                                    class:active=move || navigator_visible.get()
                                    type="button"
                                    title="Navigator pane"
                                    on:click=move |_| set_navigator_visible.set(true)
                                >
                                    <span class="ide-ws-tab-icon">"▣"</span>
                                    <span class="ide-ws-tab-text">"Navigator"</span>
                                </button>
                                <button
                                    class="ide-ws-tab"
                                    class:active=move || true
                                    type="button"
                                    title="Editor pane"
                                >
                                    <span class="ide-ws-tab-icon">"✎"</span>
                                    <span class="ide-ws-tab-text">"Editor"</span>
                                </button>
                                <button
                                    class="ide-ws-tab"
                                    class:active=move || actions_visible.get()
                                    type="button"
                                    title="Actions pane"
                                    on:click=move |_| set_actions_visible.set(true)
                                >
                                    <span class="ide-ws-tab-icon">"⚙"</span>
                                    <span class="ide-ws-tab-text">"Actions"</span>
                                </button>
                                <button
                                    class="ide-ws-tab"
                                    class:active=move || terminal_visible.get()
                                    type="button"
                                    title="Terminal pane"
                                    on:click=move |_| set_terminal_visible.set(true)
                                >
                                    <span class="ide-ws-tab-icon">"⌘"</span>
                                    <span class="ide-ws-tab-text">"Terminal"</span>
                                </button>
                            </div>
                        </div>
                        <div class="ide-ws-palette-group ide-ws-palette-right">
                            <span class="ide-ws-palette-hint">"Quick Restore"</span>
                            <button class="ide-ws-icon-btn" type="button" title={move || if navigator_visible.get() { "Navigator visible" } else { "Show Navigator" }} on:click=move |_| set_navigator_visible.set(true)>
                                <span class="ide-ws-icon">"🗂"</span>
                            </button>
                            <button class="ide-ws-icon-btn" type="button" title={move || if actions_visible.get() { "Actions visible" } else { "Show Actions" }} on:click=move |_| set_actions_visible.set(true)>
                                <span class="ide-ws-icon">"⚙"</span>
                            </button>
                            <button class="ide-ws-icon-btn" type="button" title={move || if terminal_visible.get() { "Terminal visible" } else { "Show Terminal" }} on:click=move |_| set_terminal_visible.set(true)>
                                <span class="ide-ws-icon">"⌘"</span>
                            </button>
                        </div>
                    </div>
                </Show>

                <form on:submit=submit class="ide-grid" class:ide-workspace-expanded=workspace_expanded style=move || workspace_grid_style.get()>
                    // ── LEFT: Project config (hidden in expanded workspace mode) ──────
                    <Show when=move || !workspace_expanded.get()>
                        <section class="ide-panel">
                            <h2>"Project"</h2>
                            <label class="field">
                                <span>"Source Ref"</span>
                                <input type="text" prop:value=source_ref on:input=move |ev| set_source_ref.set(event_target_value(&ev))/>
                            </label>
                            <label class="field">
                                <span>"Project Path On Runner"</span>
                                <input type="text" placeholder="/workspace/edgefirmware" prop:value=project_path on:input=move |ev| set_project_path.set(event_target_value(&ev))/>
                            </label>
                            <label class="field">
                                <span>"Region (optional)"</span>
                                <input type="text" prop:value=region on:input=move |ev| set_region.set(event_target_value(&ev))/>
                            </label>
                            <label class="field">
                                <span>"SSH User"</span>
                                <input type="text" prop:value=ssh_user on:input=move |ev| set_ssh_user.set(event_target_value(&ev))/>
                            </label>
                        </section>
                    </Show>

                    // ── CENTER: Platform-specific panels (hidden in expanded workspace mode) ──
                    <Show when=move || !workspace_expanded.get()>
                        <Show when=move || platform.get() == "ios">
                        <section class="ide-panel ide-editor">
                            <h2>"iOS Build + Starter"</h2>
                            <div class="upload-row">
                                <button class="btn btn-ghost" type="button" on:click=move |_| open_platform_starter("ios")>
                                    "Open iOS Starter In IDE"
                                </button>
                            </div>
                            <label class="field">
                                <span>"Target Device"</span>
                                <Suspense fallback=|| view! { <select disabled><option>"Loading devices…"</option></select> }>
                                    {move || {
                                        let devs = devices_resource.get()
                                            .as_deref()
                                            .cloned()
                                            .unwrap_or_default();
                                        let ios_devs: Vec<Device> = devs.devices.into_iter()
                                            .filter(|d| d.os.to_lowercase().contains("ios"))
                                            .collect();
                                        let current = ios_target_device_id.get();
                                        view! {
                                            <select
                                                prop:value=current
                                                on:change=move |ev| set_ios_target_device_id.set(event_target_value(&ev))
                                            >
                                                <option value="">"— select device —"</option>
                                                {ios_devs.into_iter().map(|d| {
                                                    let id = d.id.clone();
                                                    let label = format!("{} ({}) [{}]", d.name, d.os_version, d.status);
                                                    view! { <option value=id>{label}</option> }
                                                }).collect::<Vec<_>>()}
                                            </select>
                                        }
                                    }}
                                </Suspense>
                            </label>
                            <label class="field">
                                <span>"Scheme"</span>
                                <input type="text" prop:value=ios_scheme on:input=move |ev| set_ios_scheme.set(event_target_value(&ev))/>
                            </label>
                            <label class="field">
                                <span>"Configuration"</span>
                                <select prop:value=ios_configuration on:change=move |ev| set_ios_configuration.set(event_target_value(&ev))>
                                    <option value="Debug">"Debug"</option>
                                    <option value="Release">"Release"</option>
                                </select>
                            </label>
                            <p class="starter-meta">"Use the remote iOS starter generator or load a local SwiftUI/UIKit starter template into the workspace."</p>
                        </section>
                    </Show>

                    <Show when=move || platform.get() == "android">
                        <section class="ide-panel ide-editor">
                            <h2>"Android Starter Workspace"</h2>
                            <div class="upload-row">
                                <button class="btn btn-ghost" type="button" on:click=move |_| open_platform_starter("android")>
                                    "Open Android Starter In IDE"
                                </button>
                            </div>
                            <label class="field">
                                <span>"Application ID"</span>
                                <input type="text" prop:value=android_application_id on:input=move |ev| set_android_application_id.set(event_target_value(&ev))/>
                            </label>
                            <label class="field">
                                <span>"Gradle Task"</span>
                                <input type="text" prop:value=android_gradle_task on:input=move |ev| set_android_gradle_task.set(event_target_value(&ev))/>
                            </label>
                            <p class="starter-meta">"Android currently opens a seeded Compose workspace in the IDE. Runner-backed Android submission is not wired in the backend yet."</p>
                        </section>
                    </Show>

                    <Show when=move || platform.get() == "iot">
                        <section class="ide-panel ide-editor">
                            <h2>"IoT Build + Deploy"</h2>
                            <div class="upload-row">
                                <button class="btn btn-ghost" type="button" on:click=move |_| open_platform_starter("iot")>
                                    "Open IoT Starter In IDE"
                                </button>
                            </div>
                            <label class="field">
                                <span>"Target Device ID"</span>
                                <input type="text" prop:value=iot_target_device_id on:input=move |ev| set_iot_target_device_id.set(event_target_value(&ev))/>
                            </label>
                            <label class="field">
                                <span>"Board Profile"</span>
                                <input type="text" prop:value=iot_board_profile on:input=move |ev| set_iot_board_profile.set(event_target_value(&ev))/>
                            </label>
                            <label class="field">
                                <span>"Build/Deploy Script"</span>
                                <textarea
                                    rows="12"
                                    placeholder="idf.py build && idf.py -p /dev/ttyUSB0 flash monitor"
                                    prop:value=iot_deploy_script
                                    on:input=move |ev| set_iot_deploy_script.set(event_target_value(&ev))
                                ></textarea>
                            </label>
                        </section>
                    </Show>

                    <Show when=move || platform.get() == "quantum">
                        <section class="ide-panel ide-editor">
                            <h2>"Quantum Build + Run"</h2>
                            <div class="upload-row">
                                <button class="btn btn-ghost" type="button" on:click=move |_| open_platform_starter("quantum")>
                                    "Open Quantum Starter In IDE"
                                </button>
                            </div>
                            <label class="field">
                                <span>"Backend Kind"</span>
                                <select prop:value=quantum_backend_kind on:change=move |ev| set_quantum_backend_kind.set(event_target_value(&ev))>
                                    <option value="virtual">"virtual"</option>
                                    <option value="real">"real"</option>
                                </select>
                            </label>
                            <label class="field">
                                <span>"Backend Name (optional)"</span>
                                <input type="text" prop:value=quantum_backend_name on:input=move |ev| set_quantum_backend_name.set(event_target_value(&ev))/>
                            </label>
                            <label class="field">
                                <span>"VeerOS Host Hint (optional)"</span>
                                <input type="text" prop:value=quantum_veeros_host_hint on:input=move |ev| set_quantum_veeros_host_hint.set(event_target_value(&ev))/>
                            </label>
                            <label class="field">
                                <span>"Build/Run Script"</span>
                                <textarea
                                    rows="12"
                                    placeholder="cargo run --release --bin qprog -- --backend aer_simulator"
                                    prop:value=quantum_run_script
                                    on:input=move |ev| set_quantum_run_script.set(event_target_value(&ev))
                                ></textarea>
                            </label>
                        </section>
                    </Show>
                    </Show> // end !workspace_expanded

                    // ── EXPANDED WORKSPACE: Full-pane file navigator + editor ─────────
                    <Show when=move || workspace_expanded.get()>
                        <Show when=move || navigator_visible.get()>
                            <>
                                <section class="ide-panel ide-ws-navigator" style=move || format!("flex:0 0 {}px; min-width:{}px; max-width:{}px;", navigator_width.get(), NAV_MIN_WIDTH, NAV_MAX_WIDTH)>
                                    <div class="ide-ws-nav-header">
                                        <span class="ide-ws-nav-title">"Navigator"</span>
                                        <button
                                            class="ide-ws-collapse-btn"
                                            type="button"
                                            title="Close Navigator"
                                            on:click=close_navigator_pane
                                        >"✕"</button>
                                    </div>
                                    <div class="ide-ws-toolbar-v2">
                                        <button class="ide-ws-toolbar-btn" type="button" title="New File" on:click=add_workspace_file>
                                            "＋"
                                        </button>
                                        <button class="ide-ws-toolbar-btn" type="button" title="New Folder" on:click=add_workspace_folder>
                                            "📁"
                                        </button>
                                        <label class="ide-ws-toolbar-btn ide-file-input-label" title="Upload Files">
                                            "⤴"
                                            <input type="file" multiple on:change=upload_workspace_files style="display:none"/>
                                        </label>
                                        <label class="ide-ws-toolbar-btn ide-file-input-label" title="Import ZIP">
                                            "📦"
                                            <input type="file" accept=".zip,application/zip" on:change=import_workspace_zip style="display:none"/>
                                        </label>
                                        <div class="ide-ws-toolbar-divider"></div>
                                        <button class="ide-ws-toolbar-btn" type="button" title="Export to ZIP" on:click=export_workspace>
                                            "💾"
                                        </button>
                                        <button class="ide-ws-toolbar-btn" type="button" title="Sync to Source" on:click=sync_workspace_to_source>
                                            "↗"
                                        </button>
                                        <div class="ide-ws-toolbar-divider"></div>
                                        <button
                                            class="ide-ws-toolbar-btn"
                                            type="button"
                                            title="Save Project to Server"
                                            on:click=save_project_to_server
                                        >
                                            "☁"
                                        </button>
                                        <button
                                            class="ide-ws-toolbar-btn"
                                            type="button"
                                            title="Open Saved Project"
                                            on:click=move |_| {
                                                let next = !saved_projects_visible.get_untracked();
                                                set_saved_projects_visible.set(next);
                                                if next {
                                                    refresh_saved_projects();
                                                }
                                            }
                                        >
                                            "📂"
                                        </button>
                                    </div>
                                    <Show when=move || saved_projects_visible.get() fallback=|| ().into_view()>
                                        <div class="ide-ws-saved-projects">
                                            <div class="ide-ws-saved-projects-header">
                                                <span>"Saved on Server"</span>
                                                <input
                                                    class="ide-ws-create-input"
                                                    type="text"
                                                    placeholder="project name"
                                                    prop:value=saved_project_name
                                                    on:input=move |ev| set_saved_project_name.set(event_target_value(&ev))
                                                />
                                                <button
                                                    class="btn btn-ghost ide-ws-saved-refresh"
                                                    type="button"
                                                    title="Refresh list"
                                                    on:click=move |_| refresh_saved_projects()
                                                >"⟳"</button>
                                            </div>
                                            <Show
                                                when=move || !saved_projects.get().is_empty()
                                                fallback=|| view! {
                                                    <div class="ide-ws-saved-empty">
                                                        "No saved projects yet. Use ☁ to save the current workspace."
                                                    </div>
                                                }
                                            >
                                                <ul class="ide-ws-saved-list">
                                                    <For
                                                        each=move || saved_projects.get()
                                                        key=|p| p.id.clone()
                                                        children=move |project| {
                                                            let name_for_load = project.name.clone();
                                                            let name_for_delete = project.name.clone();
                                                            let display_name = project.name.clone();
                                                            let updated = project.updated_at.clone();
                                                            let count = project.file_count;
                                                            view! {
                                                                <li class="ide-ws-saved-item">
                                                                    <button
                                                                        class="ide-ws-saved-load"
                                                                        type="button"
                                                                        title=move || format!("Load '{display_name}'")
                                                                        on:click=move |_| load_project_from_server(name_for_load.clone())
                                                                    >
                                                                        <span class="ide-ws-saved-name">{project.name.clone()}</span>
                                                                        <span class="ide-ws-saved-meta">
                                                                            {format!("{} file(s) · {}", count, updated)}
                                                                        </span>
                                                                    </button>
                                                                    <button
                                                                        class="ide-ws-saved-delete"
                                                                        type="button"
                                                                        title="Delete from server"
                                                                        on:click=move |_| delete_saved_project(name_for_delete.clone())
                                                                    >"✕"</button>
                                                                </li>
                                                            }
                                                        }
                                                    />
                                                </ul>
                                            </Show>
                                        </div>
                                    </Show>
                                    <div class="ide-ws-quick-create">
                                        <input
                                            class="ide-ws-create-input"
                                            type="text"
                                            placeholder=move || {
                                                let ctx = workspace_context_path.get();
                                                if ctx.is_empty() {
                                                    "new/file.rs or src/components".to_string()
                                                } else {
                                                    format!("{ctx}/...")
                                                }
                                            }
                                            prop:value=workspace_new_path
                                            on:input=move |ev| set_workspace_new_path.set(event_target_value(&ev))
                                        />
                                        <div class="ide-ws-create-actions">
                                            <button class="ide-ws-mini-btn" type="button" on:click=add_workspace_file>
                                                "File"
                                            </button>
                                            <button class="ide-ws-mini-btn" type="button" on:click=add_workspace_folder>
                                                "Folder"
                                            </button>
                                        </div>
                                    </div>
                                    <div class="ide-ws-context-row">
                                        <span class="ide-ws-context-label">"Context:"</span>
                                        <button
                                            type="button"
                                            class="ide-ws-context-path"
                                            title="Clear context"
                                            on:click=move |_| set_workspace_context_path.set(String::new())
                                        >
                                            {move || {
                                                let ctx = workspace_context_path.get();
                                                if ctx.is_empty() { "/".to_string() } else { format!("/{ctx}") }
                                            }}
                                        </button>
                                    </div>
                                    <div
                                        class="ide-ws-file-tree"
                                        on:drop=handle_workspace_drop
                                        on:dragover=handle_workspace_dragover
                                    >
                                        <For
                                            each=move || {
                                                let mut files = workspace_files.get();
                                                files.sort_by(|a, b| a.path.cmp(&b.path));
                                                let expanded = expanded_folders.get();
                                                files
                                                    .into_iter()
                                                    .filter(|file| {
                                                        let file_path = if file.path.ends_with("/.gitkeep") {
                                                            file.path.trim_end_matches("/.gitkeep").to_string()
                                                        } else {
                                                            file.path.clone()
                                                        };

                                                        if let Some((parent, _)) = file_path.rsplit_once('/') {
                                                            expanded.contains(&parent.to_string())
                                                        } else {
                                                            true
                                                        }
                                                    })
                                                    .collect::<Vec<_>>()
                                            }
                                            key=|file| file.path.clone()
                                            children=move |file| {
                                                let file_path = file.path.clone();
                                                let is_folder_marker = file_path.ends_with("/.gitkeep");
                                                let logical_path = if is_folder_marker {
                                                    file_path.trim_end_matches("/.gitkeep").to_string()
                                                } else {
                                                    file_path.clone()
                                                };
                                                let folder_path = if is_folder_marker { logical_path.clone() } else { String::new() };
                                                let depth = logical_path.matches('/').count();
                                                let display_name = logical_path
                                                    .rsplit('/')
                                                    .next()
                                                    .unwrap_or(&logical_path)
                                                    .to_string();
                                                let row_style = format!("padding-left:{}px;", 10 + (depth * 14));
                                                view! {
                                                    <div class="ide-ws-file-row" style=row_style>
                                                        <button
                                                            type="button"
                                                            class="ide-ws-folder-toggle"
                                                            class:hidden=move || !is_folder_marker
                                                            title={
                                                                let fp = folder_path.clone();
                                                                move || {
                                                                    let expanded = expanded_folders.get();
                                                                    if expanded.contains(&fp) {
                                                                        "Collapse folder"
                                                                    } else {
                                                                        "Expand folder"
                                                                    }
                                                                }
                                                            }
                                                            on:click={
                                                                let fp = folder_path.clone();
                                                                move |_| {
                                                                    set_expanded_folders.update(|exp| {
                                                                        if let Some(pos) = exp.iter().position(|f| f == &fp) {
                                                                            exp.remove(pos);
                                                                        } else {
                                                                            exp.push(fp.clone());
                                                                        }
                                                                    });
                                                                }
                                                            }
                                                        >
                                                            {
                                                                let fp = folder_path.clone();
                                                                move || {
                                                                    let expanded = expanded_folders.get();
                                                                    if expanded.contains(&fp) {
                                                                        "▼"
                                                                    } else {
                                                                        "▶"
                                                                    }
                                                                }
                                                            }
                                                        </button>
                                                        <div class="ide-ws-folder-spacer" class:hidden=move || is_folder_marker></div>
                                                        <span class="ide-ws-file-icon">{if is_folder_marker { "📁" } else { "📄" }}</span>
                                                        <button
                                                            type="button"
                                                            class="ide-ws-file-name"
                                                            class:active=move || {
                                                                !is_folder_marker && workspace_active_path.get() == file_path
                                                            }
                                                            class:folder=is_folder_marker
                                                            on:click={
                                                                let file_path = file.path.clone();
                                                                move |_| {
                                                                    if file_path.ends_with("/.gitkeep") {
                                                                        set_workspace_context_path.set(
                                                                            file_path
                                                                                .trim_end_matches("/.gitkeep")
                                                                                .trim_matches('/')
                                                                                .to_string(),
                                                                        );
                                                                    } else {
                                                                        set_workspace_active_path.set(file_path.clone());
                                                                        if let Some((parent, _)) = file_path.rsplit_once('/') {
                                                                            set_workspace_context_path.set(parent.to_string());
                                                                        } else {
                                                                            set_workspace_context_path.set(String::new());
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        >{display_name}</button>
                                                        <button
                                                            type="button"
                                                            class="ide-ws-file-rename"
                                                            title="Rename"
                                                            on:click={
                                                                let file_path = file.path.clone();
                                                                move |_| rename_workspace_entry(file_path.clone())
                                                            }
                                                        >"✎"</button>
                                                        <button
                                                            type="button"
                                                            class="ide-ws-file-remove"
                                                            title="Remove"
                                                            on:click={
                                                                let file_path = file.path.clone();
                                                                move |_| remove_workspace_file(file_path.clone())
                                                            }
                                                        >"✕"</button>
                                                    </div>
                                                }
                                            }
                                        />
                                    </div>
                                    <div class="ide-ws-nav-footer">
                                        <button class="btn btn-ghost" style="font-size:0.78rem" type="button" on:click=sync_workspace_to_source>"→ Sync"</button>
                                    </div>
                                </section>
                                <div
                                    class="ide-ws-resizer ide-ws-resizer-left"
                                    title="Resize Navigator"
                                    on:pointerdown=move |ev| {
                                        ev.prevent_default();
                                        start_pane_resize("navigator", ev);
                                    }
                                />
                            </>
                        </Show>

                        <section
                            class="ide-panel ide-ws-editor"
                            style="flex:1 1 auto; min-width:0;"
                            attr:data-editor-lang=move || active_editor_language.get()
                            attr:data-editor-theme=move || editor_theme.get()
                        >
                            <div class="ide-ws-editor-header">
                                <span class="ide-ws-editor-filename">{move || {
                                    let p = workspace_active_path.get();
                                    if p.is_empty() { "No file selected".to_string() } else { p }
                                }}</span>
                                <div class="ide-ws-editor-actions">
                                    <span class="ide-ws-language-pill">{move || editor_language_label(&active_editor_language.get())}</span>
                                    <label class="ide-ws-select-wrap" title="Editor language mode">
                                        <span>"Lang"</span>
                                        <select
                                            class="ide-ws-select"
                                            prop:value=editor_language_mode
                                            on:change=move |ev| set_editor_language_mode.set(event_target_value(&ev))
                                        >
                                            <option value="auto">"Auto"</option>
                                            <option value="swift">"Swift"</option>
                                            <option value="python">"Python"</option>
                                            <option value="cpp">"C/C++"</option>
                                            <option value="rust">"Rust"</option>
                                            <option value="plain">"Plain"</option>
                                        </select>
                                    </label>
                                    <label class="ide-ws-select-wrap" title="Editor color theme">
                                        <span>"Theme"</span>
                                        <select
                                            class="ide-ws-select"
                                            prop:value=editor_theme
                                            on:change=move |ev| set_editor_theme.set(event_target_value(&ev))
                                        >
                                            <option value="edge">"Edge Dark"</option>
                                            <option value="dusk">"Dusk"</option>
                                            <option value="forest">"Forest"</option>
                                            <option value="graphite">"Graphite"</option>
                                        </select>
                                    </label>
                                    <button class="btn btn-ghost" style="font-size:0.78rem" type="button" on:click=download_source>"↓ Download"</button>
                                    <button class="btn btn-primary" style="font-size:0.78rem" type="submit" disabled=move || submitting.get() || platform.get() == "android">
                                        {move || if submitting.get() { "Submitting…" }
                                            else if platform.get() == "ios" { "▶ Run iOS Build" }
                                            else if platform.get() == "iot" { "▶ Deploy IoT" }
                                            else if platform.get() == "quantum" { "▶ Run Quantum" }
                                            else { "Prepare" }}
                                    </button>
                                </div>
                            </div>
                            <div class="ide-ws-code-wrap">
                                <pre class="ide-ws-highlight" node_ref=highlight_scroll_ref><code inner_html=move || highlighted_workspace_content.get()></code></pre>
                                <textarea
                                    class="ide-ws-code ide-ws-code-input"
                                    spellcheck="false"
                                    placeholder="Select a file in the navigator to edit its content…"
                                    prop:value=move || active_workspace_content.get()
                                    on:scroll=sync_editor_overlay_scroll
                                    on:input=move |ev| {
                                        let active = workspace_active_path.get_untracked();
                                        if active.is_empty() { return; }
                                        let value = event_target_value(&ev);
                                        set_workspace_files.update(|files| {
                                            if let Some(f) = files.iter_mut().find(|f| f.path == active) {
                                                f.content = value;
                                            }
                                        });
                                    }
                                ></textarea>
                            </div>
                            <Show when=move || message.get().is_some()>
                                <p class="ide-ws-status-bar">{move || message.get().unwrap_or_default()}</p>
                            </Show>

                            <Show when=move || build_results_visible.get() || terminal_visible.get()>
                                <Show when=move || build_results_visible.get()>
                                    <div
                                        class="ide-build-resizer"
                                        title="Resize Build Results"
                                        on:pointerdown=move |ev| {
                                            ev.prevent_default();
                                            start_build_results_resize(ev);
                                        }
                                    />
                                    <div
                                        class="ide-build-results-dock"
                                        style=move || format!("flex:0 0 {}px; min-height:{}px; max-height:{}px;", build_results_height.get(), BUILD_RESULTS_MIN_HEIGHT, BUILD_RESULTS_MAX_HEIGHT)
                                    >
                                        <div class="ide-build-header">
                                            <h3>"Build Results"</h3>
                                            <div class="ide-build-header-actions">
                                                <button class="ide-dock-btn" type="button" title="Close" on:click=move |_| set_build_results_visible.set(false)>
                                                    "✕"
                                                </button>
                                            </div>
                                        </div>
                                        <pre class="ide-build-output">{move || build_results.get()}</pre>
                                    </div>
                                </Show>
                                <Show when=move || terminal_visible.get()>
                                    <div
                                        class="ide-terminal-resizer"
                                        title="Resize Terminal"
                                        on:pointerdown=move |ev| {
                                            ev.prevent_default();
                                            start_terminal_resize(ev);
                                        }
                                    />
                                <div
                                    class="ide-terminal-dock"
                                    style=move || format!("flex:0 0 {}px; min-height:{}px; max-height:{}px;", terminal_height.get(), TERMINAL_MIN_HEIGHT, TERMINAL_MAX_HEIGHT)
                                >
                                    <div class="ide-terminal-header">
                                        <h3>"Terminal"</h3>
                                        <div class="ide-terminal-header-actions">
                                            <span class="ide-dock-path">{move || terminal_ws_path.get()}</span>
                                            <button class="ide-dock-btn ide-dock-btn-secondary" type="button" title="Terminate Session" on:click=terminate_terminal_panel>
                                                "⊗"
                                            </button>
                                            <button class="ide-dock-btn" type="button" title="Close Panel" on:click=close_terminal_panel>
                                                "✕"
                                            </button>
                                        </div>
                                    </div>
                                    <DevIdeShellTerminal ws_path=terminal_ws_path nonce=terminal_nonce/>
                                </div>
                                </Show>
                            </Show>
                        </section>
                    </Show>

                    <div
                        class="ide-ws-resizer ide-ws-resizer-right"
                        class:hidden-pane=move || !workspace_expanded.get() || !actions_visible.get()
                        title="Resize Actions"
                        on:pointerdown=move |ev| {
                            ev.prevent_default();
                            start_pane_resize("actions", ev);
                        }
                    />
                    <section class="ide-panel ide-actions" class:hidden-pane=move || workspace_expanded.get() && !actions_visible.get() style=move || format!("flex:0 0 {}px; min-width:{}px; max-width:{}px;", actions_width.get(), ACTIONS_MIN_WIDTH, ACTIONS_MAX_WIDTH)>
                        <div class="ide-ws-nav-header ide-actions-header">
                            <span class="ide-ws-nav-title">"Actions"</span>
                            <button
                                class="ide-ws-collapse-btn"
                                type="button"
                                title="Close Actions"
                                on:click=close_actions_pane
                            >"✕"</button>
                        </div>
                        <div class="field">
                            <span>"Source File Name"</span>
                            <input type="text" prop:value=source_file_name on:input=move |ev| set_source_file_name.set(event_target_value(&ev))/>
                        </div>
                        <div class="upload-row">
                            <input type="file" accept=".qasm,.rs,.c,.cpp,.h,.hpp,.py,.js,.ts,.txt,.toml,.yaml,.yml,.json,.md" on:change=upload_source/>
                            <button class="btn btn-ghost" type="button" on:click=download_source>"Download Source"</button>
                        </div>

                        <hr class="ide-divider"/>
                        <h3 class="starter-heading">"Project Workspace"</h3>
                        <p class="starter-meta">"Manage multiple files and nested paths for complex project structures before build submission."</p>

                        <label class="field">
                            <span>"New File Path"</span>
                            <input
                                type="text"
                                placeholder="src/main.rs or config/env/development.toml"
                                prop:value=workspace_new_path
                                on:input=move |ev| set_workspace_new_path.set(event_target_value(&ev))
                            />
                        </label>
                        <div class="upload-row">
                            <button class="btn btn-ghost" type="button" on:click=add_workspace_file>"Add File"</button>
                            <input type="file" multiple on:change=upload_workspace_files/>
                            <input type="file" accept=".zip,application/zip" on:change=import_workspace_zip/>
                            <button class="btn btn-ghost" type="button" on:click=export_workspace>"Export ZIP"</button>
                            <button class="btn btn-ghost" type="button" on:click=sync_workspace_to_source>"Use Active As Source"</button>
                        </div>

                        <div 
                            class="workspace-list" 
                            on:drop=handle_workspace_drop
                            on:dragover=handle_workspace_dragover
                        >
                            <For
                                each=move || workspace_files.get()
                                key=|file| file.path.clone()
                                children=move |file| {
                                    let file_path = file.path.clone();
                                    view! {
                                        <div class="workspace-item">
                                            <button
                                                type="button"
                                                class="workspace-path"
                                                class:active=move || workspace_active_path.get() == file_path
                                                on:click={
                                                    let file_path = file.path.clone();
                                                    move |_| set_workspace_active_path.set(file_path.clone())
                                                }
                                            >
                                                {file.path.clone()}
                                            </button>
                                            <button
                                                type="button"
                                                class="workspace-remove"
                                                on:click={
                                                    let file_path = file.path.clone();
                                                    move |_| remove_workspace_file(file_path.clone())
                                                }
                                            >
                                                "Remove"
                                            </button>
                                        </div>
                                    }
                                }
                            />
                        </div>

                        <label class="field">
                            <span>"Active Workspace File Content"</span>
                            <textarea
                                rows="8"
                                placeholder="Select or add a workspace file to edit its content"
                                prop:value=move || {
                                    let active = workspace_active_path.get();
                                    workspace_files
                                        .get()
                                        .into_iter()
                                        .find(|file| file.path == active)
                                        .map(|file| file.content)
                                        .unwrap_or_default()
                                }
                                on:input=move |ev| {
                                    let active = workspace_active_path.get_untracked();
                                    if active.is_empty() {
                                        return;
                                    }
                                    let value = event_target_value(&ev);
                                    set_workspace_files.update(|files| {
                                        if let Some(file) = files.iter_mut().find(|f| f.path == active) {
                                            file.content = value;
                                        }
                                    });
                                }
                            ></textarea>
                        </label>

                        <label class="field">
                            <span>"Source Code"</span>
                            <textarea
                                rows="10"
                                placeholder="Paste or upload your source code here"
                                prop:value=source_code
                                on:input=move |ev| set_source_code.set(event_target_value(&ev))
                            ></textarea>
                        </label>
                        <button class="btn btn-primary" type="submit" disabled=move || submitting.get() || platform.get() == "android">
                            {move || {
                                if submitting.get() {
                                    "Submitting..."
                                } else if platform.get() == "ios" {
                                    "Submit iOS Build"
                                } else if platform.get() == "iot" {
                                    "Submit IoT Build + Deploy"
                                } else if platform.get() == "quantum" {
                                    "Submit Quantum Build + Run"
                                } else {
                                    "Prepare Android Workspace"
                                }
                            }}
                        </button>
                        <a class="btn btn-ghost" href="/builds">"Open Builds"</a>
                        <Show when=move || message.get().is_some()>
                            <p class="submit-message">{move || message.get().unwrap_or_default()}</p>
                        </Show>
                        <Show when=move || !last_response.get().is_empty()>
                            <pre class="response-box">{move || last_response.get()}</pre>
                        </Show>

                        <hr class="ide-divider"/>
                        <h3 class="starter-heading">"Starter Projects"</h3>
                        <label class="field">
                            <span>"Project Name"</span>
                            <input type="text" prop:value=starter_name on:input=move |ev| set_starter_name.set(event_target_value(&ev))/>
                        </label>
                        <Show when=move || platform.get() == "ios">
                            <>
                                <Show when=move || !starter_default_username.get().is_empty()>
                                    <p class="starter-meta">{move || format!("Default user: {}", starter_default_username.get())}</p>
                                </Show>
                                <Show when=move || !starter_xcode_info.get().is_empty()>
                                    <p class="starter-meta">{move || format!("Xcode: {}", starter_xcode_info.get())}</p>
                                </Show>
                                <label class="field">
                                    <span>"Organization ID"</span>
                                    <input type="text" prop:value=starter_organization_id on:input=move |ev| set_starter_organization_id.set(event_target_value(&ev))/>
                                </label>
                                <label class="field">
                                    <span>"Bundle ID (optional)"</span>
                                    <input type="text" prop:value=starter_bundle_id on:input=move |ev| set_starter_bundle_id.set(event_target_value(&ev))/>
                                </label>
                                <label class="field">
                                    <span>"Apple Team ID (optional)"</span>
                                    <input type="text" prop:value=starter_team_id on:input=move |ev| set_starter_team_id.set(event_target_value(&ev))/>
                                </label>
                                <label class="field">
                                    <span>"Interface"</span>
                                    <select prop:value=starter_interface_style on:change=move |ev| set_starter_interface_style.set(event_target_value(&ev))>
                                        <option value="swiftui">"swiftui"</option>
                                        <option value="uikit">"uikit"</option>
                                    </select>
                                </label>
                                <label class="field">
                                    <span>"Deployment Target"</span>
                                    <input type="text" prop:value=starter_deployment_target on:input=move |ev| set_starter_deployment_target.set(event_target_value(&ev))/>
                                </label>
                                <div class="upload-row">
                                    <button class="btn btn-ghost" type="button" disabled=move || starter_running.get() on:click={
                                        let create_starter_project = create_starter_project.clone();
                                        move |_| create_starter_project(false)
                                    }>"Create Remote iOS Starter"</button>
                                    <button class="btn btn-primary" type="button" disabled=move || starter_running.get() on:click={
                                        let create_starter_project = create_starter_project.clone();
                                        move |_| create_starter_project(true)
                                    }>{move || if starter_running.get() { "Working..." } else { "Create + Download ZIP" }}</button>
                                </div>
                            </>
                        </Show>
                        <Show when=move || platform.get() == "android">
                            <>
                                <label class="field">
                                    <span>"Application ID"</span>
                                    <input type="text" prop:value=android_application_id on:input=move |ev| set_android_application_id.set(event_target_value(&ev))/>
                                </label>
                                <button class="btn btn-ghost" type="button" on:click=move |_| open_platform_starter("android")>"Create Android Starter In IDE"</button>
                            </>
                        </Show>
                        <Show when=move || platform.get() == "iot">
                            <>
                                <label class="field">
                                    <span>"Board Profile"</span>
                                    <input type="text" prop:value=iot_board_profile on:input=move |ev| set_iot_board_profile.set(event_target_value(&ev))/>
                                </label>
                                <button class="btn btn-ghost" type="button" on:click=move |_| open_platform_starter("iot")>"Create IoT Starter In IDE"</button>
                            </>
                        </Show>
                        <Show when=move || platform.get() == "quantum">
                            <>
                                <label class="field">
                                    <span>"Backend Name (optional)"</span>
                                    <input type="text" prop:value=quantum_backend_name on:input=move |ev| set_quantum_backend_name.set(event_target_value(&ev))/>
                                </label>
                                <button class="btn btn-ghost" type="button" on:click=move |_| open_platform_starter("quantum")>"Create Quantum Starter In IDE"</button>
                            </>
                        </Show>

                        <Show when=move || !starter_project_dir.get().is_empty()>
                            <p class="starter-meta">{move || format!("Project: {}", starter_project_dir.get())}</p>
                        </Show>
                        <Show when=move || !starter_archive_path.get().is_empty()>
                            <p class="starter-meta">{move || format!("Archive: {}", starter_archive_path.get())}</p>
                        </Show>
                        <Show when=move || starter_message.get().is_some()>
                            <p class="submit-message">{move || starter_message.get().unwrap_or_default()}</p>
                        </Show>

                        <hr class="ide-divider"/>
                        <h3 class="starter-heading">"Terminal Panel"</h3>
                        <p class="starter-meta">"Open an interactive shell docked at the bottom of Development IDE."</p>
                        <label class="field">
                            <span>"Host"</span>
                            <input type="text" prop:value=terminal_host on:input=move |ev| set_terminal_host.set(event_target_value(&ev))/>
                        </label>
                        <label class="field">
                            <span>"Port"</span>
                            <input type="text" prop:value=terminal_port on:input=move |ev| set_terminal_port.set(event_target_value(&ev))/>
                        </label>
                        <label class="field">
                            <span>"Agent ID (optional)"</span>
                            <input type="text" prop:value=terminal_agent_id on:input=move |ev| set_terminal_agent_id.set(event_target_value(&ev))/>
                        </label>
                        <div class="upload-row">
                            <button class="btn btn-primary" type="button" on:click=open_terminal_panel>
                                {move || if terminal_visible.get() { "Reconnect Terminal" } else { "Open Terminal Panel" }}
                            </button>
                            <button
                                class="btn btn-ghost"
                                type="button"
                                disabled=move || !terminal_visible.get()
                                on:click=close_terminal_panel
                            >
                                "Close Panel"
                            </button>
                        </div>
                    </section>
                </form>

            </div>
        </div>

        <style>
            {r#"
                .dev-ide-page {
                    display: flex;
                    flex-direction: column;
                    gap: 1rem;
                }

                .ide-page-fullscreen {
                    position: fixed;
                    inset: 0;
                    z-index: 1000;
                    padding: 0;
                    gap: 0;
                    background: var(--bg-primary, #0d1117);
                    overflow: hidden;
                }

                .dev-ide-shell {
                    background: var(--card-bg);
                    border: 1px solid var(--border-color);
                    border-radius: 10px;
                    padding: 1rem;
                }

                .ide-shell-fullscreen {
                    flex: 1;
                    display: flex;
                    flex-direction: column;
                    border-radius: 0;
                    border: none;
                    height: 100%;
                    overflow: hidden;
                    padding: 0.75rem;
                }

                .ide-shell-fullscreen .ide-grid {
                    flex: 1;
                    overflow: auto;
                    min-height: 0;
                    align-items: stretch;
                }

                .ide-shell-fullscreen .ide-ws-navigator,
                .ide-shell-fullscreen .ide-ws-editor {
                    height: 100%;
                    min-height: 60vh;
                }

                .ide-shell-fullscreen .ide-ws-code {
                    min-height: 50vh;
                }

                .ide-collapse-btn {
                    float: right;
                    margin-bottom: 0.5rem;
                    font-size: 0.82rem;
                }

                .ide-mode-switch {
                    display: inline-flex;
                    border: 1px solid var(--border-color);
                    border-radius: 8px;
                    overflow: hidden;
                    margin-bottom: 1rem;
                }

                .ide-mode-switch button {
                    border: none;
                    background: var(--bg-secondary);
                    color: var(--text-secondary);
                    padding: 0.5rem 0.9rem;
                    cursor: pointer;
                    font-size: 0.88rem;
                }

                .ide-mode-switch button.active {
                    background: var(--accent-color);
                    color: #fff;
                }

                .ide-workspace-expanded {
                    grid-template-columns: 240px 1fr 320px;
                }

                /* Navigator pane */
                .ide-ws-navigator {
                    display: flex;
                    flex-direction: column;
                    gap: 0;
                    padding: 0;
                    overflow: hidden;
                    min-height: 0;
                }

                .ide-ws-nav-header {
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    padding: 0.55rem 0.8rem;
                    background: var(--bg-primary, #0d1117);
                    border-bottom: 1px solid var(--border-color);
                }

                .ide-ws-nav-title {
                    font-size: 0.72rem;
                    font-weight: 600;
                    letter-spacing: 0.06em;
                    text-transform: uppercase;
                    color: var(--text-secondary);
                }

                .ide-ws-collapse-btn {
                    background: none;
                    border: none;
                    color: var(--text-secondary);
                    cursor: pointer;
                    font-size: 0.85rem;
                    padding: 0.1rem 0.3rem;
                    border-radius: 4px;
                }
                .ide-ws-collapse-btn:hover { background: var(--border-color); }

                .ide-ws-toolbar {
                    padding: 0.45rem 0.6rem;
                    border-bottom: 1px solid var(--border-color);
                    background: var(--bg-secondary);
                    gap: 0.4rem;
                    flex-wrap: nowrap;
                    align-items: center;
                }

                .ide-ws-file-tree {
                    flex: 1;
                    overflow-y: auto;
                    padding: 0.4rem 0;
                }

                .ide-ws-file-row {
                    display: flex;
                    align-items: center;
                    gap: 0.3rem;
                    padding: 0.3rem 0.7rem;
                    cursor: pointer;
                }
                .ide-ws-file-row:hover { background: var(--bg-primary, rgba(255,255,255,0.04)); }

                .ide-ws-file-icon {
                    font-size: 0.78rem;
                    flex-shrink: 0;
                }

                .ide-ws-file-name {
                    flex: 1;
                    background: none;
                    border: none;
                    color: var(--text-primary);
                    font-size: 0.82rem;
                    text-align: left;
                    cursor: pointer;
                    padding: 0;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                }
                .ide-ws-file-name.active {
                    color: var(--accent-color, #3b82f6);
                    font-weight: 600;
                }

                .ide-ws-file-remove {
                    background: none;
                    border: none;
                    color: var(--text-secondary);
                    cursor: pointer;
                    font-size: 0.72rem;
                    opacity: 0;
                    padding: 0.1rem 0.3rem;
                    border-radius: 3px;
                }
                .ide-ws-file-row:hover .ide-ws-file-remove { opacity: 1; }

                .ide-ws-nav-footer {
                    display: flex;
                    gap: 0.4rem;
                    padding: 0.45rem 0.6rem;
                    border-top: 1px solid var(--border-color);
                    background: var(--bg-secondary);
                }

                /* Editor pane */
                .ide-ws-editor {
                    display: flex;
                    flex-direction: column;
                    gap: 0;
                    padding: 0;
                    overflow: hidden;
                    min-height: 0;
                    --editor-bg: #0d1117;
                    --editor-text: #c9d1d9;
                    --editor-muted: #9ab0c6;
                    --editor-accent: #67b7ff;
                    --editor-header-bg: #0f1726;
                    --editor-status-bg: #111d2d;
                    --editor-selection: rgba(103, 183, 255, 0.25);
                }

                .ide-ws-editor[data-editor-lang="swift"] {
                    --editor-accent: #ff9d6b;
                    --editor-selection: rgba(255, 157, 107, 0.26);
                }

                .ide-ws-editor[data-editor-lang="python"] {
                    --editor-accent: #f2cf63;
                    --editor-selection: rgba(242, 207, 99, 0.24);
                }

                .ide-ws-editor[data-editor-lang="cpp"] {
                    --editor-accent: #7ecbff;
                    --editor-selection: rgba(126, 203, 255, 0.24);
                }

                .ide-ws-editor[data-editor-lang="rust"] {
                    --editor-accent: #ffa066;
                    --editor-selection: rgba(255, 160, 102, 0.24);
                }

                .ide-ws-editor[data-editor-theme="dusk"] {
                    --editor-bg: #151224;
                    --editor-text: #d8d5f5;
                    --editor-muted: #aca5d8;
                    --editor-header-bg: #1e1732;
                    --editor-status-bg: #1b1530;
                }

                .ide-ws-editor[data-editor-theme="forest"] {
                    --editor-bg: #0d1a15;
                    --editor-text: #d5e8dd;
                    --editor-muted: #97b3a6;
                    --editor-header-bg: #11241c;
                    --editor-status-bg: #102019;
                }

                .ide-ws-editor[data-editor-theme="graphite"] {
                    --editor-bg: #1a1c21;
                    --editor-text: #d5d7dc;
                    --editor-muted: #a8aeb8;
                    --editor-header-bg: #22252d;
                    --editor-status-bg: #1f2229;
                }

                .ide-ws-editor-header {
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    padding: 0.5rem 0.9rem;
                    background: var(--editor-header-bg);
                    border-bottom: 1px solid var(--border-color);
                    gap: 0.6rem;
                }

                .ide-ws-editor-filename {
                    font-size: 0.82rem;
                    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
                    color: var(--text-secondary);
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                }

                .ide-ws-editor-actions {
                    display: flex;
                    align-items: center;
                    gap: 0.4rem;
                    flex-shrink: 0;
                }

                .ide-ws-language-pill {
                    font-size: 0.72rem;
                    letter-spacing: 0.02em;
                    color: var(--editor-accent);
                    border: 1px solid color-mix(in srgb, var(--editor-accent) 45%, transparent);
                    background: color-mix(in srgb, var(--editor-accent) 14%, transparent);
                    padding: 0.22rem 0.5rem;
                    border-radius: 999px;
                    white-space: nowrap;
                }

                .ide-ws-select-wrap {
                    display: inline-flex;
                    align-items: center;
                    gap: 0.32rem;
                    font-size: 0.68rem;
                    color: var(--editor-muted);
                }

                .ide-ws-select {
                    border: 1px solid var(--border-color);
                    border-radius: 5px;
                    background: color-mix(in srgb, var(--editor-bg) 85%, #fff 15%);
                    color: var(--editor-text);
                    padding: 0.16rem 0.38rem;
                    font-size: 0.72rem;
                    outline: none;
                }

                .ide-ws-select:focus {
                    border-color: var(--editor-accent);
                }

                .ide-ws-code {
                    flex: 1;
                    width: 100%;
                    resize: none;
                    border: none;
                    outline: none;
                    background: var(--editor-bg);
                    color: var(--editor-text);
                    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
                    font-size: 0.9rem;
                    line-height: 1.6;
                    padding: 1rem 1.2rem;
                    tab-size: 4;
                    min-height: 0;
                    caret-color: var(--editor-accent);
                }

                .ide-ws-code::selection {
                    background: var(--editor-selection);
                }

                .ide-ws-code-wrap {
                    position: relative;
                    flex: 1;
                    min-height: 0;
                    overflow: hidden;
                    background: var(--editor-bg);
                }

                .ide-ws-highlight {
                    position: absolute;
                    inset: 0;
                    margin: 0;
                    overflow: auto;
                    background: transparent;
                    color: var(--editor-text);
                    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
                    font-size: 0.9rem;
                    line-height: 1.6;
                    padding: 1rem 1.2rem;
                    tab-size: 4;
                    pointer-events: none;
                    white-space: pre;
                }

                .ide-ws-highlight code {
                    display: block;
                    white-space: pre;
                    color: var(--editor-text);
                }

                .ide-ws-code-input {
                    position: absolute;
                    inset: 0;
                    background: transparent;
                    color: transparent;
                    -webkit-text-fill-color: transparent;
                    z-index: 1;
                }

                .ide-ws-code-input::placeholder {
                    color: var(--editor-muted);
                    -webkit-text-fill-color: var(--editor-muted);
                }

                .tok-keyword { color: color-mix(in srgb, var(--editor-accent) 82%, #f8f8f8 18%); font-weight: 600; }
                .tok-type { color: color-mix(in srgb, var(--editor-accent) 55%, #9fe3ff 45%); }
                .tok-string { color: #89d185; }
                .tok-comment { color: #6f8097; font-style: italic; }
                .tok-number { color: #7ed0ff; }
                .tok-op { color: #b7c6d8; }

                .ide-ws-editor[data-editor-theme="dusk"] .tok-string { color: #b6e387; }
                .ide-ws-editor[data-editor-theme="dusk"] .tok-comment { color: #8b83b3; }
                .ide-ws-editor[data-editor-theme="dusk"] .tok-number { color: #9ee3ff; }

                .ide-ws-editor[data-editor-theme="forest"] .tok-string { color: #9ee495; }
                .ide-ws-editor[data-editor-theme="forest"] .tok-comment { color: #7f9f8f; }
                .ide-ws-editor[data-editor-theme="forest"] .tok-number { color: #78d8ba; }

                .ide-ws-editor[data-editor-theme="graphite"] .tok-string { color: #8ddf9d; }
                .ide-ws-editor[data-editor-theme="graphite"] .tok-comment { color: #838b95; }
                .ide-ws-editor[data-editor-theme="graphite"] .tok-number { color: #8dc6ff; }

                .ide-ws-status-bar {
                    margin: 0;
                    padding: 0.35rem 0.9rem;
                    font-size: 0.78rem;
                    color: var(--editor-muted);
                    background: var(--editor-status-bg);
                    border-top: 1px solid var(--border-color);
                }

                .ide-grid {
                    display: grid;
                    grid-template-columns: 280px 1fr 320px;
                    gap: 1rem;
                }

                .ide-ws-palette {
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                    gap: 0.75rem;
                    padding: 0.55rem 0.75rem;
                    margin-bottom: 0.75rem;
                    border: 1px solid var(--border-color);
                    border-radius: 10px;
                    background: linear-gradient(180deg, rgba(11, 22, 40, 0.92), rgba(8, 16, 28, 0.96));
                    position: sticky;
                    top: 0;
                    z-index: 10;
                    backdrop-filter: blur(8px);
                }

                .ide-ws-palette-group {
                    display: flex;
                    align-items: center;
                    gap: 0.5rem;
                    flex-wrap: wrap;
                }

                .ide-ws-icon-btn {
                    width: 2rem;
                    height: 2rem;
                    display: inline-flex;
                    align-items: center;
                    justify-content: center;
                    border: 1px solid var(--border-color);
                    border-radius: 8px;
                    background: rgba(255,255,255,0.03);
                    color: var(--text-secondary);
                    cursor: pointer;
                    transition: background 0.14s ease, border-color 0.14s ease, color 0.14s ease, transform 0.14s ease;
                    flex: 0 0 auto;
                }

                .ide-ws-icon-btn:hover {
                    background: rgba(59, 130, 246, 0.12);
                    border-color: rgba(96, 165, 250, 0.4);
                    color: var(--text-primary);
                    transform: translateY(-1px);
                }

                .ide-ws-icon {
                    font-size: 0.9rem;
                    line-height: 1;
                }

                .ide-ws-tab-strip {
                    display: inline-flex;
                    align-items: stretch;
                    gap: 0.35rem;
                    padding-left: 0.25rem;
                    margin-left: 0.25rem;
                    border-left: 1px solid rgba(148,163,184,0.16);
                    flex-wrap: wrap;
                }

                .ide-ws-tab {
                    display: inline-flex;
                    align-items: center;
                    gap: 0.45rem;
                    padding: 0.42rem 0.7rem;
                    border: 1px solid var(--border-color);
                    border-radius: 999px;
                    background: rgba(255,255,255,0.02);
                    color: var(--text-secondary);
                    cursor: pointer;
                    transition: background 0.14s ease, border-color 0.14s ease, color 0.14s ease, transform 0.14s ease;
                    white-space: nowrap;
                }

                .ide-ws-tab:hover {
                    background: rgba(255,255,255,0.05);
                    color: var(--text-primary);
                    transform: translateY(-1px);
                }

                .ide-ws-tab.active {
                    background: rgba(59,130,246,0.15);
                    border-color: rgba(96,165,250,0.42);
                    color: #eff6ff;
                    box-shadow: inset 0 0 0 1px rgba(96,165,250,0.12);
                }

                .ide-ws-tab-icon {
                    font-size: 0.85rem;
                    line-height: 1;
                }

                .ide-ws-tab-text {
                    font-size: 0.78rem;
                    font-weight: 600;
                }

                .ide-ws-palette-hint {
                    font-size: 0.7rem;
                    letter-spacing: 0.06em;
                    text-transform: uppercase;
                    color: var(--text-muted);
                    margin-right: 0.2rem;
                }

                .ide-ws-palette-right {
                    justify-content: flex-end;
                    margin-left: auto;
                }

                .ide-ws-resizer {
                    flex: 0 0 10px;
                    align-self: stretch;
                    cursor: col-resize;
                    border-radius: 999px;
                    background: linear-gradient(180deg, rgba(148,163,184,0.25), rgba(59,130,246,0.42), rgba(148,163,184,0.25));
                    box-shadow: inset 0 0 0 1px rgba(148,163,184,0.16);
                    transition: background 0.15s ease, box-shadow 0.15s ease;
                }

                .ide-ws-resizer:hover {
                    background: linear-gradient(180deg, rgba(148,163,184,0.35), rgba(59,130,246,0.65), rgba(148,163,184,0.35));
                }

                .ide-ws-resizer-left {
                    margin-right: -0.15rem;
                }

                .ide-ws-resizer-right {
                    margin-left: -0.15rem;
                }

                .hidden-pane {
                    display: none !important;
                }

                .ide-panel {
                    background: var(--bg-secondary);
                    border: 1px solid var(--border-color);
                    border-radius: 10px;
                    padding: 0.9rem;
                    display: flex;
                    flex-direction: column;
                    gap: 0.65rem;
                }

                .ide-panel h2 {
                    margin: 0;
                    font-size: 0.95rem;
                }

                .field {
                    display: flex;
                    flex-direction: column;
                    gap: 0.28rem;
                    font-size: 0.82rem;
                    color: var(--text-secondary);
                }

                .field input,
                .field select,
                .field textarea {
                    border: 1px solid var(--border-color);
                    border-radius: 6px;
                    background: var(--card-bg);
                    color: var(--text-primary);
                    padding: 0.5rem 0.65rem;
                    font-size: 0.9rem;
                }

                .field textarea {
                    resize: vertical;
                    min-height: 180px;
                    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
                }

                .ide-actions .btn {
                    width: 100%;
                    text-align: center;
                }

                .submit-message {
                    margin: 0;
                    color: var(--text-primary);
                    font-size: 0.86rem;
                }

                .ide-divider {
                    width: 100%;
                    border: 0;
                    border-top: 1px solid var(--border-color);
                    margin: 0.4rem 0;
                }

                .starter-heading {
                    margin: 0;
                    font-size: 0.92rem;
                }

                .starter-meta {
                    margin: 0;
                    color: var(--text-secondary);
                    font-size: 0.8rem;
                    word-break: break-all;
                }

                .workspace-list {
                    display: flex;
                    flex-direction: column;
                    gap: 0.35rem;
                    max-height: 240px;
                    overflow: auto;
                }

                .workspace-item {
                    display: flex;
                    gap: 0.35rem;
                    align-items: center;
                }

                .workspace-path {
                    flex: 1;
                    border: 1px solid var(--border-color);
                    border-radius: 6px;
                    background: var(--card-bg);
                    color: var(--text-primary);
                    text-align: left;
                    padding: 0.32rem 0.5rem;
                    font-size: 0.8rem;
                    cursor: pointer;
                }

                .workspace-path.active {
                    border-color: var(--accent-color);
                    box-shadow: 0 0 0 1px var(--accent-color);
                }

                .workspace-remove {
                    border: 1px solid var(--border-color);
                    border-radius: 6px;
                    background: transparent;
                    color: var(--text-secondary);
                    padding: 0.3rem 0.5rem;
                    cursor: pointer;
                    font-size: 0.75rem;
                }

                .ide-terminal-dock {
                    margin-top: 0;
                    flex: 0 0 auto;
                    border: 1px solid var(--border-color);
                    border-radius: 0;
                    overflow: hidden;
                    background: #08111f;
                    border-left: none;
                    border-right: none;
                    border-bottom: none;
                }

                .ide-terminal-resizer {
                    flex: 0 0 10px;
                    cursor: row-resize;
                    border-radius: 999px;
                    background: linear-gradient(90deg, rgba(148,163,184,0.25), rgba(59,130,246,0.42), rgba(148,163,184,0.25));
                    box-shadow: inset 0 0 0 1px rgba(148,163,184,0.16);
                    transition: background 0.15s ease, box-shadow 0.15s ease;
                }

                .ide-terminal-resizer:hover {
                    background: linear-gradient(90deg, rgba(148,163,184,0.35), rgba(59,130,246,0.65), rgba(148,163,184,0.35));
                }

                .ide-terminal-header {
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                    gap: 0.6rem;
                    padding: 0.6rem 0.8rem;
                    border-bottom: 1px solid #1f2937;
                    color: #d7e0ea;
                    background: #0b1628;
                }

                .ide-terminal-header h3 {
                    margin: 0;
                    font-size: 0.9rem;
                }

                .ide-terminal-header span {
                    font-size: 0.72rem;
                    color: #9fb4c9;
                    max-width: 50%;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                }

                .ide-terminal-header-actions {
                    display: flex;
                    align-items: center;
                    justify-content: flex-end;
                    gap: 0.45rem;
                    flex: 1;
                    min-width: 0;
                }

                .response-box {
                    margin: 0;
                    max-height: 280px;
                    overflow: auto;
                    background: var(--card-bg);
                    border: 1px solid var(--border-color);
                    border-radius: 8px;
                    padding: 0.7rem;
                    font-size: 0.78rem;
                    color: var(--text-primary);
                }

                .upload-row {
                    display: flex;
                    gap: 0.5rem;
                    align-items: center;
                    flex-wrap: wrap;
                }

                /* Navigator Toolbar v2 - Icon-based */
                .ide-ws-toolbar-v2 {
                    display: flex;
                    align-items: center;
                    gap: 0.2rem;
                    padding: 0.35rem 0.45rem;
                    background: linear-gradient(180deg, rgba(17, 24, 39, 0.84), rgba(13, 19, 31, 0.78));
                    border-bottom: 1px solid var(--border-color);
                    flex-wrap: nowrap;
                }

                .ide-ws-toolbar-btn {
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    width: 26px;
                    height: 26px;
                    border: 1px solid rgba(148, 163, 184, 0.14);
                    border-radius: 5px;
                    background: rgba(15, 23, 42, 0.25);
                    color: #c7d2e3;
                    cursor: pointer;
                    font-size: 0.84rem;
                    line-height: 1;
                    transition: all 0.16s ease;
                    padding: 0;
                }

                .ide-ws-toolbar-btn:hover {
                    background: rgba(59, 130, 246, 0.16);
                    border-color: rgba(96, 165, 250, 0.42);
                    color: #ecf4ff;
                    transform: translateY(-1px);
                }

                .ide-ws-toolbar-btn:active {
                    transform: translateY(0);
                    background: rgba(37, 99, 235, 0.24);
                    border-color: rgba(96, 165, 250, 0.56);
                }

                .ide-file-input-label {
                    display: flex !important;
                    align-items: center;
                    justify-content: center;
                }

                .ide-ws-toolbar-divider {
                    width: 1px;
                    height: 18px;
                    background: rgba(148, 163, 184, 0.3);
                    margin: 0 0.2rem;
                }

                .ide-ws-saved-projects {
                    display: flex;
                    flex-direction: column;
                    gap: 0.4rem;
                    padding: 0.5rem;
                    background: rgba(15, 23, 42, 0.5);
                    border-bottom: 1px solid var(--border-color);
                }
                .ide-ws-saved-projects-header {
                    display: grid;
                    grid-template-columns: auto 1fr auto;
                    align-items: center;
                    gap: 0.4rem;
                    font-size: 0.74rem;
                    color: #a3b3c8;
                    letter-spacing: 0.04em;
                    text-transform: uppercase;
                }
                .ide-ws-saved-refresh {
                    padding: 0.15rem 0.45rem !important;
                    font-size: 0.85rem !important;
                    line-height: 1 !important;
                }
                .ide-ws-saved-empty {
                    padding: 0.4rem 0.2rem;
                    font-size: 0.78rem;
                    color: #7d8aa0;
                    font-style: italic;
                }
                .ide-ws-saved-list {
                    list-style: none;
                    margin: 0;
                    padding: 0;
                    display: flex;
                    flex-direction: column;
                    gap: 0.25rem;
                    max-height: 180px;
                    overflow-y: auto;
                }
                .ide-ws-saved-item {
                    display: grid;
                    grid-template-columns: 1fr auto;
                    gap: 0.25rem;
                    align-items: stretch;
                }
                .ide-ws-saved-load {
                    display: flex;
                    flex-direction: column;
                    align-items: flex-start;
                    text-align: left;
                    padding: 0.35rem 0.5rem;
                    border: 1px solid rgba(148, 163, 184, 0.14);
                    border-radius: 5px;
                    background: rgba(15, 23, 42, 0.4);
                    color: #d6e0ee;
                    cursor: pointer;
                    transition: all 0.14s ease;
                }
                .ide-ws-saved-load:hover {
                    background: rgba(59, 130, 246, 0.16);
                    border-color: rgba(96, 165, 250, 0.42);
                    color: #ecf4ff;
                }
                .ide-ws-saved-name {
                    font-size: 0.84rem;
                    font-weight: 600;
                }
                .ide-ws-saved-meta {
                    font-size: 0.7rem;
                    color: #8c9aaf;
                    margin-top: 0.1rem;
                }
                .ide-ws-saved-delete {
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    width: 28px;
                    border: 1px solid rgba(220, 38, 38, 0.25);
                    border-radius: 5px;
                    background: rgba(127, 29, 29, 0.18);
                    color: #fca5a5;
                    cursor: pointer;
                    font-size: 0.85rem;
                    transition: all 0.14s ease;
                }
                .ide-ws-saved-delete:hover {
                    background: rgba(220, 38, 38, 0.28);
                    color: #fff;
                }

                .ide-ws-quick-create {
                    display: grid;
                    grid-template-columns: 1fr auto;
                    gap: 0.35rem;
                    padding: 0.45rem;
                    border-bottom: 1px solid var(--border-color);
                    background: rgba(15, 23, 42, 0.3);
                }

                .ide-ws-create-input {
                    min-width: 0;
                    border: 1px solid rgba(148, 163, 184, 0.28);
                    border-radius: 6px;
                    background: rgba(3, 8, 18, 0.55);
                    color: var(--text-primary);
                    padding: 0.36rem 0.5rem;
                    font-size: 0.76rem;
                    outline: none;
                }

                .ide-ws-create-input:focus {
                    border-color: rgba(96, 165, 250, 0.7);
                    box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.18);
                }

                .ide-ws-create-actions {
                    display: flex;
                    gap: 0.3rem;
                }

                .ide-ws-mini-btn {
                    border: 1px solid rgba(148, 163, 184, 0.25);
                    border-radius: 6px;
                    background: rgba(30, 41, 59, 0.6);
                    color: #dbe7fb;
                    padding: 0.34rem 0.5rem;
                    font-size: 0.7rem;
                    font-weight: 600;
                    cursor: pointer;
                    letter-spacing: 0.01em;
                    transition: all 0.15s ease;
                }

                .ide-ws-mini-btn:hover {
                    background: rgba(59, 130, 246, 0.2);
                    border-color: rgba(96, 165, 250, 0.52);
                }

                .ide-ws-mini-btn:active {
                    background: rgba(37, 99, 235, 0.26);
                }

                .ide-ws-context-row {
                    display: flex;
                    align-items: center;
                    gap: 0.4rem;
                    padding: 0.35rem 0.5rem;
                    border-bottom: 1px solid var(--border-color);
                    background: rgba(8, 14, 27, 0.42);
                }

                .ide-ws-context-label {
                    color: var(--text-secondary);
                    font-size: 0.68rem;
                    letter-spacing: 0.04em;
                    text-transform: uppercase;
                    flex-shrink: 0;
                }

                .ide-ws-context-path {
                    border: 1px solid rgba(148, 163, 184, 0.25);
                    border-radius: 6px;
                    background: rgba(15, 23, 42, 0.45);
                    color: #dbe7fb;
                    font-size: 0.72rem;
                    text-align: left;
                    padding: 0.2rem 0.42rem;
                    min-width: 0;
                    width: 100%;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                    cursor: pointer;
                }

                .ide-ws-context-path:hover {
                    border-color: rgba(96, 165, 250, 0.48);
                    background: rgba(37, 99, 235, 0.2);
                }

                .ide-ws-file-name.folder {
                    color: #9cc6ff;
                    font-style: italic;
                }

                .ide-ws-folder-toggle {
                    background: none;
                    border: none;
                    color: var(--text-secondary);
                    cursor: pointer;
                    font-size: 0.65rem;
                    padding: 0.15rem 0.2rem;
                    min-width: 16px;
                    text-align: center;
                    line-height: 1;
                    flex-shrink: 0;
                    transition: color 0.15s ease;
                }

                .ide-ws-folder-toggle:hover {
                    color: #dbe7fb;
                }

                .ide-ws-folder-spacer {
                    width: 16px;
                    flex-shrink: 0;
                }

                .hidden {
                    display: none !important;
                }

                .ide-ws-file-rename {
                    background: none;
                    border: none;
                    color: var(--text-secondary);
                    cursor: pointer;
                    font-size: 0.72rem;
                    opacity: 0;
                    padding: 0.1rem 0.3rem;
                    border-radius: 3px;
                }

                .ide-ws-file-row:hover .ide-ws-file-rename,
                .ide-ws-file-row:hover .ide-ws-file-remove {
                    opacity: 1;
                }

                .ide-ws-file-rename:hover,
                .ide-ws-file-remove:hover {
                    background: rgba(148, 163, 184, 0.18);
                    color: #edf4ff;
                }

                /* Build Results Dock */
                .ide-build-results-dock {
                    flex: 0 0 auto;
                    border: 1px solid var(--border-color);
                    border-radius: 0;
                    overflow: hidden;
                    background: #0d1520;
                    border-left: none;
                    border-right: none;
                    display: flex;
                    flex-direction: column;
                    min-height: 0;
                }

                .ide-build-resizer {
                    flex: 0 0 10px;
                    cursor: row-resize;
                    border-radius: 999px;
                    background: linear-gradient(90deg, rgba(148,163,184,0.25), rgba(59,130,246,0.42), rgba(148,163,184,0.25));
                    box-shadow: inset 0 0 0 1px rgba(148,163,184,0.16);
                    transition: background 0.15s ease, box-shadow 0.15s ease;
                }

                .ide-build-resizer:hover {
                    background: linear-gradient(90deg, rgba(148,163,184,0.35), rgba(59,130,246,0.65), rgba(148,163,184,0.35));
                }

                .ide-build-header {
                    display: flex;
                    justify-content: space-between;
                    align-items: center;
                    gap: 0.6rem;
                    padding: 0.6rem 0.8rem;
                    border-bottom: 1px solid #2a3d4a;
                    color: #d7e0ea;
                    background: #0f1d28;
                    flex-shrink: 0;
                }

                .ide-build-header h3 {
                    margin: 0;
                    font-size: 0.9rem;
                    font-weight: 600;
                }

                .ide-build-header-actions {
                    display: flex;
                    align-items: center;
                    gap: 0.3rem;
                    margin-left: auto;
                }

                .ide-build-output {
                    flex: 1;
                    margin: 0;
                    padding: 0.8rem;
                    background: #08111f;
                    color: #9fb4c9;
                    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
                    font-size: 0.8rem;
                    line-height: 1.5;
                    overflow-y: auto;
                    white-space: pre-wrap;
                    word-break: break-word;
                    border: none;
                }

                /* Dock button styles */
                .ide-dock-btn {
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    width: 28px;
                    height: 28px;
                    border: 1px solid transparent;
                    border-radius: 4px;
                    background: transparent;
                    color: #9fb4c9;
                    cursor: pointer;
                    font-size: 1rem;
                    transition: all 0.15s ease;
                    padding: 0;
                }

                .ide-dock-btn:hover {
                    background: rgba(59, 130, 246, 0.15);
                    color: #d7e0ea;
                }

                .ide-dock-btn-secondary:hover {
                    background: rgba(239, 68, 68, 0.15);
                    color: #ff8c8c;
                }

                .ide-dock-path {
                    font-size: 0.72rem;
                    color: #7a8fa6;
                    max-width: 45%;
                    overflow: hidden;
                    text-overflow: ellipsis;
                    white-space: nowrap;
                    flex: 1;
                }

                @media (max-width: 1200px) {
                    .ide-grid {
                        grid-template-columns: 1fr;
                    }
                }
            "#}
        </style>
    }
}

fn platform_label(platform: &str) -> &'static str {
    match platform {
        "ios" => "iOS",
        "android" => "Android",
        "iot" => "IoT",
        "quantum" => "Quantum",
        _ => "Platform",
    }
}

fn build_platform_starter_template(
    platform: &str,
    app_name: &str,
    organization_id: &str,
    board_profile: &str,
    backend_name: &str,
    interface_style: &str,
    android_application_id: &str,
) -> (String, Vec<WorkspaceProjectFile>, String) {
    let safe_name = if app_name.trim().is_empty() { "EdgeStarter" } else { app_name.trim() };

    match platform {
        "ios" => {
            let project_path = format!("/workspace/{safe_name}");
            let files = vec![
                WorkspaceProjectFile {
                    path: "App.swift".to_string(),
                    content: format!(
                        "import SwiftUI\n\n@main\nstruct {name}App: App {{\n    var body: some Scene {{\n        WindowGroup {{\n            ContentView()\n        }}\n    }}\n}}\n",
                        name = safe_name.replace('-', "")
                    ),
                },
                WorkspaceProjectFile {
                    path: "ContentView.swift".to_string(),
                    content: if interface_style == "uikit" {
                        "import UIKit\n\nfinal class ViewController: UIViewController {\n    override func viewDidLoad() {\n        super.viewDidLoad()\n        view.backgroundColor = .systemBackground\n    }\n}\n".to_string()
                    } else {
                        format!(
                            "import SwiftUI\n\nstruct ContentView: View {{\n    var body: some View {{\n        VStack(alignment: .leading, spacing: 12) {{\n            Text(\"{safe_name}\")\n                .font(.largeTitle.bold())\n            Text(\"Organization: {organization_id}\")\n                .foregroundStyle(.secondary)\n            Text(\"Welcome to the EdgeFabric iOS starter workspace.\")\n        }}\n        .padding()\n    }}\n}}\n"
                        )
                    },
                },
                WorkspaceProjectFile {
                    path: "README.md".to_string(),
                    content: "# iOS Starter\n\nThis workspace is a lightweight starter loaded directly into Development IDE. Use the remote starter flow for a full Xcode project archive.\n".to_string(),
                },
            ];
            (project_path, files, "ContentView.swift".to_string())
        }
        "android" => {
            let project_path = format!("/workspace/{safe_name}-android");
            let package_path = android_application_id.replace('.', "/");
            let files = vec![
                WorkspaceProjectFile {
                    path: "settings.gradle.kts".to_string(),
                    content: format!("rootProject.name = \"{safe_name}\"\ninclude(\":app\")\n"),
                },
                WorkspaceProjectFile {
                    path: "app/build.gradle.kts".to_string(),
                    content: format!(
                        "plugins {{\n    id(\"com.android.application\") version \"8.6.0\"\n    id(\"org.jetbrains.kotlin.android\") version \"1.9.24\"\n}}\n\nandroid {{\n    namespace = \"{app_id}\"\n    compileSdk = 35\n\n    defaultConfig {{\n        applicationId = \"{app_id}\"\n        minSdk = 26\n        targetSdk = 35\n        versionCode = 1\n        versionName = \"1.0\"\n    }}\n}}\n\ndependencies {{\n    implementation(\"androidx.core:core-ktx:1.13.1\")\n    implementation(\"androidx.activity:activity-compose:1.9.0\")\n    implementation(\"androidx.compose.material3:material3:1.2.1\")\n}}\n",
                        app_id = android_application_id
                    ),
                },
                WorkspaceProjectFile {
                    path: format!("app/src/main/java/{package_path}/MainActivity.kt"),
                    content: format!(
                        "package {app_id}\n\nimport android.os.Bundle\nimport androidx.activity.ComponentActivity\nimport androidx.activity.compose.setContent\nimport androidx.compose.material3.MaterialTheme\nimport androidx.compose.material3.Surface\nimport androidx.compose.material3.Text\n\nclass MainActivity : ComponentActivity() {{\n    override fun onCreate(savedInstanceState: Bundle?) {{\n        super.onCreate(savedInstanceState)\n        setContent {{\n            MaterialTheme {{\n                Surface {{\n                    Text(\"{safe_name} starter loaded in EdgeFabric IDE\")\n                }}\n            }}\n        }}\n    }}\n}}\n",
                        app_id = android_application_id
                    ),
                },
            ];
            let active = format!("app/src/main/java/{package_path}/MainActivity.kt");
            (project_path, files, active)
        }
        "iot" => {
            let project_path = format!("/workspace/{safe_name}-firmware");
            let files = vec![
                WorkspaceProjectFile {
                    path: "platformio.ini".to_string(),
                    content: format!("[env:{board_profile}]\nplatform = espressif32\nboard = {board_profile}\nframework = arduino\nmonitor_speed = 115200\n"),
                },
                WorkspaceProjectFile {
                    path: "src/main.cpp".to_string(),
                    content: format!(
                        "#include <Arduino.h>\n\nvoid setup() {{\n  Serial.begin(115200);\n  pinMode(LED_BUILTIN, OUTPUT);\n}}\n\nvoid loop() {{\n  digitalWrite(LED_BUILTIN, HIGH);\n  Serial.println(\"{safe_name} heartbeat\");\n  delay(500);\n  digitalWrite(LED_BUILTIN, LOW);\n  delay(500);\n}}\n"
                    ),
                },
                WorkspaceProjectFile {
                    path: "README.md".to_string(),
                    content: "# IoT Starter\n\nStarter firmware workspace prepared in Development IDE. Adjust the board profile and deploy script before submitting.\n".to_string(),
                },
            ];
            (project_path, files, "src/main.cpp".to_string())
        }
        _ => {
            let project_path = format!("/workspace/{safe_name}-quantum");
            let backend_display = if backend_name.trim().is_empty() { "aer_simulator" } else { backend_name.trim() };
            let files = vec![
                WorkspaceProjectFile {
                    path: "Cargo.toml".to_string(),
                    content: format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde_json = \"1\"\n", safe_name.to_lowercase().replace(' ', "-")),
                },
                WorkspaceProjectFile {
                    path: "src/main.rs".to_string(),
                    content: format!(
                        "fn main() {{\n    println!(\"Launching {safe_name} on backend: {backend_display}\");\n    println!(\"qreg q[2];\");\n    println!(\"h q[0];\");\n    println!(\"cx q[0],q[1];\");\n    println!(\"measure q[0] -> c[0];\");\n}}\n"
                    ),
                },
                WorkspaceProjectFile {
                    path: "circuits/bell.qasm".to_string(),
                    content: "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[2];\ncreg c[2];\nh q[0];\ncx q[0], q[1];\nmeasure q -> c;\n".to_string(),
                },
            ];
            (project_path, files, "circuits/bell.qasm".to_string())
        }
    }
}

const DEV_IDE_TERM_KEY: &str = "__ef_dev_ide_term";
const DEV_IDE_WS_KEY: &str = "__ef_dev_ide_ws";

#[component]
fn DevIdeShellTerminal(ws_path: ReadSignal<String>, nonce: ReadSignal<u64>) -> impl IntoView {
    let (connected, set_connected) = signal(false);
    let socket_ref = StoredValue::new(None::<web_sys::WebSocket>);
    let term_ref = NodeRef::<leptos::html::Div>::new();

    let term_write = |text: &str| {
        let Some(win) = web_sys::window() else { return };
        let term = match js_sys::Reflect::get(&win, &wasm_bindgen::JsValue::from_str(DEV_IDE_TERM_KEY)) {
            Ok(t) if !t.is_undefined() && !t.is_null() => t,
            _ => return,
        };
        if let Ok(write_fn) = js_sys::Reflect::get(&term, &wasm_bindgen::JsValue::from_str("write")) {
            if let Ok(func) = write_fn.dyn_into::<js_sys::Function>() {
                let _ = func.call1(&term, &wasm_bindgen::JsValue::from_str(text));
            }
        }
    };

    Effect::new(move || {
        let _nonce = nonce.get();
        let Some(_el) = term_ref.get() else { return };
        let init_js = format!(
            r#"(function() {{
                if (typeof Terminal === 'undefined') return null;
                var existing = window.{term_key};
                if (existing) {{ try {{ existing.dispose(); }} catch(e) {{}} window.{term_key} = null; }}
                var term = new Terminal({{
                    cursorBlink: true,
                    fontSize: 13,
                    fontFamily: "ui-monospace, 'JetBrains Mono', 'Fira Code', 'SF Mono', Menlo, Consolas, monospace",
                    theme: {{
                        background: '#08111f',
                        foreground: '#d7e0ea',
                        cursor: '#67e8f9',
                        selectionBackground: '#264f78',
                    }},
                    scrollback: 10000,
                    convertEol: false,
                    allowProposedApi: true,
                }});
                var container = document.getElementById('ef-dev-ide-shell-term');
                if (!container) return null;
                term.open(container);
                if (typeof FitAddon !== 'undefined') {{
                    var fitAddon = new FitAddon.FitAddon();
                    term.loadAddon(fitAddon);
                    try {{ fitAddon.fit(); }} catch(e) {{}}
                    window.__ef_dev_ide_fit = fitAddon;
                    try {{
                        var ro = new ResizeObserver(function() {{ try {{ fitAddon.fit(); }} catch(e) {{}} }});
                        ro.observe(container);
                        window.__ef_dev_ide_ro = ro;
                    }} catch(e) {{}}
                }}
                if (typeof WebLinksAddon !== 'undefined') {{
                    term.loadAddon(new WebLinksAddon.WebLinksAddon());
                }}
                term.onData(function(data) {{
                    var ws = window.{ws_key};
                    if (ws && ws.readyState === 1) {{ ws.send(data); }}
                }});
                term.focus();
                window.{term_key} = term;
                return term;
            }})()"#,
            term_key = DEV_IDE_TERM_KEY,
            ws_key = DEV_IDE_WS_KEY,
        );
        let _ = js_sys::eval(&init_js);
    });

    Effect::new(move || {
        let _nonce = nonce.get();
        let ws_path_value = ws_path.get();
        if ws_path_value.trim().is_empty() {
            return;
        }
        if term_ref.get().is_none() {
            return;
        }

        if let Some(existing) = socket_ref.get_value() {
            let _ = existing.close();
        }
        set_connected.set(false);

        let Some(window) = web_sys::window() else { return };
        let location = window.location();
        let protocol = location.protocol().unwrap_or_else(|_| "http:".to_string());
        let host = location.hostname().unwrap_or_else(|_| "localhost".to_string());
        let scheme = if protocol == "https:" { "wss" } else { "ws" };
        let ws_url = format!("{scheme}://{host}:6060{ws_path_value}");
        let ws = match web_sys::WebSocket::new(&ws_url) {
            Ok(ws) => ws,
            Err(_) => return,
        };
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        if let Some(win) = web_sys::window() {
            let _ = js_sys::Reflect::set(&win, &wasm_bindgen::JsValue::from_str(DEV_IDE_WS_KEY), &ws);
        }

        let onmessage = Closure::<dyn FnMut(_)>::new(move |event: web_sys::MessageEvent| {
            if let Some(text) = event.data().as_string() {
                let trimmed = text.trim_start();
                if trimmed.starts_with('{') && trimmed.contains("\"shell_ready\"") {
                    return;
                }
                term_write(&text);
                return;
            }
            if let Ok(buf) = event.data().dyn_into::<js_sys::ArrayBuffer>() {
                let bytes = js_sys::Uint8Array::new(&buf);
                let mut out = vec![0u8; bytes.length() as usize];
                bytes.copy_to(&mut out);
                term_write(&String::from_utf8_lossy(&out));
            }
        });
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        let onopen = Closure::<dyn FnMut(_)>::new(move |_event: web_sys::Event| {
            set_connected.set(true);
        });
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();

        let onclose = Closure::<dyn FnMut(_)>::new(move |event: web_sys::CloseEvent| {
            set_connected.set(false);
            let code = event.code();
            let reason = event.reason();
            let msg = format!(
                "\r\n\x1b[2m[disconnected · code {code}{}]\x1b[0m\r\n",
                if reason.is_empty() { String::new() } else { format!(" · {reason}") }
            );
            term_write(&msg);
        });
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();

        let onerror = Closure::<dyn FnMut(_)>::new(move |_event: web_sys::Event| {
            term_write("\r\n\x1b[31m[websocket error · check API is reachable]\x1b[0m\r\n");
        });
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();

        socket_ref.set_value(Some(ws));
    });

    on_cleanup(move || {
        if let Some(ws) = socket_ref.get_value() {
            let _ = ws.close();
        }
        let cleanup_js = format!(
            r#"(function() {{
                try {{ if (window.__ef_dev_ide_ro) {{ window.__ef_dev_ide_ro.disconnect(); }} }} catch(e) {{}}
                try {{ if (window.{term_key}) {{ window.{term_key}.dispose(); }} }} catch(e) {{}}
                window.{term_key} = null;
                window.{ws_key} = null;
                window.__ef_dev_ide_fit = null;
                window.__ef_dev_ide_ro = null;
            }})()"#,
            term_key = DEV_IDE_TERM_KEY,
            ws_key = DEV_IDE_WS_KEY,
        );
        let _ = js_sys::eval(&cleanup_js);
    });

    let focus_terminal = move |_: web_sys::MouseEvent| {
        let _ = js_sys::eval(&format!(
            "window.{}.focus && window.{}.focus()",
            DEV_IDE_TERM_KEY, DEV_IDE_TERM_KEY
        ));
    };

    view! {
        <div style="display:flex; flex-direction:column; min-height:26rem;">
            <div style="display:flex; justify-content:space-between; align-items:center; padding:0.45rem 0.7rem; color:#d7e0ea;">
                <strong>{move || if connected.get() { "Connected" } else { "Connecting..." }}</strong>
                <span style="font-size:0.75rem; color:#9fb4c9;">"Full shell terminal at bottom panel"</span>
            </div>
            <div
                node_ref=term_ref
                id="ef-dev-ide-shell-term"
                on:click=focus_terminal
                style="margin:0; padding:0.5rem; flex:1; min-height:24rem; background:#08111f; overflow:hidden;"
            ></div>
        </div>
    }
}
