use leptos::prelude::*;

use crate::components::stat_card::StatCard;

/// Hardware Studio landing page.
///
/// Entry point for the PCB / Electronic Circuit Design track. Shows
/// at-a-glance stats and quick actions into the Schematic Editor,
/// PCB Layout Editor, Library Manager, and AI Co-Pilot.
#[component]
pub fn HardwareStudioPage() -> impl IntoView {
    // Placeholder stats — wired to real APIs in a later phase.
    let projects = Signal::derive(|| "0".to_string());
    let revisions = Signal::derive(|| "0".to_string());
    let open_drc = Signal::derive(|| "0".to_string());
    let fab_orders = Signal::derive(|| "0".to_string());

    view! {
        <div class="page-container hw-studio-page">
            <div class="page-header">
                <h1>"Hardware Studio"</h1>
                <p class="page-subtitle">
                    "Design boards from idea to fabrication. AI-assisted schematic capture, realistic PCB layout, and direct hand-off to EdgeFabric runners."
                </p>
            </div>

            <div class="stats-grid">
                <StatCard label="Projects" value=projects/>
                <StatCard label="Revisions" value=revisions/>
                <StatCard label="Open DRC Issues" value=open_drc/>
                <StatCard label="Fab Orders" value=fab_orders/>
            </div>

            <section class="quantum-section">
                <div class="section-header">
                    <h2>"Quick Actions"</h2>
                </div>
                <div class="quick-actions">
                    <a href="/hw/schematic" class="action-card">
                        <span class="action-kicker">"Capture"</span>
                        <h3>"Schematic Editor"</h3>
                        <p>"Drag-drop symbols, hierarchical sheets, live ERC."</p>
                    </a>
                    <a href="/hw/pcb" class="action-card">
                        <span class="action-kicker">"Layout"</span>
                        <h3>"PCB Designer"</h3>
                        <p>"Realistic 2D/3D board view with push-and-shove routing."</p>
                    </a>
                    <a href="/hw/library" class="action-card">
                        <span class="action-kicker">"Manage"</span>
                        <h3>"Library Manager"</h3>
                        <p>"Symbols, footprints, and 3D models."</p>
                    </a>
                    <a href="/hw/copilot" class="action-card">
                        <span class="action-kicker">"Generate"</span>
                        <h3>"AI Co-Pilot"</h3>
                        <p>"Describe a board. Get a starting schematic and placement."</p>
                    </a>
                </div>
            </section>

            <section class="quantum-section">
                <div class="section-header">
                    <h2>"Recent Projects"</h2>
                </div>
                <div class="empty-state">
                    "No projects yet. Start from a spec, blank schematic, or import a KiCad project."
                </div>
            </section>
        </div>
    }
}
