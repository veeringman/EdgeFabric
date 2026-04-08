use leptos::prelude::*;
use leptos_router::components::A;

use crate::SidebarOpen;

#[component]
pub fn Sidebar() -> impl IntoView {
    // Close sidebar on nav click (mobile)
    let close = move |_: web_sys::MouseEvent| {
        if let Some(SidebarOpen(_, set_open)) = use_context::<SidebarOpen>() {
            set_open.set(false);
        }
    };

    let is_open = move || {
        use_context::<SidebarOpen>()
            .map(|SidebarOpen(r, _)| r.get())
            .unwrap_or(false)
    };

    view! {
        <aside class="sidebar" class:open=is_open>
            <div class="sidebar-brand">
                <img class="brand-logo" src="/logo.png" alt="EdgeFabric" />
            </div>
            <nav class="sidebar-nav" on:click=close>
                <A href="/">"📊 Overview"</A>
                <A href="/devices">"📱 Devices"</A>
                <A href="/sessions">"🔗 Sessions"</A>
                <A href="/sessions/live">"📺 Live Viewer"</A>
                <A href="/jobs">"🧪 Test Jobs"</A>
                <A href="/logs">"📜 Logs"</A>
                <A href="/admin">"⚙️ Admin"</A>
            </nav>
            <div class="sidebar-footer">
                <span class="env-badge">"Development"</span>
                <span>"v0.1.0"</span>
            </div>
        </aside>
    }
}
