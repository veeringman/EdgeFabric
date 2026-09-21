use gloo_storage::Storage as _;
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;

use crate::api::{
    self, BuildArtifact, BuildArtifactsResponse, BuildDetailResponse, BuildJob,
    BuildLogsResponse, SubmitIosBuildRequest, SubmitIotDeployRequest,
    SubmitQuantumProgramRequest,
};
use crate::components::status_badge::StatusBadge;

const BUILDS_FILTER_STATUS_KEY: &str = "ef_builds_filter_status";
const BUILDS_FILTER_DEVICE_KEY: &str = "ef_builds_filter_device";
const BUILDS_FILTER_SOURCE_KEY: &str = "ef_builds_filter_source";
const BUILDS_PAGE_KEY: &str = "ef_builds_page";
const BUILDS_PAGE_SIZE: i64 = 50;

#[component]
pub fn BuildsPage() -> impl IntoView {
    let (refresh, set_refresh) = signal(0u32);
    let (selected_build, set_selected_build) = signal(Option::<String>::None);
    let (show_submit, set_show_submit) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (page, set_page) = signal(
        gloo_storage::LocalStorage::get::<i64>(BUILDS_PAGE_KEY)
            .unwrap_or(1)
            .max(1),
    );
    let (status_filter, set_status_filter) = signal(
        gloo_storage::LocalStorage::get::<String>(BUILDS_FILTER_STATUS_KEY).unwrap_or_default(),
    );
    let initial_device_filter =
        gloo_storage::LocalStorage::get::<String>(BUILDS_FILTER_DEVICE_KEY).unwrap_or_default();
    let initial_source_filter =
        gloo_storage::LocalStorage::get::<String>(BUILDS_FILTER_SOURCE_KEY).unwrap_or_default();
    let (device_filter_input, set_device_filter_input) = signal(initial_device_filter.clone());
    let (source_filter_input, set_source_filter_input) = signal(initial_source_filter.clone());
    let (device_filter, set_device_filter) = signal(initial_device_filter);
    let (source_filter, set_source_filter) = signal(initial_source_filter);
    let (device_debounce_seq, set_device_debounce_seq) = signal(0u64);
    let (source_debounce_seq, set_source_debounce_seq) = signal(0u64);

    // Persist filters so dashboard refresh/navigation preserves operator context.
    Effect::new(move || {
        let value = status_filter.get();
        let _ = gloo_storage::LocalStorage::set(BUILDS_FILTER_STATUS_KEY, value);
    });
    Effect::new(move || {
        let value = device_filter_input.get();
        let _ = gloo_storage::LocalStorage::set(BUILDS_FILTER_DEVICE_KEY, value);
    });
    Effect::new(move || {
        let value = source_filter_input.get();
        let _ = gloo_storage::LocalStorage::set(BUILDS_FILTER_SOURCE_KEY, value);
    });
    Effect::new(move || {
        let value = page.get();
        let _ = gloo_storage::LocalStorage::set(BUILDS_PAGE_KEY, value);
    });

    // Filter changes should restart pagination from first page.
    Effect::new(move || {
        let _ = status_filter.get();
        let _ = device_filter.get();
        let _ = source_filter.get();
        set_page.set(1);
    });

    // Auto-refresh builds list.
    Effect::new(move || {
        let handle = gloo_timers::callback::Interval::new(4_000, move || {
            set_refresh.update(|n| *n = n.wrapping_add(1));
        });
        std::mem::forget(handle);
    });

    let builds = LocalResource::new(move || {
        let _ = refresh.get();
        let page = page.get();
        let status = status_filter.get();
        let device = device_filter.get();
        let source = source_filter.get();
        async move {
            let offset = (page.saturating_sub(1)) * BUILDS_PAGE_SIZE;
            api::list_builds(
                BUILDS_PAGE_SIZE,
                offset,
                if status.is_empty() { None } else { Some(status.as_str()) },
                if device.is_empty() { None } else { Some(device.as_str()) },
                if source.is_empty() { None } else { Some(source.as_str()) },
            )
                .await
                .unwrap_or_default()
        }
    });

    view! {
        <h1 style="margin-bottom: 1rem">"Builds"</h1>

        <Show when=move || error.get().is_some()>
            <div class="alert error">{move || error.get().unwrap_or_default()}</div>
        </Show>

        <div class="filters">
            <button class="btn btn-primary btn-sm" on:click=move |_| set_show_submit.update(|v| *v = !*v)>
                {move || if show_submit.get() { "Cancel" } else { "+ Submit Build Job" }}
            </button>
            <button class="btn btn-ghost btn-sm" on:click=move |_| set_refresh.update(|n| *n += 1)>"↻ Refresh"</button>
            <div class="form-group">
                <label>"Status"</label>
                <select prop:value=status_filter on:change=move |ev| set_status_filter.set(event_target_value(&ev))>
                    <option value="">"All"</option>
                    <option value="queued">"Queued"</option>
                    <option value="provisioning">"Provisioning"</option>
                    <option value="running">"Running"</option>
                    <option value="succeeded">"Succeeded"</option>
                    <option value="failed">"Failed"</option>
                    <option value="cancelled">"Cancelled"</option>
                </select>
            </div>
            <div class="form-group">
                <label>"Target Device"</label>
                <input
                    type="text"
                    placeholder="device id contains…"
                    prop:value=device_filter_input
                    on:input={
                        move |ev| {
                            let value = event_target_value(&ev);
                            set_device_filter_input.set(value.clone());

                            let mut seq = 0u64;
                            set_device_debounce_seq.update(|n| {
                                *n = n.wrapping_add(1);
                                seq = *n;
                            });
                            let current_seq = device_debounce_seq;

                            leptos::task::spawn_local(async move {
                                TimeoutFuture::new(300).await;
                                if current_seq.get_untracked() == seq {
                                    set_device_filter.set(value);
                                }
                            });
                        }
                    }
                />
            </div>
            <div class="form-group">
                <label>"Source"</label>
                <input
                    type="text"
                    placeholder="source ref contains…"
                    prop:value=source_filter_input
                    on:input={
                        move |ev| {
                            let value = event_target_value(&ev);
                            set_source_filter_input.set(value.clone());

                            let mut seq = 0u64;
                            set_source_debounce_seq.update(|n| {
                                *n = n.wrapping_add(1);
                                seq = *n;
                            });
                            let current_seq = source_debounce_seq;

                            leptos::task::spawn_local(async move {
                                TimeoutFuture::new(300).await;
                                if current_seq.get_untracked() == seq {
                                    set_source_filter.set(value);
                                }
                            });
                        }
                    }
                />
            </div>
        </div>

        <Show
            when=move || {
                !status_filter.get().is_empty()
                    || !device_filter.get().is_empty()
                    || !source_filter.get().is_empty()
            }
        >
            <div class="card" style="margin-bottom: 1rem; padding: 0.75rem 1rem;">
                <div style="display:flex; align-items:center; justify-content:space-between; gap:0.75rem; flex-wrap: wrap;">
                    <div style="display:flex; align-items:center; gap:0.5rem; flex-wrap: wrap;">
                        <span style="font-size:0.8rem; color:var(--text-muted);">"Active Filters:"</span>

                        <Show when=move || !status_filter.get().is_empty()>
                            <button class="btn btn-ghost btn-sm" on:click=move |_| set_status_filter.set(String::new())>
                                {move || format!("Status: {} ✕", status_filter.get())}
                            </button>
                        </Show>

                        <Show when=move || !device_filter.get().is_empty()>
                            <button
                                class="btn btn-ghost btn-sm"
                                on:click={
                                    move |_| {
                                        set_device_debounce_seq.update(|n| *n = n.wrapping_add(1));
                                        set_device_filter_input.set(String::new());
                                        set_device_filter.set(String::new());
                                    }
                                }
                            >
                                {move || format!("Device: {} ✕", device_filter.get())}
                            </button>
                        </Show>

                        <Show when=move || !source_filter.get().is_empty()>
                            <button
                                class="btn btn-ghost btn-sm"
                                on:click={
                                    move |_| {
                                        set_source_debounce_seq.update(|n| *n = n.wrapping_add(1));
                                        set_source_filter_input.set(String::new());
                                        set_source_filter.set(String::new());
                                    }
                                }
                            >
                                {move || format!("Source: {} ✕", source_filter.get())}
                            </button>
                        </Show>
                    </div>

                    <button
                        class="btn btn-sm btn-danger"
                        on:click={
                            move |_| {
                                set_device_debounce_seq.update(|n| *n = n.wrapping_add(1));
                                set_source_debounce_seq.update(|n| *n = n.wrapping_add(1));
                                set_status_filter.set(String::new());
                                set_device_filter_input.set(String::new());
                                set_source_filter_input.set(String::new());
                                set_device_filter.set(String::new());
                                set_source_filter.set(String::new());
                            }
                        }
                    >
                        "Clear All"
                    </button>
                </div>
            </div>
        </Show>

        <Show when=move || show_submit.get()>
            <SubmitBuildForm
                on_created=move || {
                    set_show_submit.set(false);
                    set_refresh.update(|n| *n += 1);
                }
                on_error=move |e| set_error.set(Some(e))
            />
        </Show>

        <div class="card" style="margin-bottom: 1rem;">
            <Transition fallback=move || view! { <div class="loading">"Loading builds…"</div> }>
                {move || builds.get().as_deref().map(|data| {
                    let total_builds = data.total as i64;
                    let counts = data.status_counts.clone();

                    view! {
                        <div style="display: flex; gap: 0.5rem; flex-wrap: wrap; margin-bottom: 0.75rem;">
                            <button
                                class="btn btn-sm"
                                class:btn-primary=move || status_filter.get().is_empty()
                                class:btn-ghost=move || !status_filter.get().is_empty()
                                on:click=move |_| set_status_filter.set(String::new())
                            >{format!("All ({})", data.total)}</button>
                            <button
                                class="btn btn-sm"
                                class:btn-primary=move || status_filter.get() == "queued"
                                class:btn-ghost=move || status_filter.get() != "queued"
                                on:click=move |_| set_status_filter.set("queued".to_string())
                            >{format!("Queued ({})", counts.queued)}</button>
                            <button
                                class="btn btn-sm"
                                class:btn-primary=move || status_filter.get() == "provisioning"
                                class:btn-ghost=move || status_filter.get() != "provisioning"
                                on:click=move |_| set_status_filter.set("provisioning".to_string())
                            >{format!("Provisioning ({})", counts.provisioning)}</button>
                            <button
                                class="btn btn-sm"
                                class:btn-primary=move || status_filter.get() == "running"
                                class:btn-ghost=move || status_filter.get() != "running"
                                on:click=move |_| set_status_filter.set("running".to_string())
                            >{format!("Running ({})", counts.running)}</button>
                            <button
                                class="btn btn-sm"
                                class:btn-primary=move || status_filter.get() == "succeeded"
                                class:btn-ghost=move || status_filter.get() != "succeeded"
                                on:click=move |_| set_status_filter.set("succeeded".to_string())
                            >{format!("Succeeded ({})", counts.succeeded)}</button>
                            <button
                                class="btn btn-sm"
                                class:btn-primary=move || status_filter.get() == "failed"
                                class:btn-ghost=move || status_filter.get() != "failed"
                                on:click=move |_| set_status_filter.set("failed".to_string())
                            >{format!("Failed ({})", counts.failed)}</button>
                            <button
                                class="btn btn-sm"
                                class:btn-primary=move || status_filter.get() == "cancelled"
                                class:btn-ghost=move || status_filter.get() != "cancelled"
                                on:click=move |_| set_status_filter.set("cancelled".to_string())
                            >{format!("Cancelled ({})", counts.cancelled)}</button>
                        </div>

                        {if data.builds.is_empty() {
                            view! { <p class="empty-state">"No builds match current filters"</p> }.into_any()
                        } else {
                            view! {
                                <>
                                    <BuildTable
                                        builds=data.builds.clone()
                                        on_select=move |id| set_selected_build.set(Some(id))
                                    />
                                    <BuildPagination
                                        total=total_builds
                                        page=page
                                        page_size=BUILDS_PAGE_SIZE
                                        on_prev=move || set_page.update(|p| *p = (*p - 1).max(1))
                                        on_next=move || {
                                            let max_page = ((total_builds + BUILDS_PAGE_SIZE - 1) / BUILDS_PAGE_SIZE).max(1);
                                            set_page.update(|p| *p = (*p + 1).min(max_page));
                                        }
                                    />
                                </>
                            }
                            .into_any()
                        }}
                    }
                    .into_any()
                })}
            </Transition>
        </div>

        <Show when=move || selected_build.get().is_some()>
            {move || selected_build.get().map(|id| view! {
                <BuildDetailPanel
                    build_id=id
                    on_close=move || set_selected_build.set(None)
                    on_refresh=move || set_refresh.update(|n| *n += 1)
                    on_error=move |e| set_error.set(Some(e))
                />
            })}
        </Show>
    }
}

