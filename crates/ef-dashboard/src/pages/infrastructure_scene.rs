use leptos::prelude::*;
use serde_json::Value;
use std::time::Duration;

fn js_call(script: &str) {
    let _ = js_sys::eval(script);
}

fn node_label(node: &Value) -> String {
    node["attributes"]["name"].as_str()
        .or_else(|| node["attributes"]["hostname"].as_str())
        .or_else(|| node["id"].as_str())
        .unwrap_or("unnamed resource")
        .to_string()
}

#[component]
pub fn InfrastructureScenePage() -> impl IntoView {
    let (simulation_id, set_simulation_id) = signal(String::new());
    let (simulations, set_simulations) = signal(Vec::<Value>::new());
    let (scene, set_scene) = signal(Option::<Value>::None);
    let (selected, set_selected) = signal(String::new());
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (overlay_pcie, set_overlay_pcie) = signal(true);
    let (overlay_gpu, set_overlay_gpu) = signal(true);
    let (overlay_network, set_overlay_network) = signal(true);
    let (overlay_storage, set_overlay_storage) = signal(true);
    let (overlay_power, set_overlay_power) = signal(true);
    let canvas_id = "ef-infrastructure-scene-canvas";
    let (template_identity, set_template_identity) = signal("rubin-qualification-lab".to_string());
    let (template_scenario, set_template_scenario) = signal("rubin-cluster-baseline".to_string());
    let (template_hosts, set_template_hosts) = signal("4".to_string());
    let (template_gpus, set_template_gpus) = signal("8".to_string());
    let (template_generation, set_template_generation) = signal("6".to_string());
    let (template_gpu_memory, set_template_gpu_memory) = signal("192".to_string());
    let (template_peer_fabric, set_template_peer_fabric) = signal("nv_switch".to_string());
    let (creating, set_creating) = signal(false);

    // Load the tenant-scoped simulation registry for convenient ID discovery.
    Effect::new(move || {
        leptos::task::spawn_local(async move {
            if let Ok(response) = crate::api::get::<Value>("/infrastructure/simulations").await {
                set_simulations.set(response["simulations"].as_array().cloned().unwrap_or_default());
            }
        });
    });

    let create_simulation = move |_| {
        let identity = template_identity.get_untracked().trim().to_string();
        let scenario_id = template_scenario.get_untracked().trim().to_string();
        let hosts = template_hosts.get_untracked().parse::<u16>().unwrap_or(0);
        let gpus_per_host = template_gpus.get_untracked().parse::<u16>().unwrap_or(0);
        let pcie_generation = template_generation.get_untracked().parse::<u8>().unwrap_or(0);
        let gpu_memory_gib = template_gpu_memory.get_untracked().parse::<u32>().unwrap_or(0);
        let peer = template_peer_fabric.get_untracked();
        if identity.is_empty() || scenario_id.is_empty() {
            set_error.set(Some("Simulation identity and scenario are required.".into()));
            return;
        }
        let body = serde_json::json!({
            "identity": identity,
            "scenario_id": scenario_id,
            "hosts": hosts,
            "gpus_per_host": gpus_per_host,
            "pcie_generation": pcie_generation,
            "root_complex_lanes": 16,
            "switch_uplink_lanes": 16,
            "gpu_lanes": 16,
            "switch_downstream_ports": gpus_per_host,
            "gpu_memory_gib": gpu_memory_gib,
            "numa_nodes_per_host": 2,
            "gpu_peer_fabric": if peer == "none" { serde_json::Value::Null } else { serde_json::Value::String(peer) },
            "network_fabric": null,
            "storage": null,
            "cxl_memory": null,
            "power_cooling": null,
            "seed": 1
        });
        set_creating.set(true);
        set_error.set(None);
        leptos::task::spawn_local(async move {
            match crate::api::post::<_, Value>("/infrastructure/simulations/templates/gpu-cluster", &body).await {
                Ok(response) => {
                    let id = response["simulation"]["id"].as_str().unwrap_or("").to_string();
                    if id.is_empty() {
                        set_error.set(Some("Simulation was created, but the API response did not include its ID.".into()));
                    } else {
                        set_simulation_id.set(id.clone());
                        match crate::api::get::<Value>(&format!("/infrastructure/simulations/{id}/scene")).await {
                            Ok(document) => {
                                set_scene.set(Some(document));
                                set_selected.set(String::new());
                                if let Ok(list) = crate::api::get::<Value>("/infrastructure/simulations").await {
                                    set_simulations.set(list["simulations"].as_array().cloned().unwrap_or_default());
                                }
                            }
                            Err(err) => set_error.set(Some(format!("Simulation saved, but scene loading failed: {err}"))),
                        }
                    }
                }
                Err(err) => set_error.set(Some(err)),
            }
            set_creating.set(false);
        });
    };

    let load_scene = move |_| {
        let id = simulation_id.get_untracked().trim().to_string();
        if id.is_empty() {
            set_error.set(Some("Enter a simulation UUID.".into()));
            return;
        }
        set_loading.set(true);
        set_error.set(None);
        set_scene.set(None);
        set_selected.set(String::new());
        leptos::task::spawn_local(async move {
            match crate::api::get::<Value>(&format!("/infrastructure/simulations/{id}/scene")).await {
                Ok(value) => {
                    set_scene.set(Some(value));
                    set_error.set(None);
                }
                Err(err) => set_error.set(Some(err)),
            }
            set_loading.set(false);
        });
    };

    // Initialize/rebuild the WebGL scene when a new simulation is loaded.
    {
        Effect::new(move || {
            let current = scene.get();
            if let Some(document) = current {
                let json = serde_json::to_string(&document).unwrap_or_else(|_| "null".into());
                let script = format!(
                    "window.EfInfrastructureScene && window.EfInfrastructureScene.init({canvas_id:?}, {json})",
                    canvas_id = canvas_id,
                    json = json
                );
                let overlay_script = format!(
                    "window.EfInfrastructureScene && window.EfInfrastructureScene.setOverlays({canvas_id:?}, {{pcie:{pcie},gpu:{gpu},network:{network},storage:{storage},power:{power}}})",
                    canvas_id = canvas_id,
                    pcie = overlay_pcie.get_untracked(),
                    gpu = overlay_gpu.get_untracked(),
                    network = overlay_network.get_untracked(),
                    storage = overlay_storage.get_untracked(),
                    power = overlay_power.get_untracked()
                );
                set_timeout(move || {
                    js_call(&script);
                    js_call(&overlay_script);
                }, Duration::from_millis(80));
            } else {
                js_call(&format!(
                    "window.EfInfrastructureScene && window.EfInfrastructureScene.dispose({canvas_id:?})",
                    canvas_id = canvas_id
                ));
            }
        });
    }

    // Reflect the resource selected in the inspector in the 3D viewport.
    {
        Effect::new(move || {
            let id = selected.get();
            js_call(&format!(
                "window.EfInfrastructureScene && window.EfInfrastructureScene.setSelected({canvas_id:?}, {id:?})",
                canvas_id = canvas_id, id = id
            ));
        });
    }

    // Overlay toggles update the existing scene without rebuilding geometry.
    {
        Effect::new(move || {
            let pcie = overlay_pcie.get();
            let gpu = overlay_gpu.get();
            let network = overlay_network.get();
            let storage = overlay_storage.get();
            let power = overlay_power.get();
            js_call(&format!(
                "window.EfInfrastructureScene && window.EfInfrastructureScene.setOverlays({canvas_id:?}, {{pcie:{pcie},gpu:{gpu},network:{network},storage:{storage},power:{power}}})",
                canvas_id = canvas_id, pcie = pcie, gpu = gpu, network = network,
                storage = storage, power = power
            ));
        });
    }

    let nodes = move || scene.get().map(|s| s["nodes"].as_array().cloned().unwrap_or_default()).unwrap_or_default();
    let selected_node = move || {
        let id = selected.get();
        nodes().into_iter().find(|n| n["id"].as_str() == Some(id.as_str()))
    };

    view! {
        <div class="infra-scene-page">
            <div class="infra-scene-header">
                <div>
                    <div class="infra-scene-kicker">"EDGEFABRIC · INFRASTRUCTURE FABRIC"</div>
                    <h1>"Infrastructure Digital Twin"</h1>
                    <p>"Explore simulated GPU clusters, racks, interconnects, storage and power topology."</p>
                </div>
                <span class="infra-scene-sim-badge">"SIMULATION · READ ONLY"</span>
            </div>

            <div class="infra-scene-load card">
                <h2>"Create a GPU cluster simulation"</h2>
                <p class="muted">"Generate a deterministic, tenant-owned virtual cluster and load it directly into the 3D twin."</p>
                <div class="infra-scene-builder-grid">
                    <label>"Simulation identity"
                        <input type="text" prop:value=move || template_identity.get() on:input=move |ev| set_template_identity.set(event_target_value(&ev))/>
                    </label>
                    <label>"Scenario ID"
                        <input type="text" prop:value=move || template_scenario.get() on:input=move |ev| set_template_scenario.set(event_target_value(&ev))/>
                    </label>
                    <label>"Hosts"
                        <input type="number" min="1" max="256" prop:value=move || template_hosts.get() on:input=move |ev| set_template_hosts.set(event_target_value(&ev))/>
                    </label>
                    <label>"GPUs per host"
                        <input type="number" min="1" max="64" prop:value=move || template_gpus.get() on:input=move |ev| set_template_gpus.set(event_target_value(&ev))/>
                    </label>
                    <label>"PCIe generation"
                        <select prop:value=move || template_generation.get() on:change=move |ev| set_template_generation.set(event_target_value(&ev))>
                            <option value="5">"Gen 5"</option>
                            <option value="6">"Gen 6"</option>
                            <option value="7">"Gen 7"</option>
                        </select>
                    </label>
                    <label>"GPU memory (GiB)"
                        <input type="number" min="1" prop:value=move || template_gpu_memory.get() on:input=move |ev| set_template_gpu_memory.set(event_target_value(&ev))/>
                    </label>
                    <label>"GPU peer fabric"
                        <select prop:value=move || template_peer_fabric.get() on:change=move |ev| set_template_peer_fabric.set(event_target_value(&ev))>
                            <option value="nv_switch">"NVSwitch domains"</option>
                            <option value="nv_link_full_mesh">"Direct full mesh"</option>
                            <option value="none">"No peer fabric"</option>
                        </select>
                    </label>
                </div>
                <button class="btn btn-primary" on:click=create_simulation disabled=move || creating.get()>
                    {move || if creating.get() { "Creating simulation…" } else { "Create and open 3D twin" }}
                </button>
                <p class="muted" style="font-size:0.72rem">"Starter profile: PCIe x16 root/uplink/GPU lanes, 2 NUMA nodes, deterministic seed 1. Optional network, storage, CXL and rack power/cooling can be added in the next builder iteration."</p>
            </div>

            <div class="infra-scene-load card">
                <label for="infra-simulation-id">"Simulation ID"</label>
                <div class="infra-scene-load-row">
                    <input
                        id="infra-simulation-id"
                        type="text"
                        placeholder="Paste or select a saved simulation UUID"
                        attr:list="infra-simulation-options"
                        prop:value=move || simulation_id.get()
                        on:input=move |ev| set_simulation_id.set(event_target_value(&ev))
                    />
                    <datalist id="infra-simulation-options">
                        {move || simulations.get().into_iter().map(|simulation| {
                            let id = simulation["id"].as_str().unwrap_or("").to_string();
                            let label = format!("{} · {}", simulation["identity"].as_str().unwrap_or("Simulation"),
                                simulation["scenario_id"].as_str().unwrap_or("scenario"));
                            view! { <option value=id label=label></option> }
                        }).collect_view()}
                    </datalist>
                    <button class="btn btn-primary" on:click=load_scene disabled=move || loading.get()>
                        {move || if loading.get() { "Loading…" } else { "Load scene" }}
                    </button>
                </div>
                <div class="muted" style="font-size:0.72rem">
                    {move || format!("{} tenant-owned simulations available", simulations.get().len())}
                </div>
                <Show when=move || error.get().is_some()>
                    <p class="infra-scene-error">{move || error.get().unwrap_or_default()}</p>
                </Show>
            </div>

            <Show when=move || scene.get().is_some() fallback=|| view! {
                <div class="card infra-scene-empty">
                    <div class="infra-scene-empty-icon">"◇"</div>
                    <h2>"Load an infrastructure simulation"</h2>
                    <p>"Create a simulation with the GPU cluster template endpoint or simulation registry API, then select its ID above."</p>
                    <p class="muted">"The viewer never provisions hardware or modifies physical inventory."</p>
                </div>
            }>
                <div class="infra-scene-toolbar card">
                    <div class="infra-scene-toolbar-title">
                        <strong>{move || scene.get().map(|s| s["identity"].as_str().unwrap_or("Simulation").to_string()).unwrap_or_default()}</strong>
                        <span>{move || scene.get().map(|s| format!("{} nodes · {} edges",
                            s["scene_metadata"]["node_count"].as_u64().unwrap_or(0),
                            s["scene_metadata"]["edge_count"].as_u64().unwrap_or(0))).unwrap_or_default()}</span>
                    </div>
                    <div class="infra-scene-toggles">
                        <label><input type="checkbox" prop:checked=move || overlay_pcie.get() on:change=move |ev| set_overlay_pcie.set(event_target_checked(&ev))/>"PCIe"</label>
                        <label><input type="checkbox" prop:checked=move || overlay_gpu.get() on:change=move |ev| set_overlay_gpu.set(event_target_checked(&ev))/>"GPU fabric"</label>
                        <label><input type="checkbox" prop:checked=move || overlay_network.get() on:change=move |ev| set_overlay_network.set(event_target_checked(&ev))/>"Network"</label>
                        <label><input type="checkbox" prop:checked=move || overlay_storage.get() on:change=move |ev| set_overlay_storage.set(event_target_checked(&ev))/>"Storage / CXL"</label>
                        <label><input type="checkbox" prop:checked=move || overlay_power.get() on:change=move |ev| set_overlay_power.set(event_target_checked(&ev))/>"Power"</label>
                    </div>
                </div>

                <div class="infra-scene-workspace">
                    <div class="infra-scene-viewport card">
                        <div id=canvas_id class="infra-scene-canvas"></div>
                        <div class="infra-scene-viewport-hint">"Drag to orbit · Scroll to zoom · Right-drag to pan"</div>
                        <div class="infra-scene-legend">
                            <span><i class="infra-legend-box"></i>"Compute / rack"</span>
                            <span><i class="infra-legend-gpu"></i>"Accelerator"</span>
                            <span><i class="infra-legend-network"></i>"Network fabric"</span>
                        </div>
                    </div>
                    <aside class="infra-scene-inspector card">
                        <h3>"Resource explorer"</h3>
                        <div class="infra-scene-node-list">
                            {move || nodes().into_iter().map(|node| {
                                let id = node["id"].as_str().unwrap_or("").to_string();
                                let label = node_label(&node);
                                let kind = node["kind"].as_str().unwrap_or("resource").to_string();
                                let active_id = id.clone();
                                view! {
                                    <button class="infra-scene-node"
                                        class:active=move || selected.get() == active_id
                                        on:click=move |_| set_selected.set(id.clone())>
                                        <span class="infra-scene-node-kind">{kind}</span>
                                        <span class="infra-scene-node-name">{label}</span>
                                    </button>
                                }
                            }).collect_view()}
                        </div>
                        <Show when=move || selected_node().is_some()>
                            {move || selected_node().map(|node| {
                                let id = node["id"].as_str().unwrap_or("").to_string();
                                let kind = node["kind"].as_str().unwrap_or("resource").to_string();
                                let render = node["render_hint"].as_str().unwrap_or("unresolved").to_string();
                                let verified = node["provenance"]["verified"].as_bool().unwrap_or(false);
                                let bounds = node["bounds_mm"].clone();
                                let transform = node["transform"].clone();
                                let has_bounds = !bounds.is_null();
                                let has_transform = !transform.is_null();
                                let bounds_json = serde_json::to_string_pretty(&bounds).unwrap_or_default();
                                let transform_json = serde_json::to_string_pretty(&transform).unwrap_or_default();
                                let attributes = serde_json::to_string_pretty(&node["attributes"]).unwrap_or_default();
                                view! {
                                    <div class="infra-scene-details">
                                        <div class="infra-scene-detail-title">"SELECTED RESOURCE"</div>
                                        <strong>{id}</strong>
                                        <span>{kind}</span>
                                        <dl>
                                            <dt>"Rendering"</dt><dd>{render}</dd>
                                            <dt>"Verified"</dt><dd>{if verified { "Yes" } else { "No · simulated" }}</dd>
                                        </dl>
                                        <Show when=move || has_bounds>
                                            <pre>{bounds_json.clone()}</pre>
                                        </Show>
                                        <Show when=move || has_transform>
                                            <pre>{transform_json.clone()}</pre>
                                        </Show>
                                        <details>
                                            <summary>"Attributes"</summary>
                                            <pre>{attributes}</pre>
                                        </details>
                                    </div>
                                }
                            })}
                        </Show>
                    </aside>
                </div>

                <div class="infra-scene-fidelity">
                    <strong>"Fidelity boundaries"</strong>
                    <span>"Geometry assets are referenced, not bundled. Primitive boxes indicate bounds only. Power/cooling are declared design envelopes; no CFD, live telemetry or performance prediction is implied."</span>
                </div>
            </Show>
        </div>
    }
}
