use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn Sidebar() -> impl IntoView {
    view! {
        <aside class="sidebar">
            <div class="sidebar-brand">
                <span class="icon">"⚡"</span>
                <span>"EdgeFabric"</span>
            </div>
            <nav class="sidebar-nav">
                <A href="/">"📊 Overview"</A>
                <A href="/devices">"📱 Devices"</A>
                <A href="/sessions">"🔗 Sessions"</A>
                <A href="/sessions/live">"📺 Live Viewer"</A>
                <A href="/jobs">"🧪 Test Jobs"</A>
                <A href="/logs">"📜 Logs"</A>
                <A href="/admin">"⚙️ Admin"</A>
            </nav>
            <div class="sidebar-footer">
                "v0.1.0"
            </div>
        </aside>
    }
}