#[component]
fn BuildPagination(
    total: i64,
    page: ReadSignal<i64>,
    page_size: i64,
    on_prev: impl Fn() + Send + Sync + 'static + Clone,
    on_next: impl Fn() + Send + Sync + 'static + Clone,
) -> impl IntoView {
    let total_pages = move || ((total + page_size - 1) / page_size).max(1);
    let from = move || ((page.get() - 1) * page_size + 1).min(total.max(1));
    let to = move || (page.get() * page_size).min(total);
    let can_prev = move || page.get() > 1;
    let can_next = move || page.get() < total_pages();

    view! {
        <div style="display:flex; align-items:center; justify-content:space-between; gap:0.75rem; margin-top:0.9rem; flex-wrap:wrap;">
            <span style="font-size:0.82rem; color:var(--text-muted);">
                {move || if total == 0 {
                    "Showing 0 results".to_string()
                } else {
                    format!("Showing {}-{} of {}", from(), to(), total)
                }}
            </span>
            <div style="display:flex; gap:0.5rem; align-items:center;">
                <button class="btn btn-ghost btn-sm" disabled=move || !can_prev() on:click=move |_| on_prev()>"← Prev"</button>
                <span style="font-size:0.82rem; color:var(--text-secondary);">{move || format!("Page {} / {}", page.get(), total_pages())}</span>
                <button class="btn btn-ghost btn-sm" disabled=move || !can_next() on:click=move |_| on_next()>"Next →"</button>
            </div>
        </div>
    }
}

