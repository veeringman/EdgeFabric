use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

use crate::api::{
    self, SubmitIotDeployRequest, SubmitQuantumProgramRequest,
};

#[component]
pub fn DevelopmentIdePage() -> impl IntoView {
    let (mode, set_mode) = signal("iot".to_string());
    let (source_ref, set_source_ref) = signal("project-main".to_string());
    let (project_path, set_project_path) = signal(String::new());
    let (region, set_region) = signal(String::new());
    let (ssh_user, set_ssh_user) = signal("builder".to_string());

    let (iot_target_device_id, set_iot_target_device_id) = signal(String::new());
    let (iot_board_profile, set_iot_board_profile) = signal("esp32c6".to_string());
    let (iot_deploy_script, set_iot_deploy_script) = signal(String::new());

    let (quantum_backend_kind, set_quantum_backend_kind) = signal("virtual".to_string());
    let (quantum_backend_name, set_quantum_backend_name) = signal(String::new());
    let (quantum_veeros_host_hint, set_quantum_veeros_host_hint) = signal(String::new());
    let (quantum_run_script, set_quantum_run_script) = signal(String::new());
    let (source_file_name, set_source_file_name) = signal("source.txt".to_string());
    let (source_code, set_source_code) = signal(String::new());

    let (submitting, set_submitting) = signal(false);
    let (message, set_message) = signal(Option::<String>::None);
    let (last_response, set_last_response) = signal(String::new());

    let trim_opt = |value: String| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() { None } else { Some(trimmed) }
    };

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_submitting.set(true);
        set_message.set(None);
        set_last_response.set(String::new());

        let mode_value = mode.get_untracked();
        let source_ref_value = source_ref.get_untracked();
        let project_path_value = project_path.get_untracked();
        let region_value = trim_opt(region.get_untracked());
        let ssh_user_value = trim_opt(ssh_user.get_untracked());

        leptos::task::spawn_local(async move {
            let outcome = if mode_value == "iot" {
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
            } else {
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
            };

            match outcome {
                Ok(value) => {
                    set_message.set(Some(if mode_value == "iot" {
                        "IoT build and deploy job submitted.".to_string()
                    } else {
                        "Quantum build and run job submitted.".to_string()
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
            filename = if mode.get_untracked() == "iot" {
                "iot-source.txt".to_string()
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

    view! {
        <div class="page-container dev-ide-page">
            <div class="page-header">
                <h1>"Development IDE"</h1>
                <p class="page-subtitle">
                    "Create IoT and Quantum build jobs directly from the dashboard with editable build/deploy commands."
                </p>
            </div>

            <div class="dev-ide-shell">
                <div class="ide-mode-switch">
                    <button class:active=move || mode.get() == "iot" on:click=move |_| set_mode.set("iot".to_string())>
                        "IoT IDE"
                    </button>
                    <button class:active=move || mode.get() == "quantum" on:click=move |_| set_mode.set("quantum".to_string())>
                        "Quantum IDE"
                    </button>
                </div>

                <form on:submit=submit class="ide-grid">
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

                    <Show when=move || mode.get() == "iot">
                        <section class="ide-panel ide-editor">
                            <h2>"IoT Build + Deploy"</h2>
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

                    <Show when=move || mode.get() == "quantum">
                        <section class="ide-panel ide-editor">
                            <h2>"Quantum Build + Run"</h2>
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

                    <section class="ide-panel ide-actions">
                        <h2>"Actions"</h2>
                        <div class="field">
                            <span>"Source File Name"</span>
                            <input type="text" prop:value=source_file_name on:input=move |ev| set_source_file_name.set(event_target_value(&ev))/>
                        </div>
                        <div class="upload-row">
                            <input type="file" accept=".qasm,.rs,.c,.cpp,.h,.hpp,.py,.js,.ts,.txt,.toml,.yaml,.yml,.json,.md" on:change=upload_source/>
                            <button class="btn btn-ghost" type="button" on:click=download_source>"Download Source"</button>
                        </div>
                        <label class="field">
                            <span>"Source Code"</span>
                            <textarea
                                rows="10"
                                placeholder="Paste or upload your source code here"
                                prop:value=source_code
                                on:input=move |ev| set_source_code.set(event_target_value(&ev))
                            ></textarea>
                        </label>
                        <button class="btn btn-primary" type="submit" disabled=move || submitting.get()>
                            {move || {
                                if submitting.get() {
                                    "Submitting..."
                                } else if mode.get() == "iot" {
                                    "Submit IoT Build + Deploy"
                                } else {
                                    "Submit Quantum Build + Run"
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

                .dev-ide-shell {
                    background: var(--card-bg);
                    border: 1px solid var(--border-color);
                    border-radius: 10px;
                    padding: 1rem;
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

                .ide-grid {
                    display: grid;
                    grid-template-columns: 280px 1fr 320px;
                    gap: 1rem;
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

                @media (max-width: 1200px) {
                    .ide-grid {
                        grid-template-columns: 1fr;
                    }
                }
            "#}
        </style>
    }
}
