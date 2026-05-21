mod api;
mod board_config;
mod components;
mod pages;

use gloo_storage::Storage;
use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use components::sidebar::Sidebar;
use pages::{
    admin::AdminPage, builds::BuildsPage, development_ide::DevelopmentIdePage, devices::DevicesPage, discovered_hosts::DiscoveredHostsPage,
    hw_pcb::HardwarePcbPage, hw_schematic::HardwareSchematicPage, hw_studio::HardwareStudioPage,
    integrations::IntegrationsPage, jobs::JobsPage, login::LoginPage, logs::LogsPage,
    overview::OverviewPage, quantum::QuantumPage, quantum_accelerators::QuantumAcceleratorsPage,
    quantum_designer::QuantumDesignerPage, quantum_jobs::QuantumJobsPage,
    quantum_results::QuantumResultsPage, sessions::SessionsPage, sessions_live::LiveSessionsPage,
    si_backends::SiliconBackendsPage,
    si_floorplan::SiliconFloorplanPage, si_qubit::SiliconQubitPage, si_rtl::SiliconRtlPage,
    si_schematic::SiliconSchematicPage,
    si_stackup::SiliconStackupPage,
    si_studio::SiliconStudioPage, si_synth::SiliconSynthPage, si_tapeout::SiliconTapeoutPage,
    veeros_shell::VeerosShellPage, virtual_iot_backends::VirtualIotBackendsPage,
    virtual_iot_designer::VirtualIotDesignerPage, virtual_iot_shell::VirtualIotShellPage,
};

