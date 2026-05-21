//! Shared schematic editor engine.
//!
//! Domain-agnostic core: symbols, pins, wires, hierarchy, drag/drop placement,
//! orthogonal wire routing, selection, rotation, undo, JSON persistence.
//!
//! Domain skins (chip / PCB) supply:
//!   * a `SymbolLibrary` (kinds, palette categories, body SVG, pin map)
//!   * an optional starter `Schematic` (preset design)
//!   * a netlist exporter (SPICE for chip, KiCad for PCB) — TODO

use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wasm_bindgen::JsCast;

// ---------------------------------------------------------------------------
// Public data model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Pin {
    pub name: &'static str,
    /// Local symbol coordinates (before rotation/translation).
    pub x: f64,
    pub y: f64,
}

/// A reusable symbol type.
#[derive(Clone, Copy)]
pub struct SymbolDef {
    /// Unique key, e.g. "nmos", "pmos", "jj", "cap".
    pub kind: &'static str,
    /// Human label for the palette.
    pub label: &'static str,
    /// Palette grouping.
    pub category: &'static str,
    /// Reference-designator prefix (M / J / Q / U / R / C / L …).
    pub prefix: &'static str,
    /// Pin map in local coordinates.
    pub pins: &'static [Pin],
    /// Local bounding box: (w, h).
    pub bbox: (f64, f64),
    /// Raw SVG body rendered inside a `<g>` for one instance.
    /// Coordinates must be relative to the symbol's local origin (top-left).
    pub body: &'static str,
}

