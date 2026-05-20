use leptos::prelude::*;
use leptos::html;
use wasm_bindgen::JsCast;

/// One placed footprint on the board.
#[derive(Clone, Debug)]
struct Part {
    id: u32,
    reference: String,
    name: String,
    shape: &'static str, // "res" | "cap" | "ic-sop" | "ic-qfp" | "conn" | "mcu" | "led"
    /// Top-left position inside the canvas, in canvas pixels.
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    rotation: i32, // degrees, multiples of 90
    layer: &'static str, // "F.Cu" | "B.Cu"
}

/// Palette entry — defines what gets dropped onto the board when a
/// palette item is clicked.
#[derive(Clone, Copy)]
struct PaletteEntry {
    label: &'static str,
    prefix: &'static str,
    shape: &'static str,
    width: f64,
    height: f64,
    category: &'static str,
}

const PALETTE: &[PaletteEntry] = &[
    PaletteEntry { label: "Resistor 0603",   prefix: "R", shape: "res",   width: 56.0,  height: 22.0,  category: "Passives" },
    PaletteEntry { label: "Capacitor 0603",  prefix: "C", shape: "cap",   width: 32.0,  height: 32.0,  category: "Passives" },
    PaletteEntry { label: "LED 0805",        prefix: "D", shape: "led",   width: 36.0,  height: 28.0,  category: "Passives" },
    PaletteEntry { label: "Op-amp SOIC-8",   prefix: "U", shape: "ic-sop", width: 110.0, height: 70.0,  category: "ICs" },
    PaletteEntry { label: "MCU LQFP-48",     prefix: "U", shape: "ic-qfp", width: 130.0, height: 130.0, category: "ICs" },
    PaletteEntry { label: "SoC BGA",         prefix: "U", shape: "mcu",    width: 160.0, height: 160.0, category: "ICs" },
    PaletteEntry { label: "USB-C Connector", prefix: "J", shape: "conn",   width: 96.0,  height: 44.0,  category: "Connectors" },
    PaletteEntry { label: "Pin Header 1x6",  prefix: "J", shape: "conn",   width: 90.0,  height: 30.0,  category: "Connectors" },
];

const LAYERS: &[(&str, &str)] = &[
    ("F.Cu",      "#ff5d4d"),
    ("B.Cu",      "#3aa8ff"),
    ("F.Silk",    "#f1f1f1"),
    ("B.Silk",    "#9aa3b2"),
    ("Edge.Cuts", "#ffd24d"),
];

#[derive(Clone, Copy)]
struct DragState {
    id: u32,
    offset_x: f64,
    offset_y: f64,
}

