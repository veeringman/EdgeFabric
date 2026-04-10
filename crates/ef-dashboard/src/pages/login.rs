use leptos::prelude::*;
use gloo_storage::Storage;

use crate::api;

/// Login page — dual auth: API Key or Username/Password (OAuth2 via AuthBuddy).
#[component]
pub fn LoginPage() -> impl IntoView {
    let set_token = expect_context::<WriteSignal<Option<String>>>();

    // Tab state: "apikey" or "password"
    let (tab, set_tab) = signal("apikey".to_string());

    // API Key form state
    let (api_key, set_api_key) = signal("efdev-key-00000000".to_string());

    // Username/Password form state
    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());

    // Shared state
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    // Handle successful login response
    let handle_login_success = move |resp: api::LoginResponse| {
        let _ = gloo_storage::LocalStorage::set("ef_token", &resp.token);
        let _ = gloo_storage::LocalStorage::set("ef_tenant", &resp.tenant_id);
        set_token.set(Some(resp.token));
        if let Some(w) = web_sys::window() {
            let _ = w.location().set_href("/");
        }
    };

    // API Key login
    let on_apikey_submit = move |ev: leptos::ev::SubmitEvent| {
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
                Ok(resp) => handle_login_success(resp),
                Err(e) => {
                    set_error.set(Some(e));
                    set_loading.set(false);
                }
            }
        });
    };

    // Username/Password login (AuthBuddy OAuth2 — placeholder)
    let on_password_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let user = username.get_untracked();
        let pass = password.get_untracked();
        if user.is_empty() || pass.is_empty() {
            set_error.set(Some("Please enter username and password".into()));
            return;
        }
        set_loading.set(true);
        set_error.set(None);

        leptos::task::spawn_local(async move {
            match api::login_password(&user, &pass).await {
                Ok(resp) => handle_login_success(resp),
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
                // Logo
                <div class="login-logo">
                    <img src="/logo.png" alt="EdgeFabric" class="login-logo-img" />
                </div>
                <p class="subtitle">"Device Farm Dashboard"</p>

                // Auth method tabs
                <div class="login-tabs">
                    <button
                        class="login-tab"
                        class:active=move || tab.get() == "apikey"
                        on:click=move |_| { set_tab.set("apikey".into()); set_error.set(None); }
                    >
                        "🔑 API Key"
                    </button>
                    <button
                        class="login-tab"
                        class:active=move || tab.get() == "password"
                        on:click=move |_| { set_tab.set("password".into()); set_error.set(None); }
                    >
                        "👤 Sign In"
                    </button>
                </div>

                // Error banner
                <Show when=move || error.get().is_some()>
                    <div class="alert error">{move || error.get().unwrap_or_default()}</div>
                </Show>

                // API Key form
                <Show when=move || tab.get() == "apikey">
                    <form on:submit=on_apikey_submit>
                        <div class="form-group">
                            <label for="api_key">"API Key"</label>
                            <input
                                id="api_key"
                                type="text"
                                placeholder="ef-xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
                                prop:value=api_key
                                on:input=move |ev| set_api_key.set(event_target_value(&ev))
                            />
                        </div>
                        <button type="submit" class="btn btn-primary" disabled=move || loading.get()>
                            {move || if loading.get() { "Authenticating…" } else { "Authenticate" }}
                        </button>
                    </form>
                </Show>

                // Username/Password form (OAuth2 via AuthBuddy)
                <Show when=move || tab.get() == "password">
                    <form on:submit=on_password_submit>
                        <div class="form-group">
                            <label for="username">"Username"</label>
                            <input
                                id="username"
                                type="text"
                                placeholder="Enter your username"
                                autocomplete="username"
                                prop:value=username
                                on:input=move |ev| set_username.set(event_target_value(&ev))
                            />
                        </div>
                        <div class="form-group">
                            <label for="password">"Password"</label>
                            <input
                                id="password"
                                type="password"
                                placeholder="Enter your password"
                                autocomplete="current-password"
                                prop:value=password
                                on:input=move |ev| set_password.set(event_target_value(&ev))
                            />
                        </div>
                        <button type="submit" class="btn btn-primary" disabled=move || loading.get()>
                            {move || if loading.get() { "Signing in…" } else { "Sign In" }}
                        </button>
                        <p class="login-hint">"OAuth2 via AuthBuddy"</p>
                    </form>
                </Show>
            </div>
        </div>
    }
}