/// Trait implemented by each domain's symbol library.
pub trait SymbolLibrary: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn symbols(&self) -> &'static [SymbolDef];
    fn lookup(&self, kind: &str) -> Option<&'static SymbolDef> {
        self.symbols().iter().find(|s| s.kind == kind)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Instance {
    pub id: u32,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub rot: i32, // 0/90/180/270
    pub flip: bool,
    pub reference: String, // e.g. "M1", "Q3"
    pub value: String,     // e.g. "W=0.84u L=0.13u", "10pF"
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum Endpoint {
    Pin { inst_id: u32, pin_idx: u32 },
    Free { x: f64, y: f64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wire {
    pub id: u32,
    pub a: Endpoint,
    pub b: Endpoint,
    pub net: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Schematic {
    pub instances: Vec<Instance>,
    pub wires: Vec<Wire>,
    pub next_id: u32,
}

impl Schematic {
    pub fn add_instance(&mut self, kind: &str, x: f64, y: f64, prefix: &str) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        let n = self
            .instances
            .iter()
            .filter(|i| i.reference.starts_with(prefix))
            .count() as u32
            + 1;
        self.instances.push(Instance {
            id,
            kind: kind.into(),
            x,
            y,
            rot: 0,
            flip: false,
            reference: format!("{prefix}{n}"),
            value: String::new(),
        });
        id
    }

    pub fn add_wire(&mut self, a: Endpoint, b: Endpoint) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        self.wires.push(Wire { id, a, b, net: None });
        id
    }

    pub fn delete_instance(&mut self, id: u32) {
        self.instances.retain(|i| i.id != id);
        self.wires.retain(|w| {
            !matches!(w.a, Endpoint::Pin { inst_id, .. } if inst_id == id)
                && !matches!(w.b, Endpoint::Pin { inst_id, .. } if inst_id == id)
        });
    }

    pub fn delete_wire(&mut self, id: u32) {
        self.wires.retain(|w| w.id != id);
    }
}

// ---------------------------------------------------------------------------
// Geometry helpers
// ---------------------------------------------------------------------------

const GRID: f64 = 10.0;

fn snap(v: f64) -> f64 {
    (v / GRID).round() * GRID
}

/// World position of a pin (after instance translation + rotation around bbox centre).
fn pin_world(lib: &dyn SymbolLibrary, inst: &Instance, pin_idx: usize) -> (f64, f64) {
    let Some(def) = lib.lookup(&inst.kind) else {
        return (inst.x, inst.y);
    };
    let Some(p) = def.pins.get(pin_idx) else {
        return (inst.x, inst.y);
    };
    let (w, h) = def.bbox;
    let cx = w * 0.5;
    let cy = h * 0.5;
    let mut lx = p.x - cx;
    let mut ly = p.y - cy;
    if inst.flip {
        lx = -lx;
    }
    let (sx, sy) = match inst.rot.rem_euclid(360) {
        90 => (-ly, lx),
        180 => (-lx, -ly),
        270 => (ly, -lx),
        _ => (lx, ly),
    };
    (inst.x + cx + sx, inst.y + cy + sy)
}

fn endpoint_world(lib: &dyn SymbolLibrary, sch: &Schematic, e: Endpoint) -> (f64, f64) {
    match e {
        Endpoint::Free { x, y } => (x, y),
        Endpoint::Pin { inst_id, pin_idx } => sch
            .instances
            .iter()
            .find(|i| i.id == inst_id)
            .map(|i| pin_world(lib, i, pin_idx as usize))
            .unwrap_or((0.0, 0.0)),
    }
}

/// Orthogonal L-route between two points; returns SVG polyline points.
fn ortho_route(a: (f64, f64), b: (f64, f64)) -> String {
    let mx = b.0;
    let my = a.1;
    format!(
        "{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
        a.0, a.1, mx, my, b.0, b.1
    )
}

// ---------------------------------------------------------------------------
// Editor state (interaction model)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tool {
    Select,
    Wire,
    Pan,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Selection {
    None,
    Instance(u32),
    Wire(u32),
}

#[derive(Clone, Copy, Debug)]
enum Drag {
    None,
    Pan { start_x: f64, start_y: f64, base_x: f64, base_y: f64 },
    Move { inst_id: u32, offset_x: f64, offset_y: f64 },
    WireFrom { start: Endpoint, cur_x: f64, cur_y: f64 },
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

/// Drop-in schematic editor. Pass any `SymbolLibrary` and an optional starter
/// `Schematic`.
#[component]
pub fn SchematicEditor(
    #[prop(into)] title: String,
    #[prop(into)] subtitle: String,
    library: &'static dyn SymbolLibrary,
    #[prop(optional)] preset: Option<Schematic>,
) -> impl IntoView {
    // ----- reactive state -----
    let sch = RwSignal::new(preset.unwrap_or_default());
    let history: RwSignal<Vec<Schematic>> = RwSignal::new(Vec::new());
    let tool = RwSignal::new(Tool::Select);
    let selection = RwSignal::new(Selection::None);
    let drag = RwSignal::new(Drag::None);
    let pan = RwSignal::new((40.0_f64, 40.0_f64));
    let zoom = RwSignal::new(1.0_f64);
    let hover_pin: RwSignal<Option<Endpoint>> = RwSignal::new(None);
    let palette_filter = RwSignal::new(String::new());
    let show_grid = RwSignal::new(true);
    let show_pin_dots = RwSignal::new(true);

    let canvas_ref = NodeRef::<html::Div>::new();

    // ----- helpers -----
    let push_history = move || {
        history.update(|h| {
            h.push(sch.get_untracked());
            if h.len() > 50 {
                h.remove(0);
            }
        });
    };
    let undo = move |_| {
        history.update(|h| {
            if let Some(prev) = h.pop() {
                sch.set(prev);
            }
        });
    };

    let to_world = move |client_x: f64, client_y: f64| -> (f64, f64) {
        let Some(el) = canvas_ref.get_untracked() else {
            return (0.0, 0.0);
        };
        let el: web_sys::Element = el.unchecked_into();
        let r = el.get_bounding_client_rect();
        let (px, py) = pan.get_untracked();
        let z = zoom.get_untracked();
        (
            ((client_x - r.left()) - px) / z,
            ((client_y - r.top()) - py) / z,
        )
    };

    // ---------- mouse handlers ----------
    let on_canvas_mousedown = move |ev: ev::MouseEvent| {
        let (wx, wy) = to_world(ev.client_x() as f64, ev.client_y() as f64);
        // middle / pan tool / space: pan canvas
        if ev.button() == 1 || matches!(tool.get_untracked(), Tool::Pan) {
            let (bx, by) = pan.get_untracked();
            drag.set(Drag::Pan {
                start_x: ev.client_x() as f64,
                start_y: ev.client_y() as f64,
                base_x: bx,
                base_y: by,
            });
            return;
        }
        // background click: deselect / cancel wire
        if matches!(tool.get_untracked(), Tool::Wire) {
            // start a free-floating wire
            push_history();
            drag.set(Drag::WireFrom {
                start: Endpoint::Free { x: snap(wx), y: snap(wy) },
                cur_x: snap(wx),
                cur_y: snap(wy),
            });
        } else {
            selection.set(Selection::None);
        }
    };

    let on_canvas_mousemove = move |ev: ev::MouseEvent| {
        let (wx, wy) = to_world(ev.client_x() as f64, ev.client_y() as f64);
        match drag.get_untracked() {
            Drag::Pan { start_x, start_y, base_x, base_y } => {
                pan.set((
                    base_x + (ev.client_x() as f64 - start_x),
                    base_y + (ev.client_y() as f64 - start_y),
                ));
            }
            Drag::Move { inst_id, offset_x, offset_y } => {
                sch.update(|s| {
                    if let Some(i) = s.instances.iter_mut().find(|i| i.id == inst_id) {
                        i.x = snap(wx - offset_x);
                        i.y = snap(wy - offset_y);
                    }
                });
            }
            Drag::WireFrom { start, .. } => {
                drag.set(Drag::WireFrom {
                    start,
                    cur_x: snap(wx),
                    cur_y: snap(wy),
                });
            }
            Drag::None => {}
        }
    };

    let on_canvas_mouseup = move |_: ev::MouseEvent| {
        if let Drag::WireFrom { start, cur_x, cur_y } = drag.get_untracked() {
            let end = hover_pin
                .get_untracked()
                .unwrap_or(Endpoint::Free { x: cur_x, y: cur_y });
            if end != start {
                sch.update(|s| {
                    s.add_wire(start, end);
                });
            }
        }
        drag.set(Drag::None);
    };

    let on_wheel = move |ev: ev::WheelEvent| {
        ev.prevent_default();
        let d = if ev.delta_y() < 0.0 { 1.1 } else { 1.0 / 1.1 };
        let (px, py) = pan.get_untracked();
        let z0 = zoom.get_untracked();
        let z1 = (z0 * d).clamp(0.25, 4.0);
        // zoom toward cursor
        if let Some(el) = canvas_ref.get_untracked() {
            let el: web_sys::Element = el.unchecked_into();
            let r = el.get_bounding_client_rect();
            let cx = ev.client_x() as f64 - r.left();
            let cy = ev.client_y() as f64 - r.top();
            let nx = cx - (cx - px) * (z1 / z0);
            let ny = cy - (cy - py) * (z1 / z0);
            pan.set((nx, ny));
        }
        zoom.set(z1);
    };

    // ---------- drag-drop from palette ----------
    let on_dragover = move |ev: ev::DragEvent| {
        ev.prevent_default();
    };
    let on_drop = move |ev: ev::DragEvent| {
        ev.prevent_default();
        let kind = ev
            .data_transfer()
            .and_then(|d| d.get_data("application/x-ef-symbol").ok())
            .unwrap_or_default();
        if kind.is_empty() {
            return;
        }
        let def = match library.lookup(&kind) {
            Some(d) => d,
            None => return,
        };
        let (wx, wy) = to_world(ev.client_x() as f64, ev.client_y() as f64);
        // centre symbol under cursor
        let (w, h) = def.bbox;
        push_history();
        let mut id = 0;
        sch.update(|s| {
            id = s.add_instance(kind.as_str(), snap(wx - w * 0.5), snap(wy - h * 0.5), def.prefix);
        });
        selection.set(Selection::Instance(id));
    };

    // ---------- toolbar actions ----------
    let do_rotate = move |_| {
        if let Selection::Instance(id) = selection.get_untracked() {
            push_history();
            sch.update(|s| {
                if let Some(i) = s.instances.iter_mut().find(|i| i.id == id) {
                    i.rot = (i.rot + 90) % 360;
                }
            });
        }
    };
    let do_flip = move |_| {
        if let Selection::Instance(id) = selection.get_untracked() {
            push_history();
            sch.update(|s| {
                if let Some(i) = s.instances.iter_mut().find(|i| i.id == id) {
                    i.flip = !i.flip;
                }
            });
        }
    };
    let do_delete = move |_| {
        push_history();
        match selection.get_untracked() {
            Selection::Instance(id) => sch.update(|s| s.delete_instance(id)),
            Selection::Wire(id) => sch.update(|s| s.delete_wire(id)),
            Selection::None => {}
        }
        selection.set(Selection::None);
    };
    let do_clear = move |_| {
        push_history();
        sch.set(Schematic::default());
        selection.set(Selection::None);
    };
    let do_export = move |_| {
        let json = serde_json::to_string_pretty(&sch.get_untracked()).unwrap_or_default();
        if let Some(win) = web_sys::window() {
            let _ = win.alert_with_message(&json);
        }
    };

    // ---------- pin/instance event handlers (closures returned to view) ----------
    let pick_instance = move |id: u32, ev: ev::MouseEvent| {
        ev.stop_propagation();
        let (wx, wy) = to_world(ev.client_x() as f64, ev.client_y() as f64);
        selection.set(Selection::Instance(id));
        let s = sch.read_untracked();
        if let Some(i) = s.instances.iter().find(|i| i.id == id) {
            push_history();
            drag.set(Drag::Move { inst_id: id, offset_x: wx - i.x, offset_y: wy - i.y });
        }
    };
    let pick_wire = move |id: u32, ev: ev::MouseEvent| {
        ev.stop_propagation();
        selection.set(Selection::Wire(id));
    };
    let pin_down = move |inst_id: u32, pin_idx: u32, ev: ev::MouseEvent| {
        ev.stop_propagation();
        push_history();
        let (wx, wy) = to_world(ev.client_x() as f64, ev.client_y() as f64);
        drag.set(Drag::WireFrom {
            start: Endpoint::Pin { inst_id, pin_idx },
            cur_x: snap(wx),
            cur_y: snap(wy),
        });
    };
    let pin_enter = move |inst_id: u32, pin_idx: u32| {
        hover_pin.set(Some(Endpoint::Pin { inst_id, pin_idx }));
    };
    let pin_leave = move |_| {
        hover_pin.set(None);
    };

    // ---------- derived: palette categories ----------
    let categories: HashMap<&'static str, Vec<&'static SymbolDef>> = {
        let mut m: HashMap<&'static str, Vec<&'static SymbolDef>> = HashMap::new();
        for s in library.symbols() {
            m.entry(s.category).or_default().push(s);
        }
        m
    };
    let mut cat_keys: Vec<&'static str> = categories.keys().copied().collect();
    cat_keys.sort();

    // ---------- view ----------
    let header_title = title.clone();
    let header_subtitle = subtitle.clone();

    view! {
        <style>{SCHEMATIC_CSS}</style>

        <section class="sch-page">
            <header class="sch-toolbar">
                <div class="sch-title">
                    <h2>{header_title}</h2>
                    <span>{header_subtitle}</span>
                </div>
                <div class="sch-tools">
                    <button
                        class="sch-tool"
                        class:active=move || matches!(tool.get(), Tool::Select)
                        on:click=move |_| tool.set(Tool::Select)
                        title="Select / move (V)"
                    >"⬚ Select"</button>
                    <button
                        class="sch-tool"
                        class:active=move || matches!(tool.get(), Tool::Wire)
                        on:click=move |_| tool.set(Tool::Wire)
                        title="Wire (W)"
                    >"⟿ Wire"</button>
                    <button
                        class="sch-tool"
                        class:active=move || matches!(tool.get(), Tool::Pan)
                        on:click=move |_| tool.set(Tool::Pan)
                        title="Pan (Space)"
                    >"✥ Pan"</button>
                    <span class="sch-sep"/>
                    <button class="sch-tool" on:click=do_rotate title="Rotate (R)">"⟳ Rot"</button>
                    <button class="sch-tool" on:click=do_flip title="Flip (F)">"⇆ Flip"</button>
                    <button class="sch-tool sch-danger" on:click=do_delete title="Delete (⌫)">"✕ Del"</button>
                    <span class="sch-sep"/>
                    <button class="sch-tool" on:click=undo title="Undo (⌘Z)">"↶ Undo"</button>
                    <button class="sch-tool" on:click=do_clear>"Clear"</button>
                    <button class="sch-tool" on:click=do_export>"Export JSON"</button>
                    <span class="sch-sep"/>
                    <label class="sch-toggle">
                        <input type="checkbox" prop:checked=move || show_grid.get()
                            on:change=move |ev| show_grid.set(event_target_checked(&ev))/>
                        "Grid"
                    </label>
                    <label class="sch-toggle">
                        <input type="checkbox" prop:checked=move || show_pin_dots.get()
                            on:change=move |ev| show_pin_dots.set(event_target_checked(&ev))/>
                        "Pins"
                    </label>
                    <span class="sch-sep"/>
                    <span class="sch-zoom">
                        {move || format!("{:.0}%", zoom.get() * 100.0)}
                    </span>
                </div>
            </header>

            <div class="sch-body">
                // ---------- left: palette ----------
                <aside class="sch-palette">
                    <div class="sch-palette-search">
                        <input
                            type="text"
                            placeholder="Filter symbols…"
                            prop:value=move || palette_filter.get()
                            on:input=move |ev| palette_filter.set(event_target_value(&ev))/>
                    </div>
                    {cat_keys.into_iter().map(|cat| {
                        let syms: Vec<&'static SymbolDef> = categories[cat].clone();
                        view! {
                            <div class="sch-pal-cat">
                                <h4>{cat}</h4>
                                <div class="sch-pal-grid">
                                    {syms.into_iter().map(|s| {
                                        let kind = s.kind;
                                        let label = s.label;
                                        let (w, h) = s.bbox;
                                        let vb = format!("0 0 {} {}", w, h);
                                        let body = s.body;
                                        let visible = {
                                            let label_lc = label.to_ascii_lowercase();
                                            let kind_lc = kind.to_ascii_lowercase();
                                            move || {
                                                let f = palette_filter.get().to_ascii_lowercase();
                                                f.is_empty() || label_lc.contains(&f) || kind_lc.contains(&f)
                                            }
                                        };
                                        view! {
                                            <div class="sch-pal-item"
                                                class:hidden=move || !visible()
                                                draggable="true"
                                                on:dragstart=move |ev: ev::DragEvent| {
                                                    if let Some(dt) = ev.data_transfer() {
                                                        let _ = dt.set_data("application/x-ef-symbol", kind);
                                                        dt.set_effect_allowed("copy");
                                                    }
                                                }
                                                title=label
                                            >
                                                <svg viewBox=vb preserveAspectRatio="xMidYMid meet">
                                                    <g inner_html=body/>
                                                </svg>
                                                <span>{label}</span>
                                            </div>
                                        }
                                    }).collect_view()}
                                </div>
                            </div>
                        }
                    }).collect_view()}
                </aside>

                // ---------- centre: canvas ----------
                <div
                    class="sch-canvas-wrap"
                    node_ref=canvas_ref
                    on:mousedown=on_canvas_mousedown
                    on:mousemove=on_canvas_mousemove
                    on:mouseup=on_canvas_mouseup
                    on:mouseleave=on_canvas_mouseup
                    on:wheel=on_wheel
                    on:dragover=on_dragover
                    on:drop=on_drop
                >
                    <svg class="sch-canvas" xmlns="http://www.w3.org/2000/svg">
                        <defs>
                            <pattern id="sch-grid-sm" width="10" height="10" patternUnits="userSpaceOnUse">
                                <path d="M10 0 L0 0 0 10" fill="none" stroke="rgba(120,140,180,0.10)" stroke-width="0.5"/>
                            </pattern>
                            <pattern id="sch-grid-lg" width="100" height="100" patternUnits="userSpaceOnUse">
                                <rect width="100" height="100" fill="url(#sch-grid-sm)"/>
                                <path d="M100 0 L0 0 0 100" fill="none" stroke="rgba(120,140,180,0.18)" stroke-width="0.8"/>
                            </pattern>
                            <filter id="sch-glow" x="-50%" y="-50%" width="200%" height="200%">
                                <feGaussianBlur stdDeviation="2" result="b"/>
                                <feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge>
                            </filter>
                            <linearGradient id="sch-sel" x1="0" y1="0" x2="1" y2="1">
                                <stop offset="0%" stop-color="#6e8cf2"/>
                                <stop offset="100%" stop-color="#b86bff"/>
                            </linearGradient>
                        </defs>

                        // grid background — fixed (no pan/zoom) so it always covers viewport
                        <rect class="sch-bg" width="100%" height="100%"
                            fill=move || if show_grid.get() { "url(#sch-grid-lg)" } else { "#0c1220" }/>

                        // world group: pan + zoom
                        <g transform=move || {
                            let (px, py) = pan.get();
                            format!("translate({:.2} {:.2}) scale({:.3})", px, py, zoom.get())
                        }>
                            // wires
                            {move || {
                                let s = sch.get();
                                let sel = selection.get();
                                s.wires.iter().map(|w| {
                                    let a = endpoint_world(library, &s, w.a);
                                    let b = endpoint_world(library, &s, w.b);
                                    let pts = ortho_route(a, b);
                                    let wid = w.id;
                                    let selected = matches!(sel, Selection::Wire(i) if i == wid);
                                    view! {
                                        <polyline class="sch-wire"
                                            class:selected=selected
                                            points=pts
                                            on:mousedown=move |ev| pick_wire(wid, ev)
                                        />
                                    }
                                }).collect_view()
                            }}

                            // preview wire while dragging
                            {move || {
                                if let Drag::WireFrom { start, cur_x, cur_y } = drag.get() {
                                    let s = sch.get();
                                    let a = endpoint_world(library, &s, start);
                                    let b = (cur_x, cur_y);
                                    let pts = ortho_route(a, b);
                                    view! { <polyline class="sch-wire-preview" points=pts/> }.into_any()
                                } else {
                                    ().into_any()
                                }
                            }}

                            // instances
                            {move || {
                                let s = sch.get();
                                let sel = selection.get();
                                let show_pins = show_pin_dots.get();
                                s.instances.iter().map(|inst| {
                                    let id = inst.id;
                                    let def = match library.lookup(&inst.kind) {
                                        Some(d) => d,
                                        None => return ().into_any(),
                                    };
                                    let (w, h) = def.bbox;
                                    let cx = w * 0.5;
                                    let cy = h * 0.5;
                                    let selected = matches!(sel, Selection::Instance(i) if i == id);
                                    let tf = format!(
                                        "translate({:.1} {:.1}) translate({:.1} {:.1}) {} rotate({}) translate({:.1} {:.1})",
                                        inst.x, inst.y,
                                        cx, cy,
                                        if inst.flip { "scale(-1 1)" } else { "" },
                                        inst.rot,
                                        -cx, -cy,
                                    );
                                    let body = def.body;
                                    let pins = def.pins;
                                    let reference = inst.reference.clone();
                                    let value = inst.value.clone();
                                    view! {
                                        <g class="sch-inst" class:selected=selected transform=tf
                                            on:mousedown=move |ev| pick_instance(id, ev)
                                        >
                                            // selection halo
                                            <rect class="sch-halo" x="-3" y="-3"
                                                width={w + 6.0} height={h + 6.0}
                                                rx="4" ry="4"/>
                                            <g class="sch-sym-body" inner_html=body/>
                                            // pin dots
                                            {if show_pins {
                                                pins.iter().enumerate().map(|(i, p)| {
                                                    let pi = i as u32;
                                                    view! {
                                                        <circle class="sch-pin"
                                                            cx=p.x cy=p.y r="2.4"
                                                            on:mousedown=move |ev| pin_down(id, pi, ev)
                                                            on:mouseenter=move |_| pin_enter(id, pi)
                                                            on:mouseleave=pin_leave
                                                        />
                                                    }
                                                }).collect_view().into_any()
                                            } else { ().into_any() }}
                                            // labels
                                            <text class="sch-ref" x=cx y={-6.0} text-anchor="middle">{reference}</text>
                                            <text class="sch-val" x=cx y={h + 14.0} text-anchor="middle">{value}</text>
                                        </g>
                                    }.into_any()
                                }).collect_view()
                            }}
                        </g>
                    </svg>
                </div>

                // ---------- right: inspector ----------
                <aside class="sch-inspector">
                    <h4>"Inspector"</h4>
                    {move || match selection.get() {
                        Selection::None => view! { <p class="sch-hint">"Drag a symbol from the left palette onto the canvas. Click a pin (small dot) and drag to another pin to draw a wire."</p> }.into_any(),
                        Selection::Wire(id) => {
                            let s = sch.read();
                            let net = s.wires.iter().find(|w| w.id == id).and_then(|w| w.net.clone()).unwrap_or_else(|| "(unassigned)".into());
                            view! {
                                <div class="sch-insp-row"><span>"Type"</span><b>"Wire"</b></div>
                                <div class="sch-insp-row"><span>"Net"</span><b>{net}</b></div>
                                <label class="sch-insp-field">
                                    <span>"Net name"</span>
                                    <input type="text"
                                        on:change=move |ev| {
                                            let v = event_target_value(&ev);
                                            sch.update(|s| {
                                                if let Some(w) = s.wires.iter_mut().find(|w| w.id == id) {
                                                    w.net = if v.is_empty() { None } else { Some(v) };
                                                }
                                            });
                                        }/>
                                </label>
                            }.into_any()
                        }
                        Selection::Instance(id) => {
                            let snap_s = sch.read();
                            let Some(inst) = snap_s.instances.iter().find(|i| i.id == id) else {
                                return ().into_any();
                            };
                            let def = library.lookup(&inst.kind);
                            let kind_label = def.map(|d| d.label).unwrap_or("?");
                            let reference = inst.reference.clone();
                            let value = inst.value.clone();
                            let rot = inst.rot;
                            view! {
                                <div class="sch-insp-row"><span>"Symbol"</span><b>{kind_label}</b></div>
                                <div class="sch-insp-row"><span>"Rotation"</span><b>{format!("{rot}°")}</b></div>
                                <label class="sch-insp-field">
                                    <span>"Reference"</span>
                                    <input type="text" prop:value=reference
                                        on:change=move |ev| {
                                            let v = event_target_value(&ev);
                                            sch.update(|s| {
                                                if let Some(i) = s.instances.iter_mut().find(|i| i.id == id) {
                                                    i.reference = v;
                                                }
                                            });
                                        }/>
                                </label>
                                <label class="sch-insp-field">
                                    <span>"Value / params"</span>
                                    <input type="text" prop:value=value
                                        placeholder="e.g. W=0.84u L=0.13u"
                                        on:change=move |ev| {
                                            let v = event_target_value(&ev);
                                            sch.update(|s| {
                                                if let Some(i) = s.instances.iter_mut().find(|i| i.id == id) {
                                                    i.value = v;
                                                }
                                            });
                                        }/>
                                </label>
                                <div class="sch-insp-actions">
                                    <button class="sch-tool" on:click=do_rotate>"Rotate 90°"</button>
                                    <button class="sch-tool" on:click=do_flip>"Flip"</button>
                                    <button class="sch-tool sch-danger" on:click=do_delete>"Delete"</button>
                                </div>
                            }.into_any()
                        }
                    }}

                    <h4 class="sch-stats-h">"Design"</h4>
                    {move || {
                        let s = sch.read();
                        let i = s.instances.len();
                        let w = s.wires.len();
                        view! {
                            <div class="sch-insp-row"><span>"Symbols"</span><b>{i}</b></div>
                            <div class="sch-insp-row"><span>"Wires"</span><b>{w}</b></div>
                            <div class="sch-insp-row"><span>"Library"</span><b>{library.name()}</b></div>
                        }
                    }}

                    <h4 class="sch-stats-h">"Tips"</h4>
                    <ul class="sch-tips">
                        <li><kbd>"Drag"</kbd>" symbol from palette onto canvas"</li>
                        <li><kbd>"Click"</kbd>" + drag a pin to another pin to wire"</li>
                        <li><kbd>"Wheel"</kbd>" to zoom, middle-drag or Pan tool to pan"</li>
                        <li>"Rotate / flip / delete from the toolbar"</li>
                    </ul>
                </aside>
            </div>
        </section>
    }
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

const SCHEMATIC_CSS: &str = r#"
.sch-page { display:flex; flex-direction:column; height: calc(100vh - 88px); background:#0a0f1c; color:#dbe2f1; }
.sch-toolbar { display:flex; align-items:center; justify-content:space-between; gap:1rem;
    padding:.55rem .85rem; background:linear-gradient(180deg,#121a2e,#0b1222);
    border-bottom:1px solid #1d2740; }
.sch-title h2 { font-size:1.05rem; margin:0; background:linear-gradient(90deg,#9fb6ff,#d39bff);
    -webkit-background-clip:text; background-clip:text; color:transparent; }
.sch-title span { font-size:.78rem; color:#8c98b3; }
.sch-tools { display:flex; align-items:center; gap:.35rem; flex-wrap:wrap; }
.sch-tool { background:#1a2540; color:#cdd6ee; border:1px solid #2a3654; padding:.32rem .6rem;
    border-radius:6px; font-size:.78rem; cursor:pointer; transition:all .15s; }
.sch-tool:hover { background:#243358; border-color:#3a4c7a; transform:translateY(-1px); }
.sch-tool.active { background:linear-gradient(135deg,#3d52a0,#7b4ec2); border-color:#9fb6ff;
    box-shadow:0 0 12px rgba(123,78,194,.5); }
.sch-tool.sch-danger:hover { background:#5a1f2c; border-color:#c14a6b; }
.sch-sep { width:1px; height:18px; background:#2a3654; margin:0 .25rem; }
.sch-toggle { display:flex; align-items:center; gap:.3rem; font-size:.75rem; color:#9eaac8; }
.sch-zoom { font-size:.75rem; color:#9eaac8; min-width:46px; text-align:right; }

.sch-body { display:grid; grid-template-columns:240px 1fr 280px; flex:1; min-height:0; }

.sch-palette { background:#0e1526; border-right:1px solid #1d2740; overflow-y:auto;
    padding:.6rem .5rem; }
.sch-palette-search input { width:100%; background:#1a2540; color:#dbe2f1;
    border:1px solid #2a3654; border-radius:5px; padding:.32rem .5rem; font-size:.78rem; }
.sch-pal-cat h4 { color:#7e8db0; font-size:.72rem; text-transform:uppercase; letter-spacing:.06em;
    margin:.8rem .25rem .35rem; }
.sch-pal-grid { display:grid; grid-template-columns:1fr 1fr; gap:.35rem; }
.sch-pal-item { background:#141d33; border:1px solid #233258; border-radius:6px; padding:.4rem .3rem;
    display:flex; flex-direction:column; align-items:center; gap:.2rem; cursor:grab;
    transition:all .15s; }
.sch-pal-item:hover { background:#1c2848; border-color:#4863a8;
    box-shadow:0 0 12px rgba(110,140,242,.25); transform:translateY(-1px); }
.sch-pal-item:active { cursor:grabbing; }
.sch-pal-item.hidden { display:none; }
.sch-pal-item svg { width:100%; height:42px; }
.sch-pal-item span { font-size:.7rem; color:#bac4dd; text-align:center; }

.sch-canvas-wrap { position:relative; background:#0c1220; overflow:hidden; cursor:crosshair; }
.sch-canvas { width:100%; height:100%; display:block; user-select:none; }

.sch-wire { fill:none; stroke:#5fd7b7; stroke-width:1.8; stroke-linecap:round;
    stroke-linejoin:round; cursor:pointer; }
.sch-wire:hover { stroke:#a0ffe1; filter:drop-shadow(0 0 4px #5fd7b7); }
.sch-wire.selected { stroke:url(#sch-sel); stroke-width:2.4; filter:drop-shadow(0 0 6px #b86bff); }
.sch-wire-preview { fill:none; stroke:#ffd166; stroke-width:1.6; stroke-dasharray:4 3;
    pointer-events:none; filter:drop-shadow(0 0 4px #ffd166); }

.sch-inst { cursor:move; }
.sch-halo { fill:none; stroke:transparent; stroke-width:1.4; pointer-events:none;
    transition:stroke .15s; }
.sch-inst.selected .sch-halo { stroke:url(#sch-sel); filter:url(#sch-glow); }
.sch-sym-body { pointer-events:none; }
.sch-pin { fill:#ffd166; stroke:#1a1a1a; stroke-width:.6; cursor:crosshair;
    transition:r .12s; }
.sch-pin:hover { fill:#ffeb99; r:3.6; filter:drop-shadow(0 0 5px #ffd166); }
.sch-ref { font:600 8px ui-monospace,monospace; fill:#9fb6ff; pointer-events:none; }
.sch-val { font:500 7.5px ui-monospace,monospace; fill:#bac4dd; pointer-events:none; }

/* default body styling — overridable by per-symbol classes */
.sch-sym-body path, .sch-sym-body line, .sch-sym-body polyline,
.sch-sym-body rect, .sch-sym-body circle, .sch-sym-body polygon {
    stroke:#cdd6ee; stroke-width:1.4; fill:none; stroke-linecap:round; stroke-linejoin:round;
}
.sch-sym-body .fill { fill:#cdd6ee; stroke:none; }
.sch-sym-body .pmos { stroke:#ff9bd2; }
.sch-sym-body .nmos { stroke:#9bd7ff; }
.sch-sym-body .jj { stroke:#ffd166; }
.sch-sym-body .label { font:600 7px ui-monospace,monospace; fill:#7e8db0; stroke:none; }

.sch-inspector { background:#0e1526; border-left:1px solid #1d2740; padding:.7rem .8rem;
    overflow-y:auto; }
.sch-inspector h4 { font-size:.72rem; color:#7e8db0; text-transform:uppercase; letter-spacing:.06em;
    margin:.4rem 0 .5rem; }
.sch-stats-h { margin-top:1.4rem !important; }
.sch-insp-row { display:flex; justify-content:space-between; padding:.25rem 0; font-size:.78rem;
    border-bottom:1px dashed #1d2740; }
.sch-insp-row span { color:#9eaac8; }
.sch-insp-row b { color:#dbe2f1; font-weight:500; }
.sch-insp-field { display:flex; flex-direction:column; gap:.2rem; margin:.55rem 0;
    font-size:.74rem; color:#9eaac8; }
.sch-insp-field input { background:#1a2540; color:#dbe2f1; border:1px solid #2a3654;
    border-radius:5px; padding:.32rem .5rem; font-size:.78rem; }
.sch-insp-field input:focus { outline:none; border-color:#6e8cf2;
    box-shadow:0 0 0 2px rgba(110,140,242,.25); }
.sch-insp-actions { display:flex; gap:.3rem; margin-top:.6rem; flex-wrap:wrap; }
.sch-hint { font-size:.78rem; color:#7e8db0; line-height:1.5; }
.sch-tips { padding-left:1rem; margin:.3rem 0; }
.sch-tips li { font-size:.74rem; color:#9eaac8; line-height:1.6; }
kbd { background:#1a2540; border:1px solid #2a3654; border-radius:3px; padding:.05rem .3rem;
    font:600 .7rem ui-monospace,monospace; color:#dbe2f1; }
"#;
