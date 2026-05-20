use leptos::prelude::*;
use leptos::html;

/// One pin on a schematic symbol, in symbol-local pixel coordinates.
#[derive(Clone, Copy, Debug)]
struct Pin {
    name: &'static str,
    x: f64,
    y: f64,
}

/// A reusable symbol definition: SVG body + pin map.
#[derive(Clone, Copy)]
struct SymbolDef {
    label: &'static str,
    prefix: &'static str,
    kind: &'static str,
    width: f64,
    height: f64,
    pins: &'static [Pin],
    category: &'static str,
}

const SYMBOLS: &[SymbolDef] = &[
    SymbolDef {
        label: "Resistor", prefix: "R", kind: "res", width: 80.0, height: 30.0,
        pins: &[Pin { name: "1", x: 0.0, y: 15.0 }, Pin { name: "2", x: 80.0, y: 15.0 }],
        category: "Passives",
    },
    SymbolDef {
        label: "Capacitor", prefix: "C", kind: "cap", width: 60.0, height: 30.0,
        pins: &[Pin { name: "1", x: 0.0, y: 15.0 }, Pin { name: "2", x: 60.0, y: 15.0 }],
        category: "Passives",
    },
    SymbolDef {
        label: "Inductor", prefix: "L", kind: "ind", width: 80.0, height: 30.0,
        pins: &[Pin { name: "1", x: 0.0, y: 15.0 }, Pin { name: "2", x: 80.0, y: 15.0 }],
        category: "Passives",
    },
    SymbolDef {
        label: "Diode", prefix: "D", kind: "diode", width: 60.0, height: 30.0,
        pins: &[Pin { name: "A", x: 0.0, y: 15.0 }, Pin { name: "K", x: 60.0, y: 15.0 }],
        category: "Passives",
    },
    SymbolDef {
        label: "LED", prefix: "D", kind: "led", width: 60.0, height: 30.0,
        pins: &[Pin { name: "A", x: 0.0, y: 15.0 }, Pin { name: "K", x: 60.0, y: 15.0 }],
        category: "Passives",
    },
    SymbolDef {
        label: "NPN BJT", prefix: "Q", kind: "npn", width: 60.0, height: 60.0,
        pins: &[Pin { name: "B", x: 0.0, y: 30.0 }, Pin { name: "C", x: 50.0, y: 0.0 }, Pin { name: "E", x: 50.0, y: 60.0 }],
        category: "Active",
    },
    SymbolDef {
        label: "Op-amp", prefix: "U", kind: "opamp", width: 80.0, height: 60.0,
        pins: &[
            Pin { name: "-", x: 0.0, y: 18.0 },
            Pin { name: "+", x: 0.0, y: 42.0 },
            Pin { name: "OUT", x: 80.0, y: 30.0 },
        ],
        category: "Active",
    },
    SymbolDef {
        label: "MCU 8-pin", prefix: "U", kind: "ic", width: 90.0, height: 110.0,
        pins: &[
            Pin { name: "1", x: 0.0, y: 20.0 },
            Pin { name: "2", x: 0.0, y: 50.0 },
            Pin { name: "3", x: 0.0, y: 80.0 },
            Pin { name: "4", x: 0.0, y: 100.0 },
            Pin { name: "5", x: 90.0, y: 100.0 },
            Pin { name: "6", x: 90.0, y: 80.0 },
            Pin { name: "7", x: 90.0, y: 50.0 },
            Pin { name: "8", x: 90.0, y: 20.0 },
        ],
        category: "Active",
    },
    SymbolDef {
        label: "VCC", prefix: "#PWR", kind: "vcc", width: 30.0, height: 30.0,
        pins: &[Pin { name: "1", x: 15.0, y: 30.0 }],
        category: "Power",
    },
    SymbolDef {
        label: "GND", prefix: "#PWR", kind: "gnd", width: 30.0, height: 30.0,
        pins: &[Pin { name: "1", x: 15.0, y: 0.0 }],
        category: "Power",
    },
];

#[derive(Clone, Debug)]
struct PlacedSymbol {
    id: u32,
    reference: String,
    value: String,
    kind: &'static str,
    label: &'static str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    rotation: i32,
    pins: Vec<Pin>,
}

