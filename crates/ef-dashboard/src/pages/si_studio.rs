use leptos::prelude::*;

use crate::components::stat_card::StatCard;

#[component]
pub fn SiliconStudioPage() -> impl IntoView {
    let projects = Signal::derive(|| "3".to_string());
    let active_runs = Signal::derive(|| "2".to_string());
    let open_violations = Signal::derive(|| "7".to_string());
    let tapeout_ready = Signal::derive(|| "1".to_string());

    view! {
        <div class="page-container silicon-page">
            <div class="page-header">
                <div>
                    <span class="section-kicker">"Chip Design"</span>
                    <h1>"Silicon Studio"</h1>
                    <p class="page-subtitle">
                        "RTL, verification, synthesis, place-and-route, and tape-out — all in one workspace."
                    </p>
                </div>
            </div>

            <div class="stats-grid">
                <StatCard label="Projects" value=projects/>
                <StatCard label="Active Runs" value=active_runs/>
                <StatCard label="Open Violations" value=open_violations/>
                <StatCard label="Tape-out Ready" value=tapeout_ready/>
            </div>

            <section class="silicon-section">
                <div class="section-header">
                    <h2>"Workspace"</h2>
                </div>
                <div class="silicon-grid">
                    <a href="/silicon/rtl" class="action-card">
                        <span class="action-kicker">"Author"</span>
                        <h3>"RTL Workspace"</h3>
                        <p>"Browse module hierarchy and edit Verilog/SystemVerilog with inline lints."</p>
                    </a>
                    <a href="/silicon/floorplan" class="action-card">
                        <span class="action-kicker">"Layout"</span>
                        <h3>"Floorplan & PnR"</h3>
                        <p>"Interactive macro placement with congestion and density heatmaps."</p>
                    </a>
                    <a href="/silicon/synth" class="action-card">
                        <span class="action-kicker">"Build"</span>
                        <h3>"Synthesis & QoR"</h3>
                        <p>"Compare timing, area, and power across runs and corners."</p>
                    </a>
                    <a href="/silicon/tapeout" class="action-card">
                        <span class="action-kicker">"Sign-off"</span>
                        <h3>"Tape-out"</h3>
                        <p>"DRC/LVS/STA gating, GDSII artifacts, and shuttle submission."</p>
                    </a>
                    <a href="/silicon/stackup" class="action-card">
                        <span class="action-kicker">"3D View"</span>
                        <h3>"Stack-up Viewer"</h3>
                        <p>"Isometric layers, vias, and transistor-level features for any project."</p>
                    </a>
                </div>
            </section>

            <section class="silicon-section">
                <div class="section-header">
                    <h2>"Recent Projects"</h2>
                </div>
                <table class="silicon-table">
                    <thead>
                        <tr>
                            <th>"Project"</th>
                            <th>"Target"</th>
                            <th>"PDK"</th>
                            <th>"Last Run"</th>
                            <th>"Status"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr>
                            <td><a href="/silicon/projects/sqc-q5">"sqc-q5 · superconducting qubit"</a></td>
                            <td>"Quantum ASIC"</td>
                            <td>"Custom · Nb/Al on sapphire"</td>
                            <td>"EM sweep · 22m ago"</td>
                            <td><span class="badge active">"Layout"</span></td>
                        </tr>
                        <tr>
                            <td><a href="/silicon/rtl">"riscv-core-v1"</a></td>
                            <td>"ASIC"</td>
                            <td>"sky130A"</td>
                            <td>"PnR · 12m ago"</td>
                            <td><span class="badge running">"Routing"</span></td>
                        </tr>
                        <tr>
                            <td><a href="/silicon/rtl">"aes-128-accel"</a></td>
                            <td>"ASIC"</td>
                            <td>"ihp-sg13g2"</td>
                            <td>"Synth · 1h ago"</td>
                            <td><span class="badge passed">"Clean"</span></td>
                        </tr>
                        <tr>
                            <td><a href="/silicon/rtl">"uart-loopback"</a></td>
                            <td>"FPGA · ECP5"</td>
                            <td>"—"</td>
                            <td>"Bitstream · 3h ago"</td>
                            <td><span class="badge passed">"Built"</span></td>
                        </tr>
                    </tbody>
                </table>
            </section>
        </div>

        <style>{r#"
            .silicon-page { display: flex; flex-direction: column; gap: 1.25rem; padding: 1.5rem; }
            .silicon-page .page-header h1 { margin: 0; color: var(--text-primary); }
            .section-kicker {
                display: inline-flex;
                margin-bottom: 0.35rem;
                padding: 0.15rem 0.55rem;
                border-radius: 999px;
                background: rgba(59,130,246,0.15);
                color: var(--accent, #3b82f6);
                font-size: 0.72rem;
                font-weight: 700;
                letter-spacing: 0.09em;
                text-transform: uppercase;
            }
            .silicon-page .page-subtitle { margin: 0.4rem 0 0; color: var(--text-secondary); }
            .silicon-section {
                background: var(--card-bg, var(--bg-card));
                border: 1px solid var(--border-color, var(--border));
                border-radius: 12px;
                padding: 1.25rem;
            }
            .section-header { display: flex; align-items: center; justify-content: space-between; margin-bottom: 1rem; }
            .section-header h2 { margin: 0; font-size: 1rem; color: var(--text-primary); }
            .silicon-grid {
                display: grid;
                grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
                gap: 1rem;
            }
            .silicon-page .action-card {
                display: block;
                background: linear-gradient(180deg, rgba(30,41,59,0.95), rgba(15,23,42,0.95));
                border: 1px solid var(--border-color, var(--border));
                border-radius: 10px;
                padding: 1.1rem;
                text-decoration: none;
                color: var(--text-primary);
                transition: transform 0.12s ease, border-color 0.12s ease;
            }
            .silicon-page .action-card:hover { transform: translateY(-1px); border-color: rgba(59,130,246,0.55); }
            .silicon-page .action-card h3 { margin: 0.4rem 0 0.25rem; font-size: 1rem; }
            .silicon-page .action-card p { margin: 0; color: var(--text-secondary); font-size: 0.85rem; }
            .silicon-page .action-kicker { color: var(--text-muted); font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.08em; }
            .silicon-table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
            .silicon-table th, .silicon-table td {
                text-align: left;
                padding: 0.55rem 0.6rem;
                border-bottom: 1px solid var(--border-color, var(--border));
            }
            .silicon-table th { color: var(--text-muted); font-weight: 600; font-size: 0.78rem; text-transform: uppercase; letter-spacing: 0.06em; }
            .silicon-table tr:last-child td { border-bottom: 0; }
            .silicon-table a { color: var(--accent, #3b82f6); text-decoration: none; }
        "#}</style>
    }
}
