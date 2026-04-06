use leptos::prelude::*;
use serde::Serialize;

use crate::api::{self, Job, JobDetail, JobListResponse, JobProgress};
use crate::components::status_badge::StatusBadge;

/// Test job dashboard — submit, monitor, view results.
#[component]
pub fn JobsPage() -> impl IntoView {
    let (refresh, set_refresh) = signal(0u32);
    let (show_submit, set_show_submit) = signal(false);
    let (selected_job, set_selected_job) = signal(Option::<String>::None);
    let (error, set_error) = signal(Option::<String>::None);

    let jobs = LocalResource::new(move || {
        let _ = refresh.get();
        async move {
            api::get::<JobListResponse>("/jobs").await.unwrap_or_default()
        }
    });

    let do_refresh = move |_| set_refresh.update(|n| *n += 1);

    view! {
        <h1 style="margin-bottom: 1rem">"Test Jobs"</h1>

        <Show when=move || error.get().is_some()>
            <div class="alert error">{move || error.get().unwrap_or_default()}</div>
        </Show>

        <div class="filters">
            <button class="btn btn-primary btn-sm" on:click=move |_| set_show_submit.update(|v| *v = !*v)>
                {move || if show_submit.get() { "Cancel" } else { "+ Submit Job" }}
            </button>
            <button class="btn btn-ghost btn-sm" on:click=do_refresh>"↻ Refresh"</button>
        </div>

        <Show when=move || show_submit.get()>
            <SubmitJobForm on_created=move || {
                set_show_submit.set(false);
                set_refresh.update(|n| *n += 1);
            } on_error=move |e| set_error.set(Some(e))/>
        </Show>

        // Job detail view
        <Show when=move || selected_job.get().is_some()>
            {move || selected_job.get().map(|id| view! {
                <JobDetailView job_id=id on_close=move || set_selected_job.set(None)/>
            })}
        </Show>

        <div class="card">
            <Suspense fallback=move || view! { <div class="loading">"Loading jobs…"</div> }>
                {move || jobs.get().as_deref().map(|data| {
                    if data.jobs.is_empty() {
                        return view! { <p class="empty-state">"No test jobs submitted yet"</p> }.into_any();
                    }
                    let items = data.jobs.clone();
                    view! { <JobTable jobs=items on_select=move |id| set_selected_job.set(Some(id)) on_action=move || set_refresh.update(|n| *n += 1)/> }.into_any()
                })}
            </Suspense>
        </div>
    }
}

#[derive(Serialize)]
struct SubmitJobReq {
    test_suite: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    test_names: Vec<String>,
    parallelism: u32,
    max_retries: u32,
}

#[component]
fn SubmitJobForm(
    on_created: impl Fn() + Send + Sync + 'static + Clone,
    on_error: impl Fn(String) + Send + Sync + 'static + Clone,
) -> impl IntoView {
    let (suite, set_suite) = signal(String::new());
    let (parallelism, set_parallelism) = signal("4".to_string());
    let (retries, set_retries) = signal("2".to_string());
    let (loading, set_loading) = signal(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let on_created = on_created.clone();
        let on_error = on_error.clone();
        set_loading.set(true);

        let req = SubmitJobReq {
            test_suite: suite.get_untracked(),
            test_names: vec![],
            parallelism: parallelism.get_untracked().parse().unwrap_or(4),
            max_retries: retries.get_untracked().parse().unwrap_or(2),
        };

        leptos::task::spawn_local(async move {
            match api::post::<_, serde_json::Value>("/jobs", &req).await {
                Ok(_) => on_created(),
                Err(e) => on_error(e),
            }
            set_loading.set(false);
        });
    };

    view! {
        <div class="card" style="margin-bottom: 1rem">
            <form on:submit=submit style="display: flex; gap: 0.75rem; align-items: flex-end; flex-wrap: wrap">
                <div class="form-group">
                    <label>"Test Suite"</label>
                    <input type="text" placeholder="com.example.tests" prop:value=suite
                        on:input=move |ev| set_suite.set(event_target_value(&ev))/>
                </div>
                <div class="form-group">
                    <label>"Parallelism"</label>
                    <input type="number" prop:value=parallelism style="width:70px"
                        on:input=move |ev| set_parallelism.set(event_target_value(&ev))/>
                </div>
                <div class="form-group">
                    <label>"Max Retries"</label>
                    <input type="number" prop:value=retries style="width:70px"
                        on:input=move |ev| set_retries.set(event_target_value(&ev))/>
                </div>
                <button type="submit" class="btn btn-primary btn-sm" disabled=move || loading.get()>
                    {move || if loading.get() { "Submitting…" } else { "Submit" }}
                </button>
            </form>
        </div>
    }
}

fn truncate_id(s: &str) -> String {
    s.chars().take(8).collect()
}