#[derive(Clone, Copy, Debug)]
struct WireSegment {
    id: u32,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

#[derive(Clone, Copy)]
struct DragState {
    id: u32,
    offset_x: f64,
    offset_y: f64,
}

const GRID: f64 = 10.0;

fn snap(v: f64) -> f64 {
    (v / GRID).round() * GRID
}

fn make_placed(
    id: u32,
    reference: &str,
    value: &str,
    kind: &'static str,
    label: &'static str,
    x: f64,
    y: f64,
) -> PlacedSymbol {
    let def = SYMBOLS.iter().find(|s| s.kind == kind).copied().unwrap_or(SYMBOLS[0]);
    PlacedSymbol {
        id,
        reference: reference.to_string(),
        value: value.to_string(),
        kind: def.kind,
        label,
        x: snap(x),
        y: snap(y),
        width: def.width,
        height: def.height,
        rotation: 0,
        pins: def.pins.to_vec(),
    }
}

/// Render a symbol body as inline SVG, drawn in the symbol's own
/// coordinate space. Callers position the parent `<g>` to place it on
/// the schematic canvas.
fn symbol_svg(kind: &str, w: f64, h: f64) -> impl IntoView {
    match kind {
        "res" => view! {
            <svg viewBox="0 0 80 30" width=w height=h overflow="visible">
                <polyline class="sch-stroke" points="0,15 10,15 14,5 22,25 30,5 38,25 46,5 54,25 62,15 80,15"/>
            </svg>
        }.into_any(),
        "cap" => view! {
            <svg viewBox="0 0 60 30" width=w height=h overflow="visible">
                <line class="sch-stroke" x1="0"  y1="15" x2="25" y2="15"/>
                <line class="sch-stroke" x1="25" y1="3"  x2="25" y2="27"/>
                <line class="sch-stroke" x1="35" y1="3"  x2="35" y2="27"/>
                <line class="sch-stroke" x1="35" y1="15" x2="60" y2="15"/>
            </svg>
        }.into_any(),
        "ind" => view! {
            <svg viewBox="0 0 80 30" width=w height=h overflow="visible">
                <path class="sch-stroke" fill="none"
                    d="M0,15 L10,15 A6,6 0 0 1 22,15 A6,6 0 0 1 34,15 A6,6 0 0 1 46,15 A6,6 0 0 1 58,15 A6,6 0 0 1 70,15 L80,15"/>
            </svg>
        }.into_any(),
        "diode" => view! {
            <svg viewBox="0 0 60 30" width=w height=h overflow="visible">
                <line class="sch-stroke" x1="0"  y1="15" x2="22" y2="15"/>
                <polygon class="sch-stroke sch-fill" points="22,5 22,25 38,15"/>
                <line class="sch-stroke" x1="38" y1="5"  x2="38" y2="25"/>
                <line class="sch-stroke" x1="38" y1="15" x2="60" y2="15"/>
            </svg>
        }.into_any(),
        "led" => view! {
            <svg viewBox="0 0 60 30" width=w height=h overflow="visible">
                <line class="sch-stroke" x1="0"  y1="15" x2="22" y2="15"/>
                <polygon class="sch-stroke sch-led-fill" points="22,5 22,25 38,15"/>
                <line class="sch-stroke" x1="38" y1="5"  x2="38" y2="25"/>
                <line class="sch-stroke" x1="38" y1="15" x2="60" y2="15"/>
                <line class="sch-stroke sch-arrow" x1="36" y1="0" x2="44" y2="-6"/>
                <line class="sch-stroke sch-arrow" x1="44" y1="3" x2="50" y2="-3"/>
            </svg>
        }.into_any(),
        "npn" => view! {
            <svg viewBox="0 0 60 60" width=w height=h overflow="visible">
                <circle class="sch-stroke" cx="30" cy="30" r="20" fill="none"/>
                <line class="sch-stroke" x1="0"  y1="30" x2="20" y2="30"/>
                <line class="sch-stroke" x1="20" y1="15" x2="20" y2="45"/>
                <line class="sch-stroke" x1="20" y1="22" x2="50" y2="0"/>
                <line class="sch-stroke" x1="20" y1="38" x2="50" y2="60"/>
            </svg>
        }.into_any(),
        "opamp" => view! {
            <svg viewBox="0 0 80 60" width=w height=h overflow="visible">
                <polygon class="sch-stroke sch-bg" points="14,4 14,56 66,30"/>
                <line class="sch-stroke" x1="0"  y1="18" x2="14" y2="18"/>
                <line class="sch-stroke" x1="0"  y1="42" x2="14" y2="42"/>
                <line class="sch-stroke" x1="66" y1="30" x2="80" y2="30"/>
                <text x="18" y="22" class="sch-pin-mark">"-"</text>
                <text x="18" y="46" class="sch-pin-mark">"+"</text>
            </svg>
        }.into_any(),
        "ic" => view! {
            <svg viewBox="0 0 90 110" width=w height=h overflow="visible">
                <rect class="sch-stroke sch-bg" x="14" y="6" width="62" height="98"/>
                <line class="sch-stroke" x1="0"  y1="20"  x2="14" y2="20"/>
                <line class="sch-stroke" x1="0"  y1="50"  x2="14" y2="50"/>
                <line class="sch-stroke" x1="0"  y1="80"  x2="14" y2="80"/>
                <line class="sch-stroke" x1="0"  y1="100" x2="14" y2="100"/>
                <line class="sch-stroke" x1="76" y1="20"  x2="90" y2="20"/>
                <line class="sch-stroke" x1="76" y1="50"  x2="90" y2="50"/>
                <line class="sch-stroke" x1="76" y1="80"  x2="90" y2="80"/>
                <line class="sch-stroke" x1="76" y1="100" x2="90" y2="100"/>
            </svg>
        }.into_any(),
        "vcc" => view! {
            <svg viewBox="0 0 30 30" width=w height=h overflow="visible">
                <line class="sch-stroke" x1="15" y1="30" x2="15" y2="14"/>
                <circle class="sch-stroke sch-bg" cx="15" cy="10" r="6"/>
            </svg>
        }.into_any(),
        "gnd" => view! {
            <svg viewBox="0 0 30 30" width=w height=h overflow="visible">
                <line class="sch-stroke" x1="15" y1="0"  x2="15" y2="12"/>
                <line class="sch-stroke" x1="4"  y1="12" x2="26" y2="12"/>
                <line class="sch-stroke" x1="9"  y1="18" x2="21" y2="18"/>
                <line class="sch-stroke" x1="13" y1="24" x2="17" y2="24"/>
            </svg>
        }.into_any(),
        _ => view! {
            <svg viewBox="0 0 40 40" width=w height=h overflow="visible">
                <rect class="sch-stroke sch-bg" x="4" y="4" width="32" height="32"/>
            </svg>
        }.into_any(),
    }
}

/// Schematic Editor — Phase H2 scaffold.
///
/// Drag-drop symbol placement, 90° wire drawing, selection, rotate,
/// delete, and a properties panel. ERC and cross-probe to the PCB
/// designer land in subsequent phases.
#[component]
pub fn HardwareSchematicPage() -> impl IntoView {
    // Seed a tiny example circuit so the canvas isn't empty.
    let initial: Vec<PlacedSymbol> = vec![
        make_placed(1, "R1", "10k",   "res",   "Resistor",  120.0, 160.0),
        make_placed(2, "C1", "100n",  "cap",   "Capacitor", 260.0, 160.0),
        make_placed(3, "D1", "LED",   "led",   "LED",       400.0, 160.0),
        make_placed(4, "U1", "TL072", "opamp", "Op-amp",    260.0, 280.0),
        make_placed(5, "#PWR01", "GND", "gnd", "GND",       280.0, 400.0),
    ];

    let symbols: RwSignal<Vec<PlacedSymbol>> = RwSignal::new(initial);
    let next_id: RwSignal<u32> = RwSignal::new(100);

    let wires: RwSignal<Vec<WireSegment>> = RwSignal::new(vec![]);
    let next_wire_id: RwSignal<u32> = RwSignal::new(1);

    let selected: RwSignal<Option<u32>> = RwSignal::new(None);
    let drag: RwSignal<Option<DragState>> = RwSignal::new(None);

    let wire_start: RwSignal<Option<(f64, f64)>> = RwSignal::new(None);
    let wire_mode: RwSignal<bool> = RwSignal::new(false);
    let cursor: RwSignal<(f64, f64)> = RwSignal::new((0.0, 0.0));
    let zoom: RwSignal<f64> = RwSignal::new(1.0);
    let search: RwSignal<String> = RwSignal::new(String::new());

    let canvas_ref: NodeRef<html::Div> = NodeRef::new();

    let categories = ["Passives", "Active", "Power"];
    let palette_for = move |cat: &'static str| -> Vec<SymbolDef> {
        let q = search.get().to_lowercase();
        SYMBOLS.iter()
            .filter(|s| s.category == cat)
            .filter(|s| q.is_empty() || s.label.to_lowercase().contains(&q))
            .copied()
            .collect()
    };

