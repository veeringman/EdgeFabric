use leptos::prelude::*;

#[derive(Clone, Copy)]
struct Check {
    name: &'static str,
    detail: &'static str,
    status: &'static str, // "pass" | "warn" | "fail" | "pending"
}

const CHECKS: &[Check] = &[
    Check { name: "DRC",        detail: "KLayout · sky130A · 0 errors, 0 waivers",   status: "pass" },
    Check { name: "LVS",        detail: "Netgen · clean against extracted netlist",  status: "pass" },
    Check { name: "Antenna",    detail: "Magic antenna · 0 violations",              status: "pass" },
    Check { name: "ERC",        detail: "Open nets · 0 · floating gates · 0",        status: "pass" },
    Check { name: "STA · TT",   detail: "Typical corner · WNS +0.18 ns",             status: "pass" },
    Check { name: "STA · SS",   detail: "Slow corner · WNS +0.04 ns (tight)",        status: "warn" },
    Check { name: "STA · FF",   detail: "Fast corner · hold WNS +0.07 ns",           status: "pass" },
    Check { name: "Power",      detail: "IR drop avg 38 mV · peak 71 mV",            status: "warn" },
    Check { name: "Fill",       detail: "Density fill pending re-run",               status: "pending" },
];

#[derive(Clone, Copy)]
struct Artifact {
    name: &'static str,
    kind: &'static str,
    size: &'static str,
    checksum: &'static str,
}

const ARTIFACTS: &[Artifact] = &[
    Artifact { name: "chip_top.gds",          kind: "GDSII",   size: "184.2 MB", checksum: "sha256:a91f…3c2e" },
    Artifact { name: "chip_top.lef",          kind: "LEF",     size: "612 KB",   checksum: "sha256:7b04…9d11" },
    Artifact { name: "chip_top.def",          kind: "DEF",     size: "8.4 MB",   checksum: "sha256:c5d2…1f88" },
    Artifact { name: "chip_top.sdf",          kind: "SDF",     size: "22.1 MB",  checksum: "sha256:e002…b441" },
    Artifact { name: "chip_top.spef",         kind: "SPEF",    size: "14.7 MB",  checksum: "sha256:8aa9…0d70" },
    Artifact { name: "signoff_report.json",   kind: "Report",  size: "612 KB",   checksum: "sha256:14fe…cc09" },
    Artifact { name: "tapeout_manifest.toml", kind: "Manifest",size: "9 KB",     checksum: "sha256:0019…ab27" },
];