#[component]
fn JobTable(
    jobs: Vec<Job>,
    on_select: impl Fn(String) + Send + Sync + 'static + Clone,
    on_action: impl Fn() + Send + Sync + 'static + Clone,
) -> impl IntoView {
    view! {
        <div class="table-wrap">
            <table>
                <thead>
                    <tr>
                        <th>"ID"</th>
                        <th>"Test Suite"</th>
                        <th>"Status"</th>
                        <th>"Priority"</th>
                        <th>"Created"</th>
                        <th>"Actions"</th>
                    </tr>
                </thead>
                <tbody>
                    {jobs.into_iter().map(|j| {
                        let status = j.status.clone();
                        let can_cancel = matches!(j.status.as_str(), "queued" | "running");
                        let job_id = j.id.clone();
                        let job_id_sel = j.id.clone();
                        let on_select = on_select.clone();
                        let on_action = on_action.clone();

                        let cancel = move |_| {
                            let jid = job_id.clone();
                            let on_action = on_action.clone();
                            leptos::task::spawn_local(async move {
                                let _ = api::post::<_, serde_json::Value>(
                                    &format!("/jobs/{jid}/cancel"), &serde_json::json!({})
                                ).await;
                                on_action();
                            });
                        };

                        view! {
                            <tr>
                                <td>
                                    <a href="#" class="mono" style="color: var(--accent); cursor: pointer"
                                        on:click={
                                            let on_select = on_select.clone();
                                            let id = job_id_sel.clone();
                                            move |ev: leptos::ev::MouseEvent| {
                                                ev.prevent_default();
                                                on_select(id.clone());
                                            }
                                        }
                                    >{truncate_id(&j.id)}</a>
                                </td>
                                <td class="mono truncate">{j.test_suite}</td>
                                <td><StatusBadge status=status/></td>
                                <td>{j.priority.unwrap_or_else(|| "normal".into())}</td>
                                <td>{j.created_at.unwrap_or_else(|| "—".into())}</td>
                                <td>
                                    <Show when=move || can_cancel>
                                        <button class="btn btn-danger btn-sm" on:click=cancel.clone()>"Cancel"</button>
                                    </Show>
                                </td>
                            </tr>
                        }
                    }).collect::<Vec<_>>()}
                </tbody>
            </table>
        </div>
    }
}

#[component]
fn JobDetailView(job_id: String, on_close: impl Fn() + Send + Sync + 'static) -> impl IntoView {
    let jid = job_id.clone();
    let detail = LocalResource::new(move || {
        let jid = jid.clone();
        async move {
            api::get::<JobDetail>(&format!("/jobs/{jid}")).await.ok()
        }
    });

    let jid2 = job_id.clone();
    let progress = LocalResource::new(move || {
        let jid = jid2.clone();
        async move {
            api::get::<JobProgress>(&format!("/jobs/{jid}/progress")).await.unwrap_or_default()
        }
    });

    view! {
        <div class="card" style="margin-bottom: 1rem">
            <div class="card-header">
                <h2>"Job: " {truncate_id(&job_id)}</h2>
                <button class="btn btn-ghost btn-sm" on:click=move |_| on_close()>"✕ Close"</button>
            </div>

            // Progress bar
            <Suspense fallback=move || view! { <div class="loading">"Loading…"</div> }>
                {move || progress.get().as_deref().map(|p| {
                    let total = p.total.max(1);
                    let pass_pct = (p.passed * 100) / total;
                    let _fail_pct = (p.failed * 100) / total;
                    view! {
                        <div style="margin-bottom: 1rem">
                            <div style="display: flex; gap: 1rem; margin-bottom: 0.5rem; font-size: 0.85rem">
                                <span>"Total: " {p.total}</span>
                                <span style="color: var(--success)">"Passed: " {p.passed}</span>
                                <span style="color: var(--danger)">"Failed: " {p.failed}</span>
                                <span style="color: var(--accent)">"Running: " {p.running}</span>
                                <span style="color: var(--text-muted)">"Queued: " {p.queued}</span>
                            </div>
                            <div class="progress-bar" style="height: 12px">
                                <div class="fill success" style:width=format!("{pass_pct}%")></div>
                            </div>
                        </div>
                    }
                })}
            </Suspense>

            // Test cases table
            <Suspense fallback=move || view! { <div class="loading">"Loading…"</div> }>
                {move || detail.get().as_deref().map(|opt| {
                    match opt {
                        None => view! { <p class="empty-state">"Failed to load job details"</p> }.into_any(),
                        Some(d) => {
                            let cases = d.job.test_cases.clone();
                            if cases.is_empty() {
                                return view! { <p class="empty-state">"No test cases yet"</p> }.into_any();
                            }
                            view! {
                                <div class="table-wrap">
                                    <table>
                                        <thead>
                                            <tr>
                                                <th>"Test Name"</th>
                                                <th>"Status"</th>
                                                <th>"Attempt"</th>
                                                <th>"Duration"</th>
                                                <th>"Error"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {cases.into_iter().map(|tc| {
                                                let status = tc.status.clone();
                                                view! {
                                                    <tr>
                                                        <td class="mono">{tc.name}</td>
                                                        <td><StatusBadge status=status/></td>
                                                        <td>{tc.attempt}</td>
                                                        <td>{tc.duration_ms.map(|d| format!("{d}ms")).unwrap_or_else(|| "—".into())}</td>
                                                        <td class="truncate">{tc.error_message.unwrap_or_default()}</td>
                                                    </tr>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </tbody>
                                    </table>
                                </div>
                            }.into_any()
                        }
                    }
                })}
            </Suspense>
        </div>
    }
}