    let add_symbol = move |def: SymbolDef| {
        let id = next_id.get();
        next_id.set(id + 1);
        let count = symbols
            .with(|ps| ps.iter().filter(|p| p.reference.starts_with(def.prefix)).count())
            + 1;
        let reference = if def.prefix == "#PWR" {
            format!("#PWR{:02}", count)
        } else {
            format!("{}{}", def.prefix, count)
        };
        let value = match def.kind {
            "res" => "1k".into(),
            "cap" => "100n".into(),
            "ind" => "10u".into(),
            "vcc" => "VCC".into(),
            "gnd" => "GND".into(),
            _ => def.label.to_string(),
        };
        symbols.update(|ps| ps.push(PlacedSymbol {
            id,
            reference,
            value,
            kind: def.kind,
            label: def.label,
            x: snap(360.0 - def.width / 2.0),
            y: snap(220.0 - def.height / 2.0),
            width: def.width,
            height: def.height,
            rotation: 0,
            pins: def.pins.to_vec(),
        }));
        selected.set(Some(id));
    };

    let delete_selected = move |_| {
        if let Some(id) = selected.get() {
            symbols.update(|ps| ps.retain(|p| p.id != id));
            selected.set(None);
        }
    };

    let rotate_selected = move |_| {
        if let Some(id) = selected.get() {
            symbols.update(|ps| {
                if let Some(p) = ps.iter_mut().find(|p| p.id == id) {
                    p.rotation = (p.rotation + 90) % 360;
                }
            });
        }
    };

