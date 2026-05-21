use leptos::prelude::*;

#[derive(Clone, Copy)]
struct QorRow {
    run: &'static str,
    when: &'static str,
    wns_ns: f64,
    tns_ns: f64,
    area_um2: u32,
    cells: u32,
    power_mw: f64,
    status: &'static str, // "passed" | "running" | "failed"
}

const RUNS: &[QorRow] = &[
    QorRow { run: "synth-042", when: "5m ago",  wns_ns: 0.18,  tns_ns: 0.0,  area_um2: 184_320, cells: 12_410, power_mw: 8.4,  status: "passed" },
    QorRow { run: "synth-041", when: "1h ago",  wns_ns: -0.07, tns_ns: -1.2, area_um2: 181_900, cells: 12_280, power_mw: 8.2,  status: "failed" },
    QorRow { run: "synth-040", when: "3h ago",  wns_ns: 0.05,  tns_ns: 0.0,  area_um2: 178_200, cells: 12_010, power_mw: 8.1,  status: "passed" },
    QorRow { run: "synth-039", when: "yesterday", wns_ns: 0.21, tns_ns: 0.0, area_um2: 176_800, cells: 11_980, power_mw: 8.0,  status: "passed" },
    QorRow { run: "synth-038", when: "2d ago",  wns_ns: 0.34,  tns_ns: 0.0,  area_um2: 175_400, cells: 11_900, power_mw: 7.9,  status: "passed" },
];