#[component]
pub fn SiliconTapeoutPage() -> impl IntoView {
    let passed = CHECKS.iter().filter(|c| c.status == "pass").count();
    let total  = CHECKS.len();
    let pct    = (passed as f64 / total as f64) * 100.0;
    let warns  = CHECKS.iter().filter(|c| c.status == "warn").count();
    let fails  = CHECKS.iter().filter(|c| c.status == "fail").count();
    let pending = CHECKS.iter().filter(|c| c.status == "pending").count();

    view! {
        <div class="page-container silicon-tapeout-page">
            <div class="page-header">
                <div>
                    <span class="section-kicker">"Chip Design · Sign-off"</span>
                    <h1>"Tape-out"</h1>
                    <p class="page-subtitle">"Sign-off gating, artifact pack, and shuttle submission."</p>
                </div>
                <div class="rtl-actions">
                    <button class="btn btn-ghost btn-sm">"Re-run Sign-off"</button>
                    <button class="btn btn-primary btn-sm" disabled=move || warns + fails + pending > 0>
                        "Submit to Shuttle"
                    </button>
                </div>
            </div>

            <section class="silicon-section">
                <div class="tapeout-summary">
                    <div class="tapeout-progress">
                        <div class="tapeout-progress-bar">
                            <div class="tapeout-progress-fill" style=format!("width: {:.1}%", pct)></div>
                        </div>
                        <span class="tapeout-progress-label">
                            {format!("{passed}/{total} checks passing")}
                        </span>
                    </div>
                    <div class="tapeout-counts">
                        <span class="count-chip pass">{format!("{} pass", passed)}</span>
                        <span class="count-chip warn">{format!("{} warn", warns)}</span>
                        <span class="count-chip fail">{format!("{} fail", fails)}</span>
                        <span class="count-chip pend">{format!("{} pending", pending)}</span>
                    </div>
                </div>
            </section>

            <section class="silicon-section">
                <div class="section-header">
                    <h2>"Sign-off Checklist"</h2>
                </div>
                <ul class="checklist">
                    {CHECKS.iter().map(|c| {
                        let c = *c;
                        let cls = format!("check check-{}", c.status);
                        let icon = match c.status {
                            "pass" => "✓",
                            "warn" => "!",
                            "fail" => "×",
                            _      => "…",
                        };
                        view! {
                            <li class=cls>
                                <span class="check-icon">{icon}</span>
                                <div>
                                    <strong>{c.name}</strong>
                                    <span class="check-detail">{c.detail}</span>
                                </div>
                            </li>
                        }
                    }).collect::<Vec<_>>()}
                </ul>
            </section>

            <section class="silicon-section">
                <div class="section-header">
                    <h2>"Artifacts"</h2>
                </div>
                <table class="qor-table">
                    <thead>
                        <tr>
                            <th>"Name"</th>
                            <th>"Kind"</th>
                            <th>"Size"</th>
                            <th>"Checksum"</th>
                            <th></th>
                        </tr>
                    </thead>
                    <tbody>
                        {ARTIFACTS.iter().map(|a| view! {
                            <tr>
                                <td><code>{a.name}</code></td>
                                <td><span class="badge active">{a.kind}</span></td>
                                <td>{a.size}</td>
                                <td><code class="art-checksum">{a.checksum}</code></td>
                                <td><a href="#" class="art-dl">"Download"</a></td>
                            </tr>
                        }).collect::<Vec<_>>()}
                    </tbody>
                </table>
            </section>
        </div>

        <style>{r#"
            .silicon-tapeout-page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
            .silicon-tapeout-page .page-header { display: flex; justify-content: space-between; align-items: flex-start; gap: 1rem; flex-wrap: wrap; }
            .silicon-tapeout-page .page-header h1 { margin: 0; }
            .silicon-tapeout-page .page-subtitle { margin: 0.35rem 0 0; color: var(--text-secondary); }
            .silicon-section { background: var(--bg-card); border: 1px solid var(--border); border-radius: 10px; padding: 1.25rem; }
            .section-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.85rem; }
            .section-header h2 { margin: 0; font-size: 1rem; color: var(--text-primary); }

            .tapeout-summary { display: flex; gap: 1rem; align-items: center; flex-wrap: wrap; }
            .tapeout-progress { flex: 1; min-width: 240px; }
            .tapeout-progress-bar { height: 10px; background: rgba(148,163,184,0.15); border-radius: 999px; overflow: hidden; }
            .tapeout-progress-fill { height: 100%; background: linear-gradient(90deg, #22c55e, #06b6d4); }
            .tapeout-progress-label { display: block; margin-top: 0.4rem; color: var(--text-secondary); font-size: 0.85rem; }
            .tapeout-counts { display: flex; gap: 0.45rem; flex-wrap: wrap; }
            .count-chip { padding: 0.3rem 0.65rem; border-radius: 999px; font-size: 0.78rem; font-weight: 600; }
            .count-chip.pass { background: rgba(34,197,94,0.15); color: var(--success); }
            .count-chip.warn { background: rgba(245,158,11,0.15); color: var(--warning); }
            .count-chip.fail { background: rgba(239,68,68,0.15); color: var(--danger); }
            .count-chip.pend { background: rgba(148,163,184,0.15); color: var(--text-secondary); }

            .checklist { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.5rem; }
            .check {
                display: grid;
                grid-template-columns: 28px 1fr;
                gap: 0.6rem;
                padding: 0.65rem 0.85rem;
                background: var(--bg-secondary);
                border: 1px solid var(--border);
                border-radius: 8px;
                align-items: center;
            }
            .check strong { color: var(--text-primary); display: block; }
            .check-detail { color: var(--text-secondary); font-size: 0.82rem; display: block; }
            .check-icon { width: 28px; height: 28px; display: inline-flex; align-items: center; justify-content: center; border-radius: 999px; font-weight: 700; }
            .check-pass { border-color: rgba(34,197,94,0.35); }
            .check-pass .check-icon { background: rgba(34,197,94,0.18); color: var(--success); }
            .check-warn { border-color: rgba(245,158,11,0.35); }
            .check-warn .check-icon { background: rgba(245,158,11,0.18); color: var(--warning); }
            .check-fail { border-color: rgba(239,68,68,0.4); }
            .check-fail .check-icon { background: rgba(239,68,68,0.18); color: var(--danger); }
            .check-pending .check-icon { background: rgba(148,163,184,0.15); color: var(--text-secondary); }

            .qor-table { width: 100%; border-collapse: collapse; font-size: 0.88rem; }
            .qor-table th, .qor-table td { padding: 0.55rem 0.6rem; border-bottom: 1px solid var(--border); text-align: left; }
            .qor-table th { color: var(--text-muted); font-weight: 600; font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.06em; }
            .qor-table tr:last-child td { border-bottom: 0; }
            .art-checksum { color: var(--text-muted); font-size: 0.78rem; }
            .art-dl { color: var(--accent); text-decoration: none; }
        "#}</style>
    }
}