    let zoom_in  = move |_| zoom.update(|z| *z = (*z * 1.15).min(2.5));
    let zoom_out = move |_| zoom.update(|z| *z = (*z / 1.15).max(0.4));
    let zoom_fit = move |_| zoom.set(1.0);

    let toggle_wire = move |_| {
        wire_mode.update(|m| *m = !*m);
        wire_start.set(None);
    };

    let clear_wires = move |_| {
        wires.set(vec![]);
        wire_start.set(None);
    };

    let cancel_wire = move |_| {
        wire_start.set(None);
    };

    let on_canvas_move = move |ev: web_sys::PointerEvent| {
        if let Some(el) = canvas_ref.get() {
            let rect = el.get_bounding_client_rect();
            let z = zoom.get();
            let cx = (ev.client_x() as f64 - rect.left()) / z;
            let cy = (ev.client_y() as f64 - rect.top()) / z;
            cursor.set((snap(cx), snap(cy)));

            if let Some(ds) = drag.get() {
                symbols.update(|ps| {
                    if let Some(p) = ps.iter_mut().find(|p| p.id == ds.id) {
                        p.x = snap(cx - ds.offset_x);
                        p.y = snap(cy - ds.offset_y);
                    }
                });
            }
        }
    };

    let on_canvas_up = move |_ev: web_sys::PointerEvent| {
        drag.set(None);
    };

    let on_canvas_click = move |_ev: web_sys::PointerEvent| {
        if wire_mode.get() {
            let (cx, cy) = cursor.get();
            match wire_start.get() {
                None => wire_start.set(Some((cx, cy))),
                Some((sx, sy)) => {
                    let mid_x = cx;
                    let mid_y = sy;
                    let mut new_segs: Vec<WireSegment> = vec![];
                    if (sx - mid_x).abs() > 0.1 {
                        let id = next_wire_id.get(); next_wire_id.set(id + 1);
                        new_segs.push(WireSegment { id, x1: sx, y1: sy, x2: mid_x, y2: mid_y });
                    }
                    if (mid_y - cy).abs() > 0.1 {
                        let id = next_wire_id.get(); next_wire_id.set(id + 1);
                        new_segs.push(WireSegment { id, x1: mid_x, y1: mid_y, x2: cx, y2: cy });
                    }
                    if !new_segs.is_empty() {
                        wires.update(|w| w.extend(new_segs));
                    }
                    wire_start.set(Some((cx, cy)));
                }
            }
        } else {
            selected.set(None);
        }
    };