#[component]
fn BuildTable(
    builds: Vec<BuildJob>,
    on_select: impl Fn(String) + Send + Sync + 'static + Clone,
) -> impl IntoView {
    view! {
        <div class="table-wrap">
            <table>
                <thead>
                    <tr>
                        <th>"ID"</th>
                        <th>"Type"</th>
                        <th>"Status"</th>
                        <th>"Source"</th>
                        <th>"Target Device"</th>
                        <th>"Runner"</th>
                        <th>"Queued"</th>
                    </tr>
                </thead>
                <tbody>
                    {builds.into_iter().map(|b| {
                        let id = b.id.clone();
                        let short = id.chars().take(8).collect::<String>();
                        let status = b.status.clone();
                        let on_select = on_select.clone();
                        view! {
                            <tr>
                                <td>
                                    <a href="#" class="mono" style="color: var(--accent); cursor: pointer"
                                        on:click={
                                            let id = id.clone();
                                            move |ev: leptos::ev::MouseEvent| {
                                                ev.prevent_default();
                                                on_select(id.clone());
                                            }
                                        }>{short}</a>
                                </td>
                                    <td class="mono">{b.build_type}</td>
                                <td><StatusBadge status=status/></td>
                                <td class="mono truncate">{b.source_ref}</td>
                                <td class="mono truncate">{b.device_id.unwrap_or_else(|| "—".to_string())}</td>
                                <td class="mono truncate">{b.runner_id.unwrap_or_else(|| "—".to_string())}</td>
                                <td>{b.queued_at.unwrap_or_else(|| "—".to_string())}</td>
                            </tr>
                        }
                    }).collect::<Vec<_>>()}
                </tbody>
            </table>
        </div>
    }
}

