use leptos::prelude::*;

use crate::api::{
    self, CreateMlClusterRequest, CreateMlJobRequest, MlChatMessage, MlChatRequest, MlClusterEnvelope,
    MlJob, MlNode,
};
use crate::components::status_badge::StatusBadge;

#[component]
pub fn MlFabricPage() -> impl IntoView {
    let (refresh, set_refresh) = signal(0u32);
    let (error, set_error) = signal(Option::<String>::None);
    let (notice, set_notice) = signal(Option::<String>::None);

    Effect::new(move || {
        let handle = gloo_timers::callback::Interval::new(5_000, move || {
            set_refresh.update(|n| *n = n.wrapping_add(1));
        });
        std::mem::forget(handle);
    });

    let nodes = LocalResource::new(move || {
        let _ = refresh.get();
        async move { api::list_ml_nodes().await.unwrap_or_default() }
    });
    let clusters = LocalResource::new(move || {
        let _ = refresh.get();
        async move { api::list_ml_clusters().await.unwrap_or_default() }
    });
    let jobs = LocalResource::new(move || {
        let _ = refresh.get();
        async move { api::list_ml_jobs().await.unwrap_or_default() }
    });

    view! {
        <h1 style="margin-bottom: 0.35rem">"ML Fabric"</h1>
        <p class="muted" style="margin-bottom: 1rem">
            "M6 Mini pool — replica inference, LAN pipeline-parallel shards, LoRA / DiLoCo / PP train. Activation traffic stays on 2.5 GbE."
        </p>

        <Show when=move || error.get().is_some()>
            <div class="alert error">{move || error.get().unwrap_or_default()}</div>
        </Show>
        <Show when=move || notice.get().is_some()>
            <div class="alert">{move || notice.get().unwrap_or_default()}</div>
        </Show>

        <div class="filters">
            <button class="btn btn-ghost btn-sm" on:click=move |_| set_refresh.update(|n| *n += 1)>"↻ Refresh"</button>
        </div>

        <CreateClusterForm
            on_created=move || { set_notice.set(Some("Cluster leased".into())); set_refresh.update(|n| *n += 1); }
            on_error=move |e| set_error.set(Some(e))
        />
        <CreateJobForm
            on_created=move || { set_notice.set(Some("Job submitted".into())); set_refresh.update(|n| *n += 1); }
            on_error=move |e| set_error.set(Some(e))
        />
        <ChatForm
            on_error=move |e| set_error.set(Some(e))
            on_ok=move |t| set_notice.set(Some(t))
        />

        <div class="card" style="margin-top: 1rem">
            <h2>"Nodes"</h2>
            <Transition fallback=move || view! { <div class="loading">"Loading nodes…"</div> }>
                {move || nodes.get().as_deref().map(|data| {
                    if data.nodes.is_empty() {
                        return view! { <p class="empty-state">"No ML-capable SSH hosts yet. Register a macOS, Linux, or Windows host under Devices & Hosts, probe it, then start the worker."</p> }.into_any();
                    }
                    view! { <NodeTable nodes=data.nodes.clone()/> }.into_any()
                })}
            </Transition>
        </div>

        <div class="card" style="margin-top: 1rem">
            <h2>"Clusters"</h2>
            <Transition fallback=move || view! { <div class="loading">"Loading clusters…"</div> }>
                {move || clusters.get().as_deref().map(|data| {
                    if data.clusters.is_empty() {
                        return view! { <p class="empty-state">"No clusters yet"</p> }.into_any();
                    }
                    view! { <ClusterTable clusters=data.clusters.clone()/> }.into_any()
                })}
            </Transition>
        </div>

        <div class="card" style="margin-top: 1rem">
            <h2>"Jobs"</h2>
            <Transition fallback=move || view! { <div class="loading">"Loading jobs…"</div> }>
                {move || jobs.get().as_deref().map(|data| {
                    if data.jobs.is_empty() {
                        return view! { <p class="empty-state">"No ML jobs"</p> }.into_any();
                    }
                    let items = data.jobs.clone();
                    view! { <JobTable jobs=items on_action=move || set_refresh.update(|n| *n += 1)/> }.into_any()
                })}
            </Transition>
        </div>
    }
}

