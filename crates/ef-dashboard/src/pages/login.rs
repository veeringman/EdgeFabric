use leptos::prelude::*;
use gloo_storage::Storage;

use crate::api;

/// Login page — exchange API key for JWT.
#[component]
pub fn LoginPage() -> impl IntoView {
    let set_token = expect_context::<WriteSignal<Option<String>>>();
    let (api_key, set_api_key) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let key = api_key.get_untracked();
        if key.is_empty() {
            set_error.set(Some("Please enter an API key".into()));
            return;
        }
        set_loading.set(true);
        set_error.set(None);

        leptos::task::spawn_local(async move {
            match api::login(&key).await {
                Ok(resp) => {
                    let _ = gloo_storage::LocalStorage::set("ef_token", &resp.token);
                    let _ = gloo_storage::LocalStorage::set("ef_tenant", &resp.tenant_id);
                    set_token.set(Some(resp.token));
                    // Navigate to dashboard
                    if let Some(w) = web_sys::window() {
                        let _ = w.location().set_href("/");
                    }
                }
                Err(e) => {
                    set_error.set(Some(e));
                    set_loading.set(false);
                }
            }
        });
    };

    view! {
        <div class="login-page">
            <div class="login-card">
                <h1>"⚡ EdgeFabric"</h1>
                <p class="subtitle">"Device Farm Dashboard"</p>

                <Show when=move || error.get().is_some()>
                    <div class="alert error">{move || error.get().unwrap_or_default()}</div>
                </Show>

                <form on:submit=on_submit>
                    <div class="form-group">
                        <label for="api_key">"API Key"</label>
                        <input
                            id="api_key"
                            type="password"
                            placeholder="Enter your API key"
                            prop:value=api_key
                            on:input=move |ev| set_api_key.set(event_target_value(&ev))
                        />
                    </div>
                    <button
                        type="submit"
                        class="btn btn-primary"
                        disabled=move || loading.get()
                    >
                        {move || if loading.get() { "Signing in…" } else { "Sign In" }}
                    </button>
                </form>
            </div>
        </div>
    }
}