/// PCB Designer — interactive layout surface.
///
/// First cut focuses on the studio chrome and direct-manipulation
/// placement. Push-and-shove routing, 3D, and full DRC arrive in later
/// phases.
#[component]
pub fn HardwarePcbPage() -> impl IntoView {
    // Seed the canvas with a few parts so the board looks alive on first load.
    let initial: Vec<Part> = vec![
        Part { id: 1, reference: "U1".into(), name: "SoC BGA".into(),  shape: "mcu",    x: 280.0, y: 180.0, width: 160.0, height: 160.0, rotation: 0, layer: "F.Cu" },
        Part { id: 2, reference: "U2".into(), name: "MCU LQFP".into(), shape: "ic-qfp", x: 90.0,  y: 220.0, width: 130.0, height: 130.0, rotation: 0, layer: "F.Cu" },
        Part { id: 3, reference: "U3".into(), name: "Op-amp".into(),   shape: "ic-sop", x: 510.0, y: 90.0,  width: 110.0, height: 70.0,  rotation: 0, layer: "F.Cu" },
        Part { id: 4, reference: "R1".into(), name: "Resistor".into(), shape: "res",    x: 510.0, y: 200.0, width: 56.0,  height: 22.0,  rotation: 0, layer: "F.Cu" },
        Part { id: 5, reference: "R2".into(), name: "Resistor".into(), shape: "res",    x: 510.0, y: 240.0, width: 56.0,  height: 22.0,  rotation: 0, layer: "F.Cu" },
        Part { id: 6, reference: "C1".into(), name: "Capacitor".into(),shape: "cap",    x: 600.0, y: 200.0, width: 32.0,  height: 32.0,  rotation: 0, layer: "F.Cu" },
        Part { id: 7, reference: "D1".into(), name: "LED".into(),      shape: "led",    x: 600.0, y: 260.0, width: 36.0,  height: 28.0,  rotation: 0, layer: "F.Cu" },
        Part { id: 8, reference: "J1".into(), name: "USB-C".into(),    shape: "conn",   x: 320.0, y: 400.0, width: 96.0,  height: 44.0,  rotation: 0, layer: "F.Cu" },
    ];

    let parts: RwSignal<Vec<Part>> = RwSignal::new(initial);
    let next_id: RwSignal<u32> = RwSignal::new(100);
    let selected: RwSignal<Option<u32>> = RwSignal::new(None);
    let drag: RwSignal<Option<DragState>> = RwSignal::new(None);

    let active_layer: RwSignal<&'static str> = RwSignal::new("F.Cu");
    let layer_visibility: RwSignal<Vec<bool>> = RwSignal::new(vec![true; LAYERS.len()]);

    let zoom: RwSignal<f64> = RwSignal::new(1.0);
    let view_3d: RwSignal<bool> = RwSignal::new(false);
    let drc_status: RwSignal<&'static str> = RwSignal::new("ok");
    let cursor_mm: RwSignal<(f64, f64)> = RwSignal::new((0.0, 0.0));
    let search: RwSignal<String> = RwSignal::new(String::new());
    let copilot_prompt: RwSignal<String> = RwSignal::new(String::new());

    let canvas_ref: NodeRef<html::Div> = NodeRef::new();

    // Helper closures
    let add_from_palette = move |entry: PaletteEntry| {
        let id = next_id.get();
        next_id.set(id + 1);

        // Determine next reference number per prefix.
        let count = parts
            .with(|ps| ps.iter().filter(|p| p.reference.starts_with(entry.prefix)).count())
            + 1;

        let new_part = Part {
            id,
            reference: format!("{}{}", entry.prefix, count),
            name: entry.label.to_string(),
            shape: entry.shape,
            x: 380.0 - entry.width / 2.0,
            y: 240.0 - entry.height / 2.0,
            width: entry.width,
            height: entry.height,
            rotation: 0,
            layer: active_layer.get(),
        };
        parts.update(|ps| ps.push(new_part));
        selected.set(Some(id));
    };

    let delete_selected = move |_| {
        if let Some(id) = selected.get() {
            parts.update(|ps| ps.retain(|p| p.id != id));
            selected.set(None);
        }
    };

    let rotate_selected = move |_| {
        if let Some(id) = selected.get() {
            parts.update(|ps| {
                if let Some(p) = ps.iter_mut().find(|p| p.id == id) {
                    p.rotation = (p.rotation + 90) % 360;
                }
            });
        }
    };

    let run_drc = move |_| {
        drc_status.set("running");
        // Cheap fake — flip to OK on next tick.
        let _ = leptos::prelude::set_timeout(
            move || drc_status.set("ok"),
            std::time::Duration::from_millis(450),
        );
    };

    let zoom_in = move |_| zoom.update(|z| *z = (*z * 1.15).min(2.5));
    let zoom_out = move |_| zoom.update(|z| *z = (*z / 1.15).max(0.4));
    let zoom_fit = move |_| zoom.set(1.0);

    let toggle_3d = move |_| view_3d.update(|v| *v = !*v);

    // Canvas pointer handlers
    let on_canvas_move = move |ev: web_sys::PointerEvent| {
        if let Some(el) = canvas_ref.get() {
            let rect = el.get_bounding_client_rect();
            let cx = ev.client_x() as f64 - rect.left();
            let cy = ev.client_y() as f64 - rect.top();
            // Treat 4px = 1mm for the on-board ruler readout.
            cursor_mm.set((cx / 4.0, cy / 4.0));

            if let Some(ds) = drag.get() {
                parts.update(|ps| {
                    if let Some(p) = ps.iter_mut().find(|p| p.id == ds.id) {
                        // Snap to 4px grid.
                        let nx = ((cx - ds.offset_x) / 4.0).round() * 4.0;
                        let ny = ((cy - ds.offset_y) / 4.0).round() * 4.0;
                        p.x = nx;
                        p.y = ny;
                    }
                });
            }
        }
    };

    let on_canvas_up = move |_ev: web_sys::PointerEvent| {
        drag.set(None);
    };

    let on_canvas_down_bg = move |_ev: web_sys::PointerEvent| {
        // Click empty canvas clears selection.
        selected.set(None);
    };

    // Categories for the palette accordion.
    let categories = ["Passives", "ICs", "Connectors"];

    let palette_items_for = move |cat: &'static str| -> Vec<PaletteEntry> {
        let q = search.get().to_lowercase();
        PALETTE
            .iter()
            .filter(|p| p.category == cat)
            .filter(|p| q.is_empty() || p.label.to_lowercase().contains(&q))
            .copied()
            .collect()
    };

    view! {
        <div
            class="hw-studio-shell"
            class:hw-3d=move || view_3d.get()
        >
            // Toolbar
            <div class="hw-toolbar">
                <div class="hw-toolbar-group">
                    <span class="hw-crumb">"Hardware Studio"</span>
                    <span class="hw-crumb-sep">"/"</span>
                    <span class="hw-crumb hw-crumb--active">"PCB Designer"</span>
                </div>
                <div class="hw-toolbar-group">
                    <button class="hw-btn" on:click=zoom_out title="Zoom out">"−"</button>
                    <span class="hw-zoom">{move || format!("{:>3.0}%", zoom.get() * 100.0)}</span>
                    <button class="hw-btn" on:click=zoom_in title="Zoom in">"+"</button>
                    <button class="hw-btn" on:click=zoom_fit title="Fit to view">"Fit"</button>
                    <span class="hw-divider"></span>
                    <button class="hw-btn" on:click=rotate_selected title="Rotate 90°">"⟳"</button>
                    <button class="hw-btn hw-btn--danger" on:click=delete_selected title="Delete selection">"Delete"</button>
                    <span class="hw-divider"></span>
                    <button class="hw-btn" on:click=run_drc>"Run DRC"</button>
                    <button class="hw-btn">"Autoroute"</button>
                    <button class="hw-btn" on:click=toggle_3d>
                        {move || if view_3d.get() { "2D View" } else { "3D View" }}
                    </button>
                    <button class="hw-btn hw-btn--primary">"Export Gerber"</button>
                </div>
            </div>

            <div class="hw-workspace">
                // Left: parts palette
                <aside class="hw-panel hw-panel--left">
                    <div class="hw-panel-header">
                        <h3>"Parts"</h3>
                    </div>
                    <input
                        class="hw-search"
                        type="search"
                        placeholder="Search parts…"
                        prop:value=move || search.get()
                        on:input=move |ev| search.set(event_target_value(&ev))
                    />
                    {categories.iter().map(|cat| {
                        let cat = *cat;
                        view! {
                            <div class="hw-cat">
                                <div class="hw-cat-title">{cat}</div>
                                <div class="hw-cat-items">
                                    {move || palette_items_for(cat).into_iter().map(|entry| {
                                        view! {
                                            <button
                                                class="hw-part-chip"
                                                on:click=move |_| add_from_palette(entry)
                                                title=entry.label
                                            >
                                                <span class={format!("hw-part-glyph hw-glyph--{}", entry.shape)}></span>
                                                <span class="hw-part-label">{entry.label}</span>
                                            </button>
                                        }
                                    }).collect_view()}
                                </div>
                            </div>
                        }
                    }).collect_view()}
                </aside>

                // Center: canvas
                <main class="hw-canvas-wrap">
                    <div
                        class="hw-canvas"
                        node_ref=canvas_ref
                        on:pointermove=on_canvas_move
                        on:pointerup=on_canvas_up
                        on:pointerleave=on_canvas_up
                        on:pointerdown=on_canvas_down_bg
                        style:transform=move || format!(
                            "scale({}) {}",
                            zoom.get(),
                            if view_3d.get() { "rotateX(45deg) rotateZ(-12deg)" } else { "" }
                        )
                    >
                        // Board outline (Edge.Cuts) — only when layer visible.
                        <div
                            class="hw-board-outline"
                            class:hidden=move || !layer_visibility.with(|v| v.get(4).copied().unwrap_or(true))
                        ></div>

                        // Parts layer
                        <For
                            each=move || parts.get()
                            key=|p| p.id
                            children=move |p: Part| {
                                let part_id = p.id;
                                let on_down = move |ev: web_sys::PointerEvent| {
                                    ev.stop_propagation();
                                    if let Some(el) = canvas_ref.get() {
                                        let rect = el.get_bounding_client_rect();
                                        let cx = ev.client_x() as f64 - rect.left();
                                        let cy = ev.client_y() as f64 - rect.top();
                                        // Look up current part position to compute grab offset.
                                        if let Some(cur) = parts.with(|ps| ps.iter().find(|q| q.id == part_id).cloned()) {
                                            drag.set(Some(DragState {
                                                id: part_id,
                                                offset_x: cx - cur.x,
                                                offset_y: cy - cur.y,
                                            }));
                                        }
                                        selected.set(Some(part_id));

                                        // Capture pointer to the canvas so move events keep firing.
                                        if let Some(target) = ev.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                                            let _ = target.set_pointer_capture(ev.pointer_id());
                                        }
                                    }
                                };

                                let is_selected = move || selected.get() == Some(part_id);

                                view! {
                                    <div
                                        class={format!("hw-part hw-part--{}", p.shape)}
                                        class:selected=is_selected
                                        style:left=format!("{}px", p.x)
                                        style:top=format!("{}px", p.y)
                                        style:width=format!("{}px", p.width)
                                        style:height=format!("{}px", p.height)
                                        style:transform=format!("rotate({}deg)", p.rotation)
                                        on:pointerdown=on_down
                                    >
                                        <span class="hw-part-ref">{p.reference.clone()}</span>
                                    </div>
                                }
                            }
                        />
                    </div>

                    // Status bar
                    <div class="hw-statusbar">
                        <span class="hw-status-item">
                            {move || {
                                let (mx, my) = cursor_mm.get();
                                format!("X: {:>6.2} mm   Y: {:>6.2} mm", mx, my)
                            }}
                        </span>
                        <span class="hw-status-item">{move || format!("Grid: 1.00 mm")}</span>
                        <span class="hw-status-item">
                            "Active: " {move || active_layer.get()}
                        </span>
                        <span
                            class="hw-status-item hw-drc"
                            class:hw-drc--ok=move || drc_status.get() == "ok"
                            class:hw-drc--run=move || drc_status.get() == "running"
                        >
                            {move || match drc_status.get() {
                                "running" => "DRC: running…",
                                _ => "DRC: 0 errors",
                            }}
                        </span>
                    </div>
                </main>

                // Right: layers + properties + AI copilot
                <aside class="hw-panel hw-panel--right">
                    <div class="hw-panel-header"><h3>"Layers"</h3></div>
                    <div class="hw-layers">
                        {LAYERS.iter().enumerate().map(|(idx, (name, color))| {
                            let name = *name;
                            let color = *color;
                            let toggle = move |_| {
                                layer_visibility.update(|v| {
                                    if let Some(b) = v.get_mut(idx) { *b = !*b; }
                                });
                            };
                            let activate = move |_| active_layer.set(name);
                            let visible = move || layer_visibility.with(|v| v.get(idx).copied().unwrap_or(true));
                            let is_active = move || active_layer.get() == name;
                            view! {
                                <div
                                    class="hw-layer-row"
                                    class:active=is_active
                                    on:click=activate
                                >
                                    <button
                                        class="hw-layer-eye"
                                        class:off=move || !visible()
                                        on:click=move |ev| { ev.stop_propagation(); toggle(()); }
                                        title="Toggle visibility"
                                    >
                                        {move || if visible() { "●" } else { "○" }}
                                    </button>
                                    <span class="hw-layer-swatch" style:background=color></span>
                                    <span class="hw-layer-name">{name}</span>
                                </div>
                            }
                        }).collect_view()}
                    </div>

                    <div class="hw-panel-header"><h3>"Properties"</h3></div>
                    <div class="hw-props">
                        {move || {
                            match selected.get().and_then(|id| parts.with(|ps| ps.iter().find(|p| p.id == id).cloned())) {
                                Some(p) => view! {
                                    <div class="hw-prop-grid">
                                        <span class="hw-prop-k">"Ref"</span>
                                        <span class="hw-prop-v">{p.reference.clone()}</span>
                                        <span class="hw-prop-k">"Part"</span>
                                        <span class="hw-prop-v">{p.name.clone()}</span>
                                        <span class="hw-prop-k">"Layer"</span>
                                        <span class="hw-prop-v">{p.layer}</span>
                                        <span class="hw-prop-k">"X"</span>
                                        <span class="hw-prop-v">{format!("{:.2} mm", p.x / 4.0)}</span>
                                        <span class="hw-prop-k">"Y"</span>
                                        <span class="hw-prop-v">{format!("{:.2} mm", p.y / 4.0)}</span>
                                        <span class="hw-prop-k">"Rot"</span>
                                        <span class="hw-prop-v">{format!("{}°", p.rotation)}</span>
                                    </div>
                                }.into_any(),
                                None => view! {
                                    <div class="hw-empty">"Select a part to inspect."</div>
                                }.into_any(),
                            }
                        }}
                    </div>

                    <div class="hw-panel-header"><h3>"AI Co-Pilot"</h3></div>
                    <div class="hw-copilot">
                        <textarea
                            class="hw-copilot-input"
                            placeholder="e.g. Place 100nF decoupling caps near each VDD pin of U1."
                            prop:value=move || copilot_prompt.get()
                            on:input=move |ev| copilot_prompt.set(event_target_value(&ev))
                        />
                        <button class="hw-btn hw-btn--primary hw-copilot-go">"Suggest"</button>
                        <div class="hw-copilot-out">
                            "AI suggestions arrive in Phase H4 — backed by ef-eda-ai."
                        </div>
                    </div>
                </aside>
            </div>
        </div>
    }
}