#[component]
fn NodeTable(nodes: Vec<MlNode>) -> impl IntoView {
    view! {
        <table class="data-table">
            <thead>
                <tr>
                    <th>"Name"</th><th>"Chip"</th><th>"RAM"</th><th>"Link"</th><th>"MLX"</th><th>"Host"</th><th>"Status"</th>
                </tr>
            </thead>
            <tbody>
                {nodes.into_iter().map(|n| view! {
                    <tr>
                        <td>{n.labels.first().cloned().unwrap_or_else(|| n.runner_id.clone())}</td>
                        <td>{format!("{}  CPU {}  GPU {}  ANE {}", n.chip, n.cpu_cores.unwrap_or(0), n.gpu_cores.unwrap_or(0), n.ane_cores.unwrap_or(0))}</td>
                        <td>{format!("{} GB", n.unified_memory_gb.unwrap_or(0))}</td>
                        <td>{format!("{} Mbps", n.link_mbps)}</td>
                        <td>{n.mlx_version}</td>
                        <td>{n.worker_host}</td>
                        <td><StatusBadge status=n.status/></td>
                    </tr>
                }).collect_view()}
            </tbody>
        </table>
    }
}

#[component]
fn ClusterTable(clusters: Vec<MlClusterEnvelope>) -> impl IntoView {
    view! {
        <table class="data-table">
            <thead>
                <tr><th>"Name"</th><th>"Role"</th><th>"Ranks"</th><th>"Rank-0"</th><th>"Status"</th></tr>
            </thead>
            <tbody>
                {clusters.into_iter().map(|env| {
                    let ranks = env.members.len();
                    let c = env.cluster;
                    view! {
                        <tr>
                            <td>{c.name}</td>
                            <td>{c.role}</td>
                            <td>{ranks}</td>
                            <td>{c.rank0_url}</td>
                            <td><StatusBadge status=c.status/></td>
                        </tr>
                    }
                }).collect_view()}
            </tbody>
        </table>
    }
}

#[component]
fn JobTable(jobs: Vec<MlJob>, on_action: impl Fn() + Send + Sync + 'static + Clone) -> impl IntoView {
    view! {
        <table class="data-table">
            <thead>
                <tr><th>"Kind"</th><th>"Model"</th><th>"Status"</th><th>"Metrics"</th><th></th></tr>
            </thead>
            <tbody>
                {jobs.into_iter().map(|j| {
                    let id = j.id.clone();
                    let on_action = on_action.clone();
                    let metrics = j.metrics.to_string();
                    view! {
                        <tr>
                            <td>{j.kind}</td>
                            <td>{j.model_id}</td>
                            <td><StatusBadge status=j.status/></td>
                            <td class="muted">{metrics}</td>
                            <td>
                                <button class="btn btn-ghost btn-sm" on:click=move |_| {
                                    let id = id.clone();
                                    let on_action = on_action.clone();
                                    leptos::task::spawn_local(async move {
                                        let _ = api::cancel_ml_job(&id).await;
                                        on_action();
                                    });
                                }>"Cancel"</button>
                            </td>
                        </tr>
                    }
                }).collect_view()}
            </tbody>
        </table>
    }
}

#[component]
fn CreateClusterForm(
    on_created: impl Fn() + Send + Sync + 'static + Clone,
    on_error: impl Fn(String) + Send + Sync + 'static + Clone,
) -> impl IntoView {
    let (name, set_name) = signal("lab-minis".to_string());
    let (role, set_role) = signal("pipeline".to_string());
    let (size, set_size) = signal("1".to_string());
    let (cidr, set_cidr) = signal("192.168.29.0/24".to_string());

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let on_created = on_created.clone();
        let on_error = on_error.clone();
        let req = CreateMlClusterRequest {
            name: name.get_untracked(),
            role: role.get_untracked(),
            size: size.get_untracked().parse().unwrap_or(1),
            interconnect_cidrs: vec![cidr.get_untracked()],
            required_capabilities: vec!["mlx".into(), "llm_infer".into(), "pipeline_pp".into()],
        };
        leptos::task::spawn_local(async move {
            match api::create_ml_cluster(&req).await {
                Ok(_) => on_created(),
                Err(e) => on_error(e),
            }
        });
    };

    view! {
        <div class="card" style="margin-bottom: 1rem">
            <h3>"Lease cluster"</h3>
            <form on:submit=submit style="display:flex;gap:0.75rem;flex-wrap:wrap;align-items:flex-end">
                <div class="form-group"><label>"Name"</label>
                    <input prop:value=name on:input=move |ev| set_name.set(event_target_value(&ev))/>
                </div>
                <div class="form-group"><label>"Role"</label>
                    <select on:change=move |ev| set_role.set(event_target_value(&ev))>
                        <option value="pipeline" selected>"pipeline"</option>
                        <option value="replica">"replica"</option>
                        <option value="diloco">"diloco"</option>
                    </select>
                </div>
                <div class="form-group"><label>"Size"</label>
                    <input type="number" style="width:70px" prop:value=size on:input=move |ev| set_size.set(event_target_value(&ev))/>
                </div>
                <div class="form-group"><label>"LAN CIDR"</label>
                    <input prop:value=cidr on:input=move |ev| set_cidr.set(event_target_value(&ev))/>
                </div>
                <button class="btn btn-primary btn-sm" type="submit">"Create cluster"</button>
            </form>
        </div>
    }
}

