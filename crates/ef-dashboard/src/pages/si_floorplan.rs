use leptos::prelude::*;

#[derive(Clone, Copy)]
struct Macro {
    name: &'static str,
    kind: &'static str, // "core" | "sram" | "pll" | "io"
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

const MACROS: &[Macro] = &[
    Macro { name: "CPU",       kind: "core", x: 80.0,  y: 80.0,  w: 220.0, h: 180.0 },
    Macro { name: "L1 I$",     kind: "sram", x: 320.0, y: 80.0,  w: 100.0, h: 80.0 },
    Macro { name: "L1 D$",     kind: "sram", x: 320.0, y: 180.0, w: 100.0, h: 80.0 },
    Macro { name: "L2",        kind: "sram", x: 440.0, y: 80.0,  w: 140.0, h: 180.0 },
    Macro { name: "PLL",       kind: "pll",  x: 80.0,  y: 280.0, w: 80.0,  h: 60.0 },
    Macro { name: "UART",      kind: "core", x: 180.0, y: 280.0, w: 90.0,  h: 60.0 },
    Macro { name: "SPI",       kind: "core", x: 280.0, y: 280.0, w: 90.0,  h: 60.0 },
    Macro { name: "GPIO",      kind: "core", x: 380.0, y: 280.0, w: 90.0,  h: 60.0 },
    Macro { name: "DMA",       kind: "core", x: 480.0, y: 280.0, w: 100.0, h: 60.0 },
];

#[component]
pub fn SiliconFloorplanPage() -> impl IntoView {
    let show_congestion = RwSignal::new(true);
    let show_grid = RwSignal::new(true);
    let selected: RwSignal<Option<&'static str>> = RwSignal::new(Some("CPU"));

    // Pseudo-congestion heatmap cells (16x10)
    let cells: Vec<(u32, u32, f64)> = (0..10).flat_map(|r| {
        (0..16).map(move |c| {
            let cx = c as f64;
            let cy = r as f64;
            // Hotspot near CPU<->L2 path
            let d = ((cx - 7.0).powi(2) + (cy - 3.0).powi(2)).sqrt();
            let v = (1.0 - (d / 9.0)).clamp(0.0, 1.0);
            (c, r, v)
        })
    }).collect();

    view! {
        <div class="page-container silicon-fp-page">
            <div class="page-header">
                <div>
                    <span class="section-kicker">"Chip Design · Layout"</span>
                    <h1>"Floorplan & PnR"</h1>
                    <p class="page-subtitle">"Macro placement, pin assignment, and congestion review."</p>
                </div>
                <div class="fp-toolbar">
                    <label class="fp-toggle">
                        <input type="checkbox" prop:checked=move || show_grid.get()
                            on:change=move |ev| show_grid.set(event_target_checked(&ev))/>
                        <span>"Grid"</span>
                    </label>
                    <label class="fp-toggle">
                        <input type="checkbox" prop:checked=move || show_congestion.get()
                            on:change=move |ev| show_congestion.set(event_target_checked(&ev))/>
                        <span>"Congestion"</span>
                    </label>
                    <button class="btn btn-ghost btn-sm">"Run PnR"</button>
                    <button class="btn btn-primary btn-sm">"Save Floorplan"</button>
                </div>
            </div>

            <div class="fp-layout">
                <section class="fp-canvas-wrap">
                    <svg viewBox="0 0 660 380" class="fp-svg" preserveAspectRatio="xMidYMid meet">
                        // die outline
                        <rect x="20" y="20" width="620" height="340" rx="6"
                              class="fp-die"/>

                        // grid
                        {move || show_grid.get().then(|| {
                            let mut lines = Vec::new();
                            for c in 0..=16u32 {
                                let x = 20.0_f64 + c as f64 * 38.75;
                                lines.push(view! {
                                    <line x1=x y1=20.0_f64 x2=x y2=360.0_f64 class="fp-grid"/>
                                });
                            }
                            for r in 0..=10u32 {
                                let y = 20.0_f64 + r as f64 * 34.0;
                                lines.push(view! {
                                    <line x1=20.0_f64 y1=y x2=640.0_f64 y2=y class="fp-grid"/>
                                });
                            }
                            lines
                        })}

                        // congestion heatmap
                        {move || show_congestion.get().then(|| {
                            cells.iter().map(|(c, r, v)| {
                                let v = *v;
                                let x = 20.0 + (*c as f64) * 38.75;
                                let y = 20.0 + (*r as f64) * 34.0;
                                let opacity = (v * 0.55).max(0.0);
                                view! {
                                    <rect x=x y=y width="38.75" height="34"
                                        fill={if v > 0.66 { "#ef4444" } else if v > 0.33 { "#f59e0b" } else { "#22c55e" }}
                                        opacity=opacity/>
                                }
                            }).collect::<Vec<_>>()
                        })}

                        // macros
                        {MACROS.iter().map(|m| {
                            let m = *m;
                            let is_sel = {
                                let selected = selected;
                                move || selected.get() == Some(m.name)
                            };
                            view! {
                                <g class="fp-macro" class:selected=is_sel
                                   on:click=move |_| selected.set(Some(m.name))>
                                    <rect x=m.x y=m.y width=m.w height=m.h rx="4"
                                          class={format!("fp-mac fp-mac--{}", m.kind)}/>
                                    <text x={m.x + m.w/2.0} y={m.y + m.h/2.0 + 4.0}
                                          text-anchor="middle" class="fp-mac-label">{m.name}</text>
                                </g>
                            }
                        }).collect::<Vec<_>>()}

                        // pin ring (sample IO pads)
                        {(0..16).map(|i| {
                            let x = 20.0 + (i as f64 + 0.5) * 38.75;
                            view! { <rect x={x - 4.0} y="14" width="8" height="6" class="fp-pad"/> }
                        }).collect::<Vec<_>>()}
                        {(0..16).map(|i| {
                            let x = 20.0 + (i as f64 + 0.5) * 38.75;
                            view! { <rect x={x - 4.0} y="360" width="8" height="6" class="fp-pad"/> }
                        }).collect::<Vec<_>>()}
                    </svg>

                    <div class="fp-legend">
                        <span class="legend-item"><span class="dot dot-core"></span>"Core"</span>
                        <span class="legend-item"><span class="dot dot-sram"></span>"SRAM"</span>
                        <span class="legend-item"><span class="dot dot-pll"></span>"PLL"</span>
                        <span class="legend-item"><span class="dot dot-cong"></span>"Congestion"</span>
                    </div>
                </section>

                <aside class="fp-inspector">
                    <div class="fp-inspector-header">"Macro Inspector"</div>
                    {move || match selected.get() {
                        Some(name) => {
                            let m = MACROS.iter().find(|m| m.name == name).copied();
                            match m {
                                Some(m) => view! {
                                    <div class="fp-props">
                                        <div class="prop"><span>"Name"</span><strong>{m.name}</strong></div>
                                        <div class="prop"><span>"Kind"</span><strong>{m.kind}</strong></div>
                                        <div class="prop"><span>"Origin"</span><strong>{format!("({:.0}, {:.0}) µm", m.x, m.y)}</strong></div>
                                        <div class="prop"><span>"Size"</span><strong>{format!("{:.0} × {:.0} µm", m.w, m.h)}</strong></div>
                                        <div class="prop"><span>"Utilization"</span><strong>"68%"</strong></div>
                                        <div class="prop"><span>"Halo"</span><strong>"4 µm"</strong></div>
                                    </div>
                                }.into_any(),
                                None => view! { <p class="empty-state">"Macro not found"</p> }.into_any(),
                            }
                        }
                        None => view! { <p class="empty-state">"Click a macro to inspect"</p> }.into_any(),
                    }}

                    <div class="fp-inspector-header">"Die Summary"</div>
                    <div class="fp-props">
                        <div class="prop"><span>"Die Size"</span><strong>"1.20 × 0.80 mm"</strong></div>
                        <div class="prop"><span>"PDK"</span><strong>"sky130A"</strong></div>
                        <div class="prop"><span>"Macros"</span><strong>"9"</strong></div>
                        <div class="prop"><span>"Std-cell Util."</span><strong>"71%"</strong></div>
                        <div class="prop"><span>"Peak Congestion"</span><strong class="text-warn">"82%"</strong></div>
                        <div class="prop"><span>"DRC Errors"</span><strong class="text-ok">"0"</strong></div>
                    </div>
                </aside>
            </div>
        </div>

        <style>{r#"
            .silicon-fp-page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
            .silicon-fp-page .page-header { display: flex; justify-content: space-between; align-items: flex-start; gap: 1rem; flex-wrap: wrap; }
            .silicon-fp-page .page-header h1 { margin: 0; }
            .silicon-fp-page .page-subtitle { margin: 0.35rem 0 0; color: var(--text-secondary); }
            .fp-toolbar { display: flex; gap: 0.6rem; align-items: center; flex-wrap: wrap; }
            .fp-toggle { display: inline-flex; gap: 0.35rem; align-items: center; color: var(--text-secondary); font-size: 0.85rem; }
            .fp-layout { display: grid; grid-template-columns: minmax(0,1fr) 280px; gap: 0.75rem; }
            .fp-canvas-wrap, .fp-inspector {
                background: var(--bg-card);
                border: 1px solid var(--border);
                border-radius: 10px;
                padding: 1rem;
            }
            .fp-svg { width: 100%; height: auto; display: block; background: #0b1220; border-radius: 6px; }
            .fp-die { fill: #0f172a; stroke: #475569; stroke-width: 1.5; }
            .fp-grid { stroke: rgba(148,163,184,0.12); stroke-width: 0.5; }
            .fp-mac { stroke-width: 1.2; }
            .fp-mac--core { fill: rgba(59,130,246,0.18); stroke: #3b82f6; }
            .fp-mac--sram { fill: rgba(34,197,94,0.18); stroke: #22c55e; }
            .fp-mac--pll  { fill: rgba(168,85,247,0.22); stroke: #a855f7; }
            .fp-mac--io   { fill: rgba(245,158,11,0.22); stroke: #f59e0b; }
            .fp-macro { cursor: pointer; }
            .fp-macro.selected .fp-mac { stroke: #f1f5f9; stroke-width: 2.2; filter: drop-shadow(0 0 4px rgba(241,245,249,0.4)); }
            .fp-mac-label { fill: #e2e8f0; font-size: 11px; font-family: var(--font-mono); pointer-events: none; }
            .fp-pad { fill: #f59e0b; opacity: 0.85; }
            .fp-legend { display: flex; gap: 1rem; margin-top: 0.6rem; flex-wrap: wrap; color: var(--text-secondary); font-size: 0.78rem; }
            .legend-item { display: inline-flex; gap: 0.35rem; align-items: center; }
            .dot { width: 10px; height: 10px; border-radius: 2px; display: inline-block; }
            .dot-core { background: #3b82f6; }
            .dot-sram { background: #22c55e; }
            .dot-pll  { background: #a855f7; }
            .dot-cong { background: linear-gradient(90deg,#22c55e,#f59e0b,#ef4444); }
            .fp-inspector-header { font-size: 0.75rem; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.08em; margin: 0.4rem 0 0.5rem; }
            .fp-props { display: grid; gap: 0.35rem; }
            .prop { display: flex; justify-content: space-between; gap: 0.5rem; font-size: 0.85rem; }
            .prop span { color: var(--text-muted); }
            .prop strong { color: var(--text-primary); font-weight: 600; }
            .text-warn { color: var(--warning); }
            .text-ok { color: var(--success); }
            @media (max-width: 1024px) { .fp-layout { grid-template-columns: 1fr; } }
        "#}</style>
    }
}
