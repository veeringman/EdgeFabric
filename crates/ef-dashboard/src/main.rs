mod api;
mod components;
mod pages;

use gloo_storage::Storage;
use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes, A},
    path,
};

use components::sidebar::Sidebar;
use pages::{
    admin::AdminPage, devices::DevicesPage, jobs::JobsPage, login::LoginPage, logs::LogsPage,
    overview::OverviewPage, sessions::SessionsPage, sessions_live::LiveSessionsPage,
};

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    provide_meta_context();

    let (token, set_token) = signal(
        gloo_storage::LocalStorage::get::<String>("ef_token").ok(),
    );

    provide_context(token);
    provide_context(set_token);

    view! {
        <Title text="EdgeFabric Dashboard"/>
        <Router>
            <Routes fallback=|| view! { <p class="empty-state">"404 — Page not found"</p> }>
                <Route path=path!("/login") view=LoginPage/>
                <Route path=path!("/") view=move || view! { <AuthGuard><Shell><OverviewPage/></Shell></AuthGuard> }/>
                <Route path=path!("/devices") view=move || view! { <AuthGuard><Shell><DevicesPage/></Shell></AuthGuard> }/>
                <Route path=path!("/sessions") view=move || view! { <AuthGuard><Shell><SessionsPage/></Shell></AuthGuard> }/>
                <Route path=path!("/sessions/live") view=move || view! { <AuthGuard><Shell><LiveSessionsPage/></Shell></AuthGuard> }/>
                <Route path=path!("/jobs") view=move || view! { <AuthGuard><Shell><JobsPage/></Shell></AuthGuard> }/>
                <Route path=path!("/logs") view=move || view! { <AuthGuard><Shell><LogsPage/></Shell></AuthGuard> }/>
                <Route path=path!("/admin") view=move || view! { <AuthGuard><Shell><AdminPage/></Shell></AuthGuard> }/>
            </Routes>
        </Router>
    }
}

/// Redirects to /login if no token is stored.
#[component]
fn AuthGuard(children: Children) -> impl IntoView {
    let token = expect_context::<ReadSignal<Option<String>>>();

    // If not logged in at render time, redirect immediately.
    // (Reactive check not needed — route re-renders on navigation.)
    if token.get_untracked().is_none() {
        return view! { <leptos_router::components::Redirect path="/login"/> }.into_any();
    }

    children().into_any()
}

/// App shell with sidebar + topbar.
#[component]
fn Shell(children: Children) -> impl IntoView {
    view! {
        <div class="app-shell">
            <Sidebar/>
            <div class="main-content">
                <header class="topbar">
                    <span class="topbar-title">"EdgeFabric"</span>
                    <div class="topbar-actions">
                        <LogoutButton/>
                    </div>
                </header>
                <main class="page">
                    {children()}
                </main>
            </div>
        </div>
    }
}

#[component]
fn LogoutButton() -> impl IntoView {
    let set_token = expect_context::<WriteSignal<Option<String>>>();

    let logout = move |_| {
        gloo_storage::LocalStorage::delete("ef_token");
        set_token.set(None);
    };

    view! {
        <button class="btn btn-ghost btn-sm" on:click=logout>"Logout"</button>
    }
}