/// Global signal for mobile sidebar open/close.
/// Provided via context so sidebar & shell can share state.
#[derive(Clone, Copy)]
struct SidebarOpen(ReadSignal<bool>, WriteSignal<bool>);

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
                <Route path=path!("/builds") view=move || view! { <AuthGuard><Shell><BuildsPage/></Shell></AuthGuard> }/>
                <Route path=path!("/development/ide") view=move || view! { <AuthGuard><Shell><DevelopmentIdePage/></Shell></AuthGuard> }/>
                <Route path=path!("/virtual-iot/backends") view=move || view! { <AuthGuard><Shell><VirtualIotBackendsPage/></Shell></AuthGuard> }/>
                <Route path=path!("/integrations") view=move || view! { <AuthGuard><Shell><IntegrationsPage/></Shell></AuthGuard> }/>
                <Route path=path!("/logs") view=move || view! { <AuthGuard><Shell><LogsPage/></Shell></AuthGuard> }/>
                <Route path=path!("/admin") view=move || view! { <AuthGuard><Shell><AdminPage/></Shell></AuthGuard> }/>
                <Route path=path!("/virtual-iot/new") view=move || view! { <AuthGuard><Shell><VirtualIotDesignerPage/></Shell></AuthGuard> }/>
                <Route path=path!("/virtual-iot/edit") view=move || view! { <AuthGuard><Shell><VirtualIotDesignerPage/></Shell></AuthGuard> }/>
                <Route path=path!("/virtual-iot/shell") view=move || view! { <AuthGuard><Shell><VirtualIotShellPage/></Shell></AuthGuard> }/>
                <Route path=path!("/veeros/shell") view=move || view! { <AuthGuard><Shell><VeerosShellPage/></Shell></AuthGuard> }/>
                <Route path=path!("/quantum") view=move || view! { <AuthGuard><Shell><QuantumPage/></Shell></AuthGuard> }/>
                <Route path=path!("/quantum/accelerators") view=move || view! { <AuthGuard><Shell><QuantumAcceleratorsPage/></Shell></AuthGuard> }/>
                <Route path=path!("/quantum/designer") view=move || view! { <AuthGuard><Shell><QuantumDesignerPage/></Shell></AuthGuard> }/>
                <Route path=path!("/quantum/jobs") view=move || view! { <AuthGuard><Shell><QuantumJobsPage/></Shell></AuthGuard> }/>
                <Route path=path!("/quantum/results/:id") view=move || view! { <AuthGuard><Shell><QuantumResultsPage/></Shell></AuthGuard> }/>
                <Route path=path!("/veeros/hosts") view=move || view! { <AuthGuard><Shell><DiscoveredHostsPage/></Shell></AuthGuard> }/>
                <Route path=path!("/hw") view=move || view! { <AuthGuard><Shell><HardwareStudioPage/></Shell></AuthGuard> }/>
                <Route path=path!("/silicon") view=move || view! { <AuthGuard><Shell><SiliconStudioPage/></Shell></AuthGuard> }/>
                <Route path=path!("/silicon/rtl") view=move || view! { <AuthGuard><Shell><SiliconRtlPage/></Shell></AuthGuard> }/>
                <Route path=path!("/silicon/floorplan") view=move || view! { <AuthGuard><Shell><SiliconFloorplanPage/></Shell></AuthGuard> }/>
                <Route path=path!("/silicon/synth") view=move || view! { <AuthGuard><Shell><SiliconSynthPage/></Shell></AuthGuard> }/>
                <Route path=path!("/silicon/tapeout") view=move || view! { <AuthGuard><Shell><SiliconTapeoutPage/></Shell></AuthGuard> }/>
                <Route path=path!("/silicon/stackup") view=move || view! { <AuthGuard><Shell><SiliconStackupPage/></Shell></AuthGuard> }/>
                <Route path=path!("/silicon/schematic") view=move || view! { <AuthGuard><Shell><SiliconSchematicPage/></Shell></AuthGuard> }/>
                <Route path=path!("/silicon/backends") view=move || view! { <AuthGuard><Shell><SiliconBackendsPage/></Shell></AuthGuard> }/>
                <Route path=path!("/silicon/projects/sqc-q5") view=move || view! { <AuthGuard><Shell><SiliconQubitPage/></Shell></AuthGuard> }/>
                <Route path=path!("/hw/schematic") view=move || view! { <AuthGuard><Shell><HardwareSchematicPage/></Shell></AuthGuard> }/>
                <Route path=path!("/hw/pcb") view=move || view! { <AuthGuard><Shell><HardwarePcbPage/></Shell></AuthGuard> }/>
                <Route path=path!("/hw/library") view=move || view! { <AuthGuard><Shell><HardwareStudioPage/></Shell></AuthGuard> }/>
                <Route path=path!("/hw/bom") view=move || view! { <AuthGuard><Shell><HardwareStudioPage/></Shell></AuthGuard> }/>
                <Route path=path!("/hw/copilot") view=move || view! { <AuthGuard><Shell><HardwareStudioPage/></Shell></AuthGuard> }/>
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
    let (sidebar_open, set_sidebar_open) = signal(false);
    provide_context(SidebarOpen(sidebar_open, set_sidebar_open));

    let toggle = move |_| set_sidebar_open.update(|v| *v = !*v);
    let close_sidebar = move |_| set_sidebar_open.set(false);

    view! {
        <div class="app-shell">
            // Backdrop overlay for mobile
            <div
                class="sidebar-backdrop"
                class:open=move || sidebar_open.get()
                on:click=close_sidebar
            />
            <Sidebar/>
            <div class="main-content">
                <header class="topbar">
                    <div class="topbar-left">
                        <button class="hamburger-btn" on:click=toggle aria-label="Toggle menu">
                            <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                <line x1="3" y1="6" x2="21" y2="6"/>
                                <line x1="3" y1="12" x2="21" y2="12"/>
                                <line x1="3" y1="18" x2="21" y2="18"/>
                            </svg>
                        </button>
                        <img class="topbar-logo" src="/logo.png" alt="EdgeFabric" />
                        <div class="topbar-title-stack topbar-title-full">
                            <span class="topbar-title topbar-title-primary">"Multi-Platform Design, Build, Test & Deploy"</span>
                            <span class="topbar-title topbar-title-secondary">"Orchestration and Management · iOS · Android · IoT · Quantum · PCB · Silicon"</span>
                        </div>
                        <span class="topbar-title topbar-title-short">"Design · Build · Test · Deploy · Manage"</span>
                    </div>
                    <div class="topbar-actions">
                        <span class="topbar-status">
                            <span class="status-dot online"></span>
                            "API Connected"
                        </span>
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