#[component]
pub fn SiliconSynthPage() -> impl IntoView {
    let max_area = RUNS.iter().map(|r| r.area_um2).max().unwrap_or(1) as f64;
    let max_power = RUNS.iter().map(|r| r.power_mw).fold(0.0_f64, f64::max);

    view! {
        <div class="page-container silicon-synth-page">
            <div class="page-header">
                <div>
                    <span class="section-kicker">"Chip Design · Build"</span>
                    <h1>"Synthesis & QoR"</h1>
                    <p class="page-subtitle">"Compare timing, area, and power across recent synthesis runs."</p>
                </div>
                <div class="rtl-actions">
                    <button class="btn btn-ghost btn-sm">"Open Baseline"</button>
                    <button class="btn btn-primary btn-sm">"Launch Run"</button>
                </div>
            </div>

            <div class="qor-stats">
                <div class="qor-stat">
                    <span class="qor-stat-label">"Best WNS"</span>
                    <span class="qor-stat-value text-ok">"+0.34 ns"</span>
                </div>
                <div class="qor-stat">
                    <span class="qor-stat-label">"Latest WNS"</span>
                    <span class="qor-stat-value text-ok">"+0.18 ns"</span>
                </div>
                <div class="qor-stat">
                    <span class="qor-stat-label">"Area Δ vs baseline"</span>
                    <span class="qor-stat-value text-warn">"+5.1%"</span>
                </div>
                <div class="qor-stat">
                    <span class="qor-stat-label">"Power Δ vs baseline"</span>
                    <span class="qor-stat-value text-warn">"+6.3%"</span>
                </div>
            </div>

            <section class="silicon-section">
                <div class="section-header">
                    <h2>"Run History"</h2>
                </div>
                <table class="qor-table">
                    <thead>
                        <tr>
                            <th>"Run"</th>
                            <th>"When"</th>
                            <th>"WNS"</th>
                            <th>"TNS"</th>
                            <th>"Area"</th>
                            <th>"Cells"</th>
                            <th>"Power"</th>
                            <th>"Status"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {RUNS.iter().map(|r| {
                            let r = *r;
                            let area_pct = (r.area_um2 as f64 / max_area) * 100.0;
                            let power_pct = (r.power_mw / max_power) * 100.0;
                            let wns_class = if r.wns_ns >= 0.0 { "text-ok" } else { "text-bad" };
                            let badge_class = match r.status {
                                "passed" => "badge passed",
                                "failed" => "badge failed",
                                _ => "badge running",
                            };
                            view! {
                                <tr>
                                    <td><code>{r.run}</code></td>
                                    <td>{r.when}</td>
                                    <td class=wns_class>{format!("{:+.2} ns", r.wns_ns)}</td>
                                    <td>{format!("{:.1} ns", r.tns_ns)}</td>
                                    <td>
                                        <div class="qor-bar-wrap">
                                            <div class="qor-bar"><div class="qor-bar-fill" style=format!("width: {:.1}%; background: #3b82f6;", area_pct)></div></div>
                                            <span>{format!("{} µm²", format_with_commas(r.area_um2))}</span>
                                        </div>
                                    </td>
                                    <td>{format!("{}", format_with_commas(r.cells))}</td>
                                    <td>
                                        <div class="qor-bar-wrap">
                                            <div class="qor-bar"><div class="qor-bar-fill" style=format!("width: {:.1}%; background: #f59e0b;", power_pct)></div></div>
                                            <span>{format!("{:.1} mW", r.power_mw)}</span>
                                        </div>
                                    </td>
                                    <td><span class=badge_class>{r.status}</span></td>
                                </tr>
                            }
                        }).collect::<Vec<_>>()}
                    </tbody>
                </table>
            </section>

            <section class="silicon-section">
                <div class="section-header">
                    <h2>"Top Critical Paths · synth-042"</h2>
                </div>
                <table class="qor-table">
                    <thead>
                        <tr>
                            <th>"#"</th>
                            <th>"Startpoint"</th>
                            <th>"Endpoint"</th>
                            <th>"Slack"</th>
                            <th>"Stages"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr><td>"1"</td><td><code>"cpu/regfile/q_reg[5]"</code></td><td><code>"cpu/alu/y_reg[31]"</code></td><td class="text-ok">"+0.18 ns"</td><td>"14"</td></tr>
                        <tr><td>"2"</td><td><code>"cpu/decode/op_reg[3]"</code></td><td><code>"cpu/csr/mstatus_reg[7]"</code></td><td class="text-ok">"+0.22 ns"</td><td>"11"</td></tr>
                        <tr><td>"3"</td><td><code>"axi/arvalid_reg"</code></td><td><code>"axi/state_reg[2]"</code></td><td class="text-ok">"+0.41 ns"</td><td>"9"</td></tr>
                        <tr><td>"4"</td><td><code>"uart/tx_shift_reg[7]"</code></td><td><code>"uart/tx_pin_reg"</code></td><td class="text-ok">"+0.62 ns"</td><td>"6"</td></tr>
                    </tbody>
                </table>
            </section>
        </div>

        <style>{r#"
            .silicon-synth-page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
            .silicon-synth-page .page-header { display: flex; justify-content: space-between; align-items: flex-start; gap: 1rem; flex-wrap: wrap; }
            .silicon-synth-page .page-header h1 { margin: 0; }
            .silicon-synth-page .page-subtitle { margin: 0.35rem 0 0; color: var(--text-secondary); }
            .silicon-section { background: var(--bg-card); border: 1px solid var(--border); border-radius: 10px; padding: 1.25rem; }
            .section-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.85rem; }
            .section-header h2 { margin: 0; font-size: 1rem; color: var(--text-primary); }
            .qor-stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 0.75rem; }
            .qor-stat { background: var(--bg-card); border: 1px solid var(--border); border-radius: 10px; padding: 0.9rem 1rem; }
            .qor-stat-label { display: block; font-size: 0.72rem; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.06em; }
            .qor-stat-value { display: block; font-size: 1.35rem; font-weight: 700; margin-top: 0.2rem; }
            .qor-table { width: 100%; border-collapse: collapse; font-size: 0.88rem; }
            .qor-table th, .qor-table td { padding: 0.55rem 0.6rem; border-bottom: 1px solid var(--border); text-align: left; }
            .qor-table th { color: var(--text-muted); font-weight: 600; font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.06em; }
            .qor-table tr:last-child td { border-bottom: 0; }
            .qor-bar-wrap { display: flex; align-items: center; gap: 0.55rem; }
            .qor-bar { width: 90px; height: 6px; background: rgba(148,163,184,0.18); border-radius: 999px; overflow: hidden; }
            .qor-bar-fill { height: 100%; border-radius: 999px; }
            .text-ok { color: var(--success); }
            .text-warn { color: var(--warning); }
            .text-bad { color: var(--danger); }
        "#}</style>
    }
}

fn format_with_commas(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { out.push(','); }
        out.push(ch);
    }
    out.chars().rev().collect()
}