    view! {
        <div class="hw-studio-shell">
            // Toolbar
            <div class="hw-toolbar">
                <div class="hw-toolbar-group">
                    <span class="hw-crumb">"Hardware Studio"</span>
                    <span class="hw-crumb-sep">"/"</span>
                    <span class="hw-crumb hw-crumb--active">"Schematic Editor"</span>
                </div>
                <div class="hw-toolbar-group">
                    <button class="hw-btn" on:click=zoom_out title="Zoom out">"−"</button>
                    <span class="hw-zoom">{move || format!("{:>3.0}%", zoom.get() * 100.0)}</span>
                    <button class="hw-btn" on:click=zoom_in title="Zoom in">"+"</button>
                    <button class="hw-btn" on:click=zoom_fit>"Fit"</button>
                    <span class="hw-divider"></span>
                    <button class="hw-btn" on:click=rotate_selected title="Rotate 90°">"⟳"</button>
                    <button class="hw-btn hw-btn--danger" on:click=delete_selected>"Delete"</button>
                    <span class="hw-divider"></span>
                    <button
                        class="hw-btn"
                        class:hw-btn--primary=move || wire_mode.get()
                        on:click=toggle_wire
                        title="Wire tool"
                    >
                        {move || if wire_mode.get() { "Wire: ON" } else { "Wire" }}
                    </button>
                    <button class="hw-btn" on:click=cancel_wire>"Cancel"</button>
                    <button class="hw-btn" on:click=clear_wires>"Clear wires"</button>
                    <span class="hw-divider"></span>
                    <a class="hw-btn" href="/hw/pcb">"Open PCB →"</a>
                    <button class="hw-btn">"Run ERC"</button>
                </div>
            </div>

            <div class="hw-workspace">
                // Left: symbol palette
                <aside class="hw-panel hw-panel--left">
                    <div class="hw-panel-header"><h3>"Symbols"</h3></div>
                    <input
                        class="hw-search"
                        type="search"
                        placeholder="Search symbols…"
                        prop:value=move || search.get()
                        on:input=move |ev| search.set(event_target_value(&ev))
                    />
                    {categories.iter().map(|cat| {
                        let cat = *cat;
                        view! {
                            <div class="hw-cat">
                                <div class="hw-cat-title">{cat}</div>
                                <div class="hw-cat-items">
                                    {move || palette_for(cat).into_iter().map(|def| {
                                        view! {
                                            <button
                                                class="hw-part-chip sch-chip"
                                                on:click=move |_| add_symbol(def)
                                                title=def.label
                                            >
                                                <span class="sch-mini-wrap">
                                                    {symbol_svg(def.kind, 28.0, 18.0)}
                                                </span>
                                                <span class="hw-part-label">{def.label}</span>
                                            </button>
                                        }
                                    }).collect_view()}
                                </div>
                            </div>
                        }
                    }).collect_view()}
                </aside>

                // Canvas
                <main class="hw-canvas-wrap">
                    <div
                        class="sch-canvas"
                        node_ref=canvas_ref
                        on:pointermove=on_canvas_move
                        on:pointerup=on_canvas_up
                        on:pointerleave=on_canvas_up
                        on:pointerdown=on_canvas_click
                        style:transform=move || format!("scale({})", zoom.get())
                        class:wire-mode=move || wire_mode.get()
                    >
                        <svg class="sch-svg" width="1200" height="800" overflow="visible">
                            // Wires
                            {move || wires.get().into_iter().map(|w| view! {
                                <line class="sch-wire" x1=w.x1 y1=w.y1 x2=w.x2 y2=w.y2/>
                            }).collect_view()}

                            // Wire-in-progress preview
                            {move || match (wire_mode.get(), wire_start.get()) {
                                (true, Some((sx, sy))) => {
                                    let (cx, cy) = cursor.get();
                                    view! {
                                        <g class="sch-wire-preview">
                                            <line x1=sx y1=sy x2=cx y2=sy/>
                                            <line x1=cx y1=sy x2=cx y2=cy/>
                                        </g>
                                    }.into_any()
                                }
                                _ => view! { <g></g> }.into_any(),
                            }}

                            // Symbols
                            <For
                                each=move || symbols.get()
                                key=|s| s.id
                                children=move |s: PlacedSymbol| {
                                    let sid = s.id;
                                    let on_down = move |ev: web_sys::PointerEvent| {
                                        ev.stop_propagation();
                                        if wire_mode.get() { return; }
                                        if let Some(el) = canvas_ref.get() {
                                            let rect = el.get_bounding_client_rect();
                                            let z = zoom.get();
                                            let cx = (ev.client_x() as f64 - rect.left()) / z;
                                            let cy = (ev.client_y() as f64 - rect.top()) / z;
                                            if let Some(cur) = symbols.with(|ps| ps.iter().find(|q| q.id == sid).cloned()) {
                                                drag.set(Some(DragState {
                                                    id: sid,
                                                    offset_x: cx - cur.x,
                                                    offset_y: cy - cur.y,
                                                }));
                                            }
                                            selected.set(Some(sid));
                                        }
                                    };
                                    let is_selected = move || selected.get() == Some(sid);
                                    let kind = s.kind;
                                    let ref_text = s.reference.clone();
                                    let value_text = s.value.clone();
                                    let w = s.width;
                                    let h = s.height;
                                    view! {
                                        <g
                                            class="sch-symbol"
                                            class:selected=is_selected
                                            transform=format!("translate({},{}) rotate({} {} {})",
                                                s.x, s.y, s.rotation, s.width / 2.0, s.height / 2.0)
                                            on:pointerdown=on_down
                                        >
                                            {symbol_svg(kind, w, h)}
                                            {s.pins.iter().map(|p| view! {
                                                <circle class="sch-pin" cx=p.x cy=p.y r="2.5"/>
                                            }).collect_view()}
                                            <text class="sch-ref"   x=w / 2.0 y=-6.0>{ref_text}</text>
                                            <text class="sch-value" x=w / 2.0 y=h + 14.0>{value_text}</text>
                                        </g>
                                    }
                                }
                            />
                        </svg>
                    </div>

                    // Status bar
                    <div class="hw-statusbar">
                        <span class="hw-status-item">
                            {move || {
                                let (x, y) = cursor.get();
                                format!("X: {:>5.0}   Y: {:>5.0}", x, y)
                            }}
                        </span>
                        <span class="hw-status-item">{format!("Grid: {:.0}", GRID)}</span>
                        <span class="hw-status-item">
                            {move || if wire_mode.get() { "Tool: WIRE" } else { "Tool: SELECT" }}
                        </span>
                        <span class="hw-status-item hw-drc hw-drc--ok">"ERC: not run"</span>
                    </div>
                </main>

                // Right: sheet + properties
                <aside class="hw-panel hw-panel--right">
                    <div class="hw-panel-header"><h3>"Sheet"</h3></div>
                    <div class="hw-props">
                        <div class="hw-prop-grid">
                            <span class="hw-prop-k">"Name"</span>
                            <span class="hw-prop-v">"root.kicad_sch"</span>
                            <span class="hw-prop-k">"Symbols"</span>
                            <span class="hw-prop-v">{move || symbols.with(|s| s.len().to_string())}</span>
                            <span class="hw-prop-k">"Wires"</span>
                            <span class="hw-prop-v">{move || wires.with(|w| w.len().to_string())}</span>
                        </div>
                    </div>

                    <div class="hw-panel-header"><h3>"Properties"</h3></div>
                    <div class="hw-props">
                        {move || {
                            match selected.get().and_then(|id| symbols.with(|ps| ps.iter().find(|p| p.id == id).cloned())) {
                                Some(p) => view! {
                                    <div class="hw-prop-grid">
                                        <span class="hw-prop-k">"Ref"</span>
                                        <span class="hw-prop-v">{p.reference.clone()}</span>
                                        <span class="hw-prop-k">"Value"</span>
                                        <span class="hw-prop-v">{p.value.clone()}</span>
                                        <span class="hw-prop-k">"Part"</span>
                                        <span class="hw-prop-v">{p.label}</span>
                                        <span class="hw-prop-k">"X"</span>
                                        <span class="hw-prop-v">{format!("{:.0}", p.x)}</span>
                                        <span class="hw-prop-k">"Y"</span>
                                        <span class="hw-prop-v">{format!("{:.0}", p.y)}</span>
                                        <span class="hw-prop-k">"Rot"</span>
                                        <span class="hw-prop-v">{format!("{}°", p.rotation)}</span>
                                    </div>
                                }.into_any(),
                                None => view! {
                                    <div class="hw-empty">"Select a symbol to edit."</div>
                                }.into_any(),
                            }
                        }}
                    </div>

                    <div class="hw-panel-header"><h3>"Tips"</h3></div>
                    <div class="hw-copilot-out">
                        "Drag symbols from the left panel. Toggle " <b>"Wire"</b>
                        " to draw nets — click to anchor, click again to drop an L-segment. "
                        "Use Rotate (⟳) for 90° turns."
                    </div>
                </aside>
            </div>
        </div>
    }
}