#[component]
fn BuildDetailPanel(
    build_id: String,
    on_close: impl Fn() + Send + Sync + 'static + Clone,
    on_refresh: impl Fn() + Send + Sync + 'static + Clone,
    on_error: impl Fn(String) + Send + Sync + 'static + Clone,
) -> impl IntoView {
    let (tick, set_tick) = signal(0u32);

    // Auto-refresh logs/details while panel is open.
    Effect::new(move || {
        let handle = gloo_timers::callback::Interval::new(3_000, move || {
            set_tick.update(|n| *n = n.wrapping_add(1));
        });
        std::mem::forget(handle);
    });

    let detail_id = build_id.clone();
    let detail = LocalResource::new(move || {
        let _ = tick.get();
        let id = detail_id.clone();
        async move { api::get_build(&id).await.ok() }
    });

    let logs_id = build_id.clone();
    let logs = LocalResource::new(move || {
        let _ = tick.get();
        let id = logs_id.clone();
        async move { api::get_build_logs(&id).await.ok() }
    });

    let artifacts_id = build_id.clone();
    let artifacts = LocalResource::new(move || {
        let _ = tick.get();
        let id = artifacts_id.clone();
        async move { api::get_build_artifacts(&id).await.ok() }
    });

    let do_cancel = {
        let id = build_id.clone();
        let on_error = on_error.clone();
        let on_refresh = on_refresh.clone();
        move |_| {
            let id = id.clone();
            let on_error = on_error.clone();
            let on_refresh = on_refresh.clone();
            leptos::task::spawn_local(async move {
                match api::cancel_build(&id).await {
                    Ok(_) => {
                        on_refresh();
                    }
                    Err(e) => on_error(e),
                }
            });
        }
    };

    view! {
        <div class="card">
            <div class="card-header">
                <h2>{format!("Build {}", build_id.chars().take(8).collect::<String>())}</h2>
                <div style="display:flex; gap: 0.5rem;">
                    <button class="btn btn-danger btn-sm" on:click=do_cancel>"Cancel Build"</button>
                    <button class="btn btn-ghost btn-sm" on:click=move |_| on_close()>"✕ Close"</button>
                </div>
            </div>

            <Transition fallback=move || view! { <div class="loading">"Loading build details…"</div> }>
                {move || detail.get().as_deref().map(|d| {
                    match d {
                        None => view! { <p class="empty-state">"Failed to load build details"</p> }.into_any(),
                        Some(BuildDetailResponse { build }) => {
                            let status = build.status.clone();
                            view! {
                                <div style="display:grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 0.75rem; margin-bottom: 1rem;">
                                    <div><strong>"Build Type: "</strong><span class="mono">{build.build_type.clone()}</span></div>
                                    <div><strong>"Status: "</strong><StatusBadge status=status/></div>
                                    <div><strong>"Source: "</strong><span class="mono">{build.source_ref.clone()}</span></div>
                                    <div><strong>"Target Device: "</strong><span class="mono">{build.device_id.clone().unwrap_or_else(|| "—".to_string())}</span></div>
                                    <div><strong>"Runner: "</strong><span class="mono">{build.runner_id.clone().unwrap_or_else(|| "—".to_string())}</span></div>
                                    <div><strong>"Queued: "</strong>{build.queued_at.clone().unwrap_or_else(|| "—".to_string())}</div>
                                    <div><strong>"Completed: "</strong>{build.completed_at.clone().unwrap_or_else(|| "—".to_string())}</div>
                                </div>
                            }
                            .into_any()
                        }
                    }
                })}
            </Transition>

            <div class="card" style="margin-bottom: 1rem;">
                <h3 style="margin-bottom: 0.5rem;">"Logs"</h3>
                <Transition fallback=move || view! { <div class="loading">"Loading logs…"</div> }>
                    {move || logs.get().as_deref().map(|d| {
                        match d {
                            None => view! { <p class="empty-state">"No logs available yet"</p> }.into_any(),
                            Some(BuildLogsResponse { mode, stdout, stderr, .. }) => {
                                let mode_text = mode.clone().unwrap_or_else(|| "unknown".to_string());
                                let out_text = stdout.clone();
                                let err_text = stderr.clone();
                                let has_stderr = !err_text.is_empty();
                                view! {
                                <p style="margin-bottom: 0.5rem; color: var(--text-muted); font-size: 0.8rem;">
                                    {format!("mode: {}", mode_text)}
                                </p>
                                <pre class="log-viewer" style="max-height: 260px; overflow:auto;">{out_text}</pre>
                                {if has_stderr {
                                    view! {
                                        <>
                                            <h4 style="margin: 0.75rem 0 0.4rem;">"stderr"</h4>
                                            <pre class="log-viewer" style="max-height: 160px; overflow:auto;">{err_text}</pre>
                                        </>
                                    }
                                    .into_any()
                                } else {
                                    view! { <></> }.into_any()
                                }}
                            }
                            .into_any()
                            }
                        }
                    })}
                </Transition>
            </div>

            <div class="card">
                <h3 style="margin-bottom: 0.5rem;">"Artifacts"</h3>
                <Transition fallback=move || view! { <div class="loading">"Loading artifacts…"</div> }>
                    {move || artifacts.get().as_deref().map(|d| {
                        match d {
                            None => view! { <p class="empty-state">"No artifacts yet"</p> }.into_any(),
                            Some(BuildArtifactsResponse { artifacts, .. }) if artifacts.is_empty() => {
                                view! { <p class="empty-state">"No artifacts yet"</p> }.into_any()
                            }
                            Some(BuildArtifactsResponse { artifacts, .. }) => view! {
                                <div class="table-wrap">
                                    <table>
                                        <thead>
                                            <tr>
                                                <th>"Type"</th>
                                                <th>"Name"</th>
                                                <th>"Path"</th>
                                                <th>"Size"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {artifacts.iter().map(|a: &BuildArtifact| view! {
                                                <tr>
                                                    <td>{a.artifact_type.clone()}</td>
                                                    <td class="mono truncate">{a.file_name.clone()}</td>
                                                    <td class="mono truncate">{a.storage_path.clone()}</td>
                                                    <td>{a.size_bytes}</td>
                                                </tr>
                                            }).collect::<Vec<_>>()}
                                        </tbody>
                                    </table>
                                </div>
                            }.into_any(),
                        }
                    })}
                </Transition>
            </div>
        </div>
    }
}

