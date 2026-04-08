use leptos::prelude::*;
use serde::Serialize;

use crate::api::{self, Session, SessionListResponse};
use crate::components::status_badge::StatusBadge;

/// Session management — list, create, and end sessions.
#[component]
pub fn SessionsPage() -> impl IntoView {
    let (refresh, set_refresh) = signal(0u32);
    let (show_create, set_show_create) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    // Auto-refresh every 5 seconds
    Effect::new(move || {
        let handle = gloo_timers::callback::Interval::new(5_000, move || {
            set_refresh.update(|n| *n = n.wrapping_add(1));
        });
        std::mem::forget(handle);
    });

    let sessions = LocalResource::new(move || {
        let _ = refresh.get();
        async move {
            api::get::<SessionListResponse>("/sessions").await.unwrap_or_default()
        }
    });

    let do_refresh = move |_| set_refresh.update(|n| *n += 1);

    view! {
        <h1 style="margin-bottom: 1rem">"Sessions"</h1>

        <Show when=move || error.get().is_some()>
            <div class="alert error">{move || error.get().unwrap_or_default()}</div>
        </Show>

        <div class="filters">
            <button class="btn btn-primary btn-sm" on:click=move |_| set_show_create.update(|v| *v = !*v)>
                {move || if show_create.get() { "Cancel" } else { "+ New Session" }}
            </button>
            <button class="btn btn-ghost btn-sm" on:click=do_refresh>"↻ Refresh"</button>
        </div>

        <Show when=move || show_create.get()>
            <CreateSessionForm on_created=move || {
                set_show_create.set(false);
                set_refresh.update(|n| *n += 1);
            } on_error=move |e| set_error.set(Some(e))/>
        </Show>

        <div class="card">
            <Suspense fallback=move || view! { <div class="loading">"Loading sessions…"</div> }>
                {move || sessions.get().as_deref().map(|data| {
                    if data.sessions.is_empty() {
                        return view! { <p class="empty-state">"No active sessions"</p> }.into_any();
                    }
                    let sess = data.sessions.clone();
                    let set_refresh = set_refresh.clone();
                    view! { <SessionTable sessions=sess on_action=move || set_refresh.update(|n| *n += 1)/> }.into_any()
                })}
            </Suspense>
        </div>
    }
}

#[derive(Serialize)]
struct CreateSessionReq {
    device_id: String,
    duration_minutes: u32,
}

#[component]
fn CreateSessionForm(
    on_created: impl Fn() + Send + Sync + 'static + Clone,
    on_error: impl Fn(String) + Send + Sync + 'static + Clone,
) -> impl IntoView {
    let (device_id, set_device_id) = signal(String::new());
    let (duration, set_duration) = signal("30".to_string());
    let (loading, set_loading) = signal(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let did = device_id.get_untracked();
        let dur: u32 = duration.get_untracked().parse().unwrap_or(30);
        let on_created = on_created.clone();
        let on_error = on_error.clone();

        set_loading.set(true);
        leptos::task::spawn_local(async move {
            let req = CreateSessionReq { device_id: did, duration_minutes: dur };
            match api::post::<_, serde_json::Value>("/sessions", &req).await {
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
                    <label>"Device ID"</label>
                    <input type="text" placeholder="UUID" prop:value=device_id
                        on:input=move |ev| set_device_id.set(event_target_value(&ev))/>
                </div>
                <div class="form-group">
                    <label>"Duration (min)"</label>
                    <input type="number" prop:value=duration style="width:80px"
                        on:input=move |ev| set_duration.set(event_target_value(&ev))/>
                </div>
                <button type="submit" class="btn btn-primary btn-sm" disabled=move || loading.get()>
                    {move || if loading.get() { "Creating…" } else { "Create" }}
                </button>
            </form>
        </div>
    }
}

#[component]
fn SessionTable(sessions: Vec<Session>, on_action: impl Fn() + Send + Sync + 'static + Clone) -> impl IntoView {
    view! {
        <div class="table-wrap">
            <table>
                <thead>
                    <tr>
                        <th>"ID"</th>
                        <th>"Device"</th>
                        <th>"Status"</th>
                        <th>"Expires"</th>
                        <th>"Created"</th>
                        <th>"Actions"</th>
                    </tr>
                </thead>
                <tbody>
                    {sessions.into_iter().map(|s| {
                        let status = s.status.clone();
                        let is_active = s.status == "active";
                        let session_id = s.id.clone();
                        let id_short = truncate_id(&s.id);
                        let device_short = truncate_id(&s.device_id);
                        let on_action = on_action.clone();
                        let end_session = move |_| {
                            let sid = session_id.clone();
                            let on_action = on_action.clone();
                            leptos::task::spawn_local(async move {
                                let _ = api::post::<_, serde_json::Value>(
                                    &format!("/sessions/{sid}/end"), &serde_json::json!({})
                                ).await;
                                on_action();
                            });
                        };
                        view! {
                            <tr>
                                <td class="mono truncate">{id_short}</td>
                                <td class="mono truncate">{device_short}</td>
                                <td><StatusBadge status=status/></td>
                                <td>{s.expires_at.unwrap_or_else(|| "—".into())}</td>
                                <td>{s.created_at.unwrap_or_else(|| "—".into())}</td>
                                <td>
                                    <Show when=move || is_active>
                                        <button class="btn btn-danger btn-sm" on:click=end_session.clone()>"End"</button>
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

fn truncate_id(s: &str) -> String {
    s.chars().take(8).collect()
}
