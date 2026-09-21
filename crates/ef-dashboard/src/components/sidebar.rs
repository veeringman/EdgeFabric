use leptos::prelude::*;
use leptos_router::components::A;

use crate::SidebarOpen;

#[component]
fn NavIcon(name: String) -> impl IntoView {
    match name.as_str() {
        "overview" => view! {
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <path d="M3 3v18h18"/>
                <path d="M7 14l3-3 3 2 4-5"/>
            </svg>
        }
        .into_any(),
        "devices" => view! {
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <rect x="7" y="2.5" width="10" height="19" rx="2"/>
                <path d="M10 5.5h4M11 18.5h2"/>
            </svg>
        }
        .into_any(),
        "sessions" => view! {
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="7" cy="12" r="2"/>
                <circle cx="17" cy="7" r="2"/>
                <circle cx="17" cy="17" r="2"/>
                <path d="M9 11 15 8M9 13 15 16"/>
            </svg>
        }
        .into_any(),
        "live" => view! {
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <rect x="3" y="5" width="14" height="14" rx="2"/>
                <path d="m17 10 4-2v8l-4-2z"/>
            </svg>
        }
        .into_any(),
        "jobs" => view! {
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <path d="M9 2h6v4H9z"/>
                <rect x="5" y="6" width="14" height="16" rx="2"/>
                <path d="M8 11h8M8 15h8"/>
            </svg>
        }
        .into_any(),
        "builds" => view! {
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <path d="M12 3v6"/>
                <path d="m8 7 4-4 4 4"/>
                <rect x="4" y="11" width="16" height="10" rx="2"/>
                <path d="M8 15h8M8 18h5"/>
            </svg>
        }
        .into_any(),
        "logs" => view! {
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <path d="M4 4h16v16H4z"/>
                <path d="M8 9h8M8 13h8M8 17h5"/>
            </svg>
        }
        .into_any(),
        "iot" => view! {
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <rect x="6" y="3" width="12" height="18" rx="2"/>
                <circle cx="12" cy="8" r="1.5"/>
                <path d="M8 13h8M8 16h8"/>
                <path d="M9 3V1M15 3V1"/>
            </svg>
        }
        .into_any(),
            "quantum" => view! {
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <circle cx="8" cy="8" r="2"/>
                    <circle cx="16" cy="8" r="2"/>
                    <circle cx="12" cy="16" r="2"/>
                    <path d="M8 10v4M16 10v4M13 14h-2M12 12v2"/>
                </svg>
            }
            .into_any(),
            "silicon" => view! {
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
                    <rect x="6" y="6" width="12" height="12" rx="1"/>
                    <rect x="9.5" y="9.5" width="5" height="5"/>
                    <path d="M9 3v3M12 3v3M15 3v3M9 18v3M12 18v3M15 18v3M3 9h3M3 12h3M3 15h3M18 9h3M18 12h3M18 15h3"/>
                </svg>
            }
            .into_any(),
            "download" => view! {
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M12 3v12"/>
                    <path d="m7 11 5 5 5-5"/>
                    <path d="M5 21h14"/>
                </svg>
            }
            .into_any(),
            "admin" => view! {
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <circle cx="12" cy="12" r="3"/>
                    <path d="M19.4 15a1 1 0 0 0 .2 1.1l.1.1a2 2 0 0 1-2.8 2.8l-.1-.1a1 1 0 0 0-1.1-.2 1 1 0 0 0-.6.9V20a2 2 0 0 1-4 0v-.2a1 1 0 0 0-.6-.9 1 1 0 0 0-1.1.2l-.1.1a2 2 0 0 1-2.8-2.8l.1-.1a1 1 0 0 0 .2-1.1 1 1 0 0 0-.9-.6H4a2 2 0 0 1 0-4h.2a1 1 0 0 0 .9-.6 1 1 0 0 0-.2-1.1l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1 1 0 0 0 1.1.2h.1a1 1 0 0 0 .6-.9V4a2 2 0 0 1 4 0v.2a1 1 0 0 0 .6.9h.1a1 1 0 0 0 1.1-.2l.1-.1a2 2 0 0 1 2.8 2.8l-.1.1a1 1 0 0 0-.2 1.1v.1a1 1 0 0 0 .9.6H20a2 2 0 0 1 0 4h-.2a1 1 0 0 0-.9.6z"/>
                </svg>
            }
            .into_any(),
        _ => view! {
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="12" cy="12" r="3"/>
                <path d="M19.4 15a1 1 0 0 0 .2 1.1l.1.1a2 2 0 0 1-2.8 2.8l-.1-.1a1 1 0 0 0-1.1-.2 1 1 0 0 0-.6.9V20a2 2 0 0 1-4 0v-.2a1 1 0 0 0-.6-.9 1 1 0 0 0-1.1.2l-.1.1a2 2 0 0 1-2.8-2.8l.1-.1a1 1 0 0 0 .2-1.1 1 1 0 0 0-.9-.6H4a2 2 0 0 1 0-4h.2a1 1 0 0 0 .9-.6 1 1 0 0 0-.2-1.1l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1 1 0 0 0 1.1.2h.1a1 1 0 0 0 .6-.9V4a2 2 0 0 1 4 0v.2a1 1 0 0 0 .6.9h.1a1 1 0 0 0 1.1-.2l.1-.1a2 2 0 0 1 2.8 2.8l-.1.1a1 1 0 0 0-.2 1.1v.1a1 1 0 0 0 .9.6H20a2 2 0 0 1 0 4h-.2a1 1 0 0 0-.9.6z"/>
            </svg>
        }
        .into_any(),
    }
}

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
                <div class="nav-section-title">"Core Platform"</div>
                <A href="/"><span class="nav-icon"><NavIcon name="overview".to_string()/></span><span>"Overview"</span></A>
                <A href="/devices"><span class="nav-icon"><NavIcon name="devices".to_string()/></span><span>"Devices & Hosts"</span></A>
                <A href="/sessions"><span class="nav-icon"><NavIcon name="sessions".to_string()/></span><span>"Sessions"</span></A>
                <A href="/sessions/live"><span class="nav-icon"><NavIcon name="live".to_string()/></span><span>"Live Viewer"</span></A>
                <A href="/jobs"><span class="nav-icon"><NavIcon name="jobs".to_string()/></span><span>"Test Jobs"</span></A>
                <A href="/ml"><span class="nav-icon"><NavIcon name="jobs".to_string()/></span><span>"ML Fabric"</span></A>
                <A href="/builds"><span class="nav-icon"><NavIcon name="builds".to_string()/></span><span>"Builds"</span></A>
                <A href="/development/ide"><span class="nav-icon"><NavIcon name="builds".to_string()/></span><span>"Development IDE"</span></A>
                <A href="/logs"><span class="nav-icon"><NavIcon name="logs".to_string()/></span><span>"Logs"</span></A>
                <A href="/resources"><span class="nav-icon"><NavIcon name="download".to_string()/></span><span>"Downloads"</span></A>

                <div class="nav-section-title">"IoT & Integrations"</div>
                <A href="/virtual-iot/new"><span class="nav-icon"><NavIcon name="iot".to_string()/></span><span>"IoT Designer"</span></A>
                <A href="/virtual-iot/backends"><span class="nav-icon"><NavIcon name="iot".to_string()/></span><span>"IoT Backends"</span></A>
                <A href="/integrations"><span class="nav-icon"><NavIcon name="iot".to_string()/></span><span>"Integrations"</span></A>

                <div class="nav-section-title">"Quantum & VeerOS"</div>
                <A href="/quantum"><span class="nav-icon"><NavIcon name="quantum".to_string()/></span><span>"Quantum"</span></A>
                <A href="/quantum/accelerators"><span class="nav-icon"><NavIcon name="quantum".to_string()/></span><span>"Accelerators"</span></A>
                <A href="/quantum/designer"><span class="nav-icon"><NavIcon name="quantum".to_string()/></span><span>"Circuit Designer"</span></A>
                <A href="/quantum/jobs"><span class="nav-icon"><NavIcon name="quantum".to_string()/></span><span>"Jobs"</span></A>
                <A href="/veeros/hosts"><span class="nav-icon"><NavIcon name="quantum".to_string()/></span><span>"VeerOS Hosts"</span></A>

                <div class="nav-section-title">"Chip Design"</div>
                <A href="/silicon"><span class="nav-icon"><NavIcon name="silicon".to_string()/></span><span>"Silicon Studio"</span></A>
                <A href="/silicon/rtl"><span class="nav-icon"><NavIcon name="silicon".to_string()/></span><span>"RTL Workspace"</span></A>
                <A href="/silicon/floorplan"><span class="nav-icon"><NavIcon name="silicon".to_string()/></span><span>"Floorplan & PnR"</span></A>
                <A href="/silicon/synth"><span class="nav-icon"><NavIcon name="silicon".to_string()/></span><span>"Synthesis & QoR"</span></A>
                <A href="/silicon/tapeout"><span class="nav-icon"><NavIcon name="silicon".to_string()/></span><span>"Tape-out"</span></A>
                <A href="/silicon/stackup"><span class="nav-icon"><NavIcon name="silicon".to_string()/></span><span>"3D Stack-up Viewer"</span></A>
                <A href="/silicon/schematic"><span class="nav-icon"><NavIcon name="silicon".to_string()/></span><span>"Schematic Editor"</span></A>
                <A href="/silicon/backends"><span class="nav-icon"><NavIcon name="silicon".to_string()/></span><span>"Sim / FPGA Backends"</span></A>
                <A href="/silicon/projects/sqc-q5"><span class="nav-icon"><NavIcon name="silicon".to_string()/></span><span>"SQC-Q5 (Qubit Chip)"</span></A>

                <div class="nav-section-title">"Hardware Studio"</div>
                <A href="/hw"><span class="nav-icon"><NavIcon name="quantum".to_string()/></span><span>"Studio"</span></A>
                <A href="/hw/schematic"><span class="nav-icon"><NavIcon name="quantum".to_string()/></span><span>"Schematic"</span></A>
                <A href="/hw/pcb"><span class="nav-icon"><NavIcon name="quantum".to_string()/></span><span>"PCB Designer"</span></A>
                <A href="/hw/library"><span class="nav-icon"><NavIcon name="quantum".to_string()/></span><span>"Library"</span></A>
                <A href="/hw/bom"><span class="nav-icon"><NavIcon name="quantum".to_string()/></span><span>"BOM"</span></A>
                <A href="/hw/copilot"><span class="nav-icon"><NavIcon name="quantum".to_string()/></span><span>"AI Co-Pilot"</span></A>

                <div class="nav-section-title">"Administration"</div>
                <A href="/admin"><span class="nav-icon"><NavIcon name="admin".to_string()/></span><span>"Admin"</span></A>
            </nav>
            <div class="sidebar-footer">
                <span class="env-badge">"Development"</span>
                <span>"v0.1.0"</span>
            </div>
        </aside>
    }
}