#[component]
fn SubmitBuildForm(
    on_created: impl Fn() + Send + Sync + 'static + Clone,
    on_error: impl Fn(String) + Send + Sync + 'static + Clone,
) -> impl IntoView {
    let (build_kind, set_build_kind) = signal("ios".to_string());
    let (source_ref, set_source_ref) = signal("project-main".to_string());
    let (project_path, set_project_path) = signal(String::new());
    let (scheme, set_scheme) = signal(String::new());
    let (configuration, set_configuration) = signal("Debug".to_string());
    let (target_device_id, set_target_device_id) = signal(String::new());
    let (board_profile, set_board_profile) = signal("esp32c6".to_string());
    let (deploy_command, set_deploy_command) = signal(String::new());
    let (run_command, set_run_command) = signal(String::new());
    let (backend_kind, set_backend_kind) = signal("virtual".to_string());
    let (backend_name, set_backend_name) = signal(String::new());
    let (veeros_host_hint, set_veeros_host_hint) = signal(String::new());
    let (region, set_region) = signal(String::new());
    let (ssh_user, set_ssh_user) = signal("builder".to_string());
    let (loading, set_loading) = signal(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let on_created = on_created.clone();
        let on_error = on_error.clone();
        set_loading.set(true);

        let opt = |value: String| {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() { None } else { Some(trimmed) }
        };

        let build_kind_value = build_kind.get_untracked();
        let source_ref_value = source_ref.get_untracked();
        let project_path_value = project_path.get_untracked();
        let region_value = opt(region.get_untracked());
        let ssh_user_value = opt(ssh_user.get_untracked());

        leptos::task::spawn_local(async move {
            let result = match build_kind_value.as_str() {
                "iot" => {
                    let req = SubmitIotDeployRequest {
                        source_ref: source_ref_value,
                        project_path: project_path_value,
                        target_device_id: target_device_id.get_untracked(),
                        board_profile: opt(board_profile.get_untracked()),
                        deploy_command: opt(deploy_command.get_untracked()),
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
                }
                "quantum" => {
                    let backend_kind_value = backend_kind.get_untracked();
                    let mut required_runner_capabilities = vec![
                        "cicd_runner".into(),
                        "linux_runner".into(),
                        "quantum_sdk".into(),
                        "artifact_upload".into(),
                    ];
                    if backend_kind_value == "virtual" {
                        required_runner_capabilities.push("quantum_simulator".into());
                    } else {
                        required_runner_capabilities.push("veeros_host".into());
                    }

                    let req = SubmitQuantumProgramRequest {
                        source_ref: source_ref_value,
                        project_path: project_path_value,
                        run_command: opt(run_command.get_untracked()),
                        backend_kind: Some(backend_kind_value),
                        backend_name: opt(backend_name.get_untracked()),
                        veeros_host_hint: opt(veeros_host_hint.get_untracked()),
                        region: region_value,
                        required_runner_capabilities,
                        required_runner_labels: vec!["quantum".into()],
                        lease_ttl_seconds: Some(1800),
                        ssh_user: ssh_user_value,
                    };
                    api::submit_quantum_program(&req).await
                }
                _ => {
                    let req = SubmitIosBuildRequest {
                        source_ref: source_ref_value,
                        project_path: project_path_value,
                        scheme: scheme.get_untracked(),
                        configuration: configuration.get_untracked(),
                        target_device_id: target_device_id.get_untracked(),
                        region: region_value,
                        required_runner_capabilities: vec![
                            "cicd_runner".into(),
                            "xcode".into(),
                            "artifact_upload".into(),
                        ],
                        required_runner_labels: vec![],
                        lease_ttl_seconds: Some(1800),
                        ssh_user: ssh_user_value,
                    };
                    api::submit_ios_build(&req).await
                }
            };

            match result {
                Ok(_) => on_created(),
                Err(e) => on_error(e),
            }
            set_loading.set(false);
        });
    };

    view! {
        <div class="card" style="margin-bottom: 1rem;">
            <form on:submit=submit style="display:grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 0.75rem;">
                <div class="form-group">
                    <label>"Build Type"</label>
                    <select prop:value=build_kind on:change=move |ev| set_build_kind.set(event_target_value(&ev))>
                        <option value="ios">"iOS Build"</option>
                        <option value="iot">"IoT Deploy (ESP32C6/RPi)"</option>
                        <option value="quantum">"Quantum Program Run"</option>
                    </select>
                </div>
                <div class="form-group">
                    <label>"Source Ref"</label>
                    <input type="text" prop:value=source_ref on:input=move |ev| set_source_ref.set(event_target_value(&ev))/>
                </div>
                <div class="form-group">
                    <label>"Project Path On Runner"</label>
                    <input type="text" prop:value=project_path on:input=move |ev| set_project_path.set(event_target_value(&ev))/>
                </div>

                <Show when=move || build_kind.get() == "ios">
                    <>
                        <div class="form-group">
                            <label>"Scheme"</label>
                            <input type="text" prop:value=scheme on:input=move |ev| set_scheme.set(event_target_value(&ev))/>
                        </div>
                        <div class="form-group">
                            <label>"Configuration"</label>
                            <input type="text" prop:value=configuration on:input=move |ev| set_configuration.set(event_target_value(&ev))/>
                        </div>
                        <div class="form-group">
                            <label>"Target Device ID (Dashboard iOS device)"</label>
                            <input type="text" prop:value=target_device_id on:input=move |ev| set_target_device_id.set(event_target_value(&ev))/>
                        </div>
                    </>
                </Show>

                <Show when=move || build_kind.get() == "iot">
                    <>
                        <div class="form-group">
                            <label>"Target Device ID (IoT/Virtual IoT device)"</label>
                            <input type="text" prop:value=target_device_id on:input=move |ev| set_target_device_id.set(event_target_value(&ev))/>
                        </div>
                        <div class="form-group">
                            <label>"Board Profile (optional)"</label>
                            <input type="text" prop:value=board_profile on:input=move |ev| set_board_profile.set(event_target_value(&ev))/>
                        </div>
                        <div class="form-group">
                            <label>"Deploy Command (optional)"</label>
                            <input type="text" prop:value=deploy_command on:input=move |ev| set_deploy_command.set(event_target_value(&ev))/>
                        </div>
                    </>
                </Show>

                <Show when=move || build_kind.get() == "quantum">
                    <>
                        <div class="form-group">
                            <label>"Run Command (optional)"</label>
                            <input type="text" prop:value=run_command on:input=move |ev| set_run_command.set(event_target_value(&ev))/>
                        </div>
                        <div class="form-group">
                            <label>"Backend Kind"</label>
                            <select prop:value=backend_kind on:change=move |ev| set_backend_kind.set(event_target_value(&ev))>
                                <option value="virtual">"virtual"</option>
                                <option value="real">"real"</option>
                            </select>
                        </div>
                        <div class="form-group">
                            <label>"Backend Name (optional)"</label>
                            <input type="text" prop:value=backend_name on:input=move |ev| set_backend_name.set(event_target_value(&ev))/>
                        </div>
                        <div class="form-group">
                            <label>"VeerOS Host Hint (optional)"</label>
                            <input type="text" prop:value=veeros_host_hint on:input=move |ev| set_veeros_host_hint.set(event_target_value(&ev))/>
                        </div>
                    </>
                </Show>

                <div class="form-group">
                    <label>"Region (optional)"</label>
                    <input type="text" prop:value=region on:input=move |ev| set_region.set(event_target_value(&ev))/>
                </div>
                <div class="form-group">
                    <label>"SSH User"</label>
                    <input type="text" prop:value=ssh_user on:input=move |ev| set_ssh_user.set(event_target_value(&ev))/>
                </div>
                <div class="form-group" style="align-self:end;">
                    <button type="submit" class="btn btn-primary btn-sm" disabled=move || loading.get()>
                        {move || {
                            if loading.get() {
                                "Submitting…"
                            } else {
                                match build_kind.get().as_str() {
                                    "iot" => "Submit IoT Deploy",
                                    "quantum" => "Submit Quantum Run",
                                    _ => "Submit iOS Build",
                                }
                            }
                        }}
                    </button>
                </div>
            </form>
        </div>
    }
}