#[component]
fn CreateJobForm(
    on_created: impl Fn() + Send + Sync + 'static + Clone,
    on_error: impl Fn(String) + Send + Sync + 'static + Clone,
) -> impl IntoView {
    let (cluster_id, set_cluster_id) = signal(String::new());
    let (kind, set_kind) = signal("infer_serve".to_string());

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let on_created = on_created.clone();
        let on_error = on_error.clone();
        let req = CreateMlJobRequest {
            cluster_id: cluster_id.get_untracked(),
            kind: kind.get_untracked(),
            model_id: "toy-pp".into(),
            quantization: "q4".into(),
            context_length: 2048,
            n_layers: 12,
            hidden_size: 64,
        };
        leptos::task::spawn_local(async move {
            match api::create_ml_job(&req).await {
                Ok(_) => on_created(),
                Err(e) => on_error(e),
            }
        });
    };

    view! {
        <div class="card" style="margin-bottom: 1rem">
            <h3>"Submit job"</h3>
            <form on:submit=submit style="display:flex;gap:0.75rem;flex-wrap:wrap;align-items:flex-end">
                <div class="form-group"><label>"Cluster ID"</label>
                    <input placeholder="uuid" prop:value=cluster_id on:input=move |ev| set_cluster_id.set(event_target_value(&ev))/>
                </div>
                <div class="form-group"><label>"Kind"</label>
                    <select on:change=move |ev| set_kind.set(event_target_value(&ev))>
                        <option value="infer_serve" selected>"infer_serve"</option>
                        <option value="train_lora">"train_lora"</option>
                        <option value="train_diloco">"train_diloco"</option>
                        <option value="train_pipeline">"train_pipeline"</option>
                    </select>
                </div>
                <button class="btn btn-primary btn-sm" type="submit">"Run"</button>
            </form>
        </div>
    }
}

#[component]
fn ChatForm(
    on_error: impl Fn(String) + Send + Sync + 'static + Clone,
    on_ok: impl Fn(String) + Send + Sync + 'static + Clone,
) -> impl IntoView {
    let (cluster_id, set_cluster_id) = signal(String::new());
    let (prompt, set_prompt) = signal("hello from edgefabric".to_string());

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let on_error = on_error.clone();
        let on_ok = on_ok.clone();
        let req = MlChatRequest {
            model: "toy-pp".into(),
            messages: vec![MlChatMessage { role: "user".into(), content: prompt.get_untracked() }],
            max_tokens: 24,
            cluster_id: cluster_id.get_untracked(),
        };
        leptos::task::spawn_local(async move {
            match api::ml_chat(&req).await {
                Ok(v) => {
                    let text = v.pointer("/choices/0/message/content")
                        .and_then(|x| x.as_str())
                        .unwrap_or(&v.to_string())
                        .to_string();
                    on_ok(text);
                }
                Err(e) => on_error(e),
            }
        });
    };

    view! {
        <div class="card" style="margin-bottom: 1rem">
            <h3>"Chat (rank-0 gateway)"</h3>
            <form on:submit=submit style="display:flex;gap:0.75rem;flex-wrap:wrap;align-items:flex-end">
                <div class="form-group"><label>"Cluster ID"</label>
                    <input placeholder="uuid" prop:value=cluster_id on:input=move |ev| set_cluster_id.set(event_target_value(&ev))/>
                </div>
                <div class="form-group" style="flex:1"><label>"Prompt"</label>
                    <input prop:value=prompt on:input=move |ev| set_prompt.set(event_target_value(&ev))/>
                </div>
                <button class="btn btn-primary btn-sm" type="submit">"Send"</button>
            </form>
        </div>
    }
}
