use leptos::prelude::*;
use leptos_router::hooks::use_query_map;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Isometric projection helpers
// ---------------------------------------------------------------------------

const SQRT3_2: f64 = 0.866_025_4;

/// Project a 3D point (x east, y north-into-page, z up) to 2D screen.
fn p(x: f64, y: f64, z: f64) -> (f64, f64) {
    ((x - y) * SQRT3_2, (x + y) * 0.5 - z)
}

fn poly4(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> String {
    format!(
        "{:.1},{:.1} {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
        a.0, a.1, b.0, b.1, c.0, c.1, d.0, d.1
    )
}

fn slab_top(x0: f64, y0: f64, z: f64, w: f64, d: f64) -> String {
    poly4(p(x0, y0, z), p(x0 + w, y0, z), p(x0 + w, y0 + d, z), p(x0, y0 + d, z))
}
fn slab_right(x: f64, y0: f64, z0: f64, d: f64, h: f64) -> String {
    poly4(
        p(x, y0, z0),
        p(x, y0 + d, z0),
        p(x, y0 + d, z0 + h),
        p(x, y0, z0 + h),
    )
}
fn slab_front(x0: f64, y: f64, z0: f64, w: f64, h: f64) -> String {
    poly4(
        p(x0, y, z0),
        p(x0 + w, y, z0),
        p(x0 + w, y, z0 + h),
        p(x0, y, z0 + h),
    )
}

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Feat {
    x: f64,
    y: f64,
    w: f64,
    d: f64,
    cls: &'static str,
    label: Option<&'static str>,
}

#[derive(Clone)]
struct Lyr {
    id: String,
    name: String,
    material: String,
    thickness_nm: f64,
    slab_h: f64,
    cls: &'static str,
    feats: Vec<Feat>,
    /// Optional inspector notes (units/features count summary)
    summary: String,
}

#[derive(Clone, Copy)]
struct Via {
    x: f64,
    y: f64,
    r: f64,
    from_idx: usize,
    to_idx: usize,
    cls: &'static str,
}

#[derive(Clone)]
struct Proj {
    id: &'static str,
    name: String,
    pdk: String,
    die_w: f64,
    die_d: f64,
    layers: Vec<Lyr>,
    vias: Vec<Via>,
}

// Memo equality: project id uniquely determines the (pure) build output.
impl PartialEq for Proj {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

// ---------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------

fn project_sqc_q5() -> Proj {
    // 5 transmons in linear-with-T arrangement
    let q_pos: [(f64, f64, &str, &str); 5] = [
        (80.0, 120.0, "Q0", "data"),
        (160.0, 60.0, "Q1", "ancilla"),
        (240.0, 120.0, "Q2", "data"),
        (240.0, 200.0, "Q3", "ancilla"),
        (340.0, 120.0, "Q4", "data"),
    ];

    // X-mon transmon = a horizontal + vertical arm (drawn as two rectangles)
    let mut xmon_feats: Vec<Feat> = Vec::new();
    let mut jj_lower: Vec<Feat> = Vec::new();
    let mut jj_upper: Vec<Feat> = Vec::new();
    let mut bandage: Vec<Feat> = Vec::new();
    for (cx, cy, _id, role) in q_pos.iter() {
        let cls = if *role == "ancilla" { "feat-xmon-anc" } else { "feat-xmon-data" };
        // horizontal arm
        xmon_feats.push(Feat { x: cx - 22.0, y: cy - 4.0, w: 44.0, d: 8.0, cls, label: None });
        // vertical arm
        xmon_feats.push(Feat { x: cx - 4.0, y: cy - 22.0, w: 8.0, d: 44.0, cls, label: None });
        // central junction pad (Al base)
        xmon_feats.push(Feat { x: cx - 3.0, y: cy - 3.0, w: 6.0, d: 6.0, cls: "feat-pad", label: None });
        // AlOx tunnel barrier (tiny)
        jj_lower.push(Feat { x: cx - 1.5, y: cy - 1.5, w: 3.0, d: 3.0, cls: "feat-jj", label: Some("JJ") });
        // Al top electrode (Manhattan)
        jj_upper.push(Feat { x: cx - 2.5, y: cy - 2.5, w: 5.0, d: 5.0, cls: "feat-jj-top", label: None });
        // Au bandage stitching ground
        bandage.push(Feat { x: cx + 18.0, y: cy + 18.0, w: 6.0, d: 6.0, cls: "feat-bandage", label: None });
    }

    // Bus CPW (drawn as a thin strip on Nb layer + Al layer feature)
    let cpw = Feat { x: 50.0, y: 116.0, w: 320.0, d: 4.0, cls: "feat-cpw", label: Some("bus CPW") };

    // Readout resonators (short hooks per qubit)
    let mut readouts: Vec<Feat> = Vec::new();
    for (cx, cy, _id, _) in q_pos.iter() {
        readouts.push(Feat { x: cx - 30.0, y: cy - 38.0, w: 60.0, d: 2.0, cls: "feat-readout", label: None });
        readouts.push(Feat { x: cx + 28.0, y: cy - 38.0, w: 2.0, d: 18.0, cls: "feat-readout", label: None });
    }

    // Airbridges (Al) crossing the bus
    let mut airbridges: Vec<Feat> = Vec::new();
    for (cx, _, _, _) in q_pos.iter() {
        airbridges.push(Feat { x: cx - 2.0, y: 100.0, w: 4.0, d: 32.0, cls: "feat-airbridge", label: None });
    }

    // Layers — bottom-up
    let layers = vec![
        Lyr {
            id: "sub".into(),
            name: "Substrate".into(),
            material: "Sapphire (c-plane)".into(),
            thickness_nm: 500_000.0,
            slab_h: 28.0,
            cls: "lyr-sub",
            feats: vec![],
            summary: "500 µm c-plane sapphire wafer · low-loss dielectric.".into(),
        },
        Lyr {
            id: "nb_gp".into(),
            name: "Ground Plane".into(),
            material: "Niobium (Nb)".into(),
            thickness_nm: 200.0,
            slab_h: 16.0,
            cls: "lyr-nb",
            feats: {
                let mut v = vec![cpw.clone()];
                v.extend(readouts.iter().cloned());
                v
            },
            summary: "200 nm Nb sputtered ground plane · bus CPW and readout resonators patterned out.".into(),
        },
        Lyr {
            id: "al_base".into(),
            name: "Qubit Pads (Al base)".into(),
            material: "Aluminum (Al)".into(),
            thickness_nm: 150.0,
            slab_h: 14.0,
            cls: "lyr-al",
            feats: xmon_feats.clone(),
            summary: "5 × X-mon transmons (3 data, 2 ancilla) · Al pads + JJ bottom electrode.".into(),
        },
        Lyr {
            id: "alox".into(),
            name: "Tunnel Barrier".into(),
            material: "AlOx".into(),
            thickness_nm: 1.5,
            slab_h: 6.0,
            cls: "lyr-alox",
            feats: jj_lower,
            summary: "~1.5 nm thermally-grown AlOx · forms 5 Josephson junctions (one per qubit).".into(),
        },
        Lyr {
            id: "al_top".into(),
            name: "JJ Top Electrode".into(),
            material: "Aluminum (Al)".into(),
            thickness_nm: 150.0,
            slab_h: 14.0,
            cls: "lyr-al",
            feats: jj_upper,
            summary: "Manhattan-style Al top electrode completes 5 Al/AlOx/Al transmon junctions.".into(),
        },
        Lyr {
            id: "ab".into(),
            name: "Airbridges".into(),
            material: "Aluminum (Al)".into(),
            thickness_nm: 400.0,
            slab_h: 18.0,
            cls: "lyr-al-ab",
            feats: airbridges,
            summary: "5 × Al airbridges crossing the bus CPW to enforce equipotential ground.".into(),
        },
        Lyr {
            id: "band".into(),
            name: "Bandages".into(),
            material: "Au (passivation patches)".into(),
            thickness_nm: 100.0,
            slab_h: 10.0,
            cls: "lyr-au",
            feats: bandage,
            summary: "Au stitching patches reduce TLS loss at Nb–Al interfaces.".into(),
        },
    ];

    // Vias here represent JJ stack alignment (visual only)
    let vias = vec![
        Via { x: 80.0,  y: 120.0, r: 2.5, from_idx: 2, to_idx: 4, cls: "via-jj" },
        Via { x: 160.0, y: 60.0,  r: 2.5, from_idx: 2, to_idx: 4, cls: "via-jj" },
        Via { x: 240.0, y: 120.0, r: 2.5, from_idx: 2, to_idx: 4, cls: "via-jj" },
        Via { x: 240.0, y: 200.0, r: 2.5, from_idx: 2, to_idx: 4, cls: "via-jj" },
        Via { x: 340.0, y: 120.0, r: 2.5, from_idx: 2, to_idx: 4, cls: "via-jj" },
    ];

    Proj {
        id: "sqc-q5",
        name: "SQC-Q5 · Superconducting Qubit Chip".into(),
        pdk: "Custom · Nb/Al on sapphire".into(),
        die_w: 420.0,
        die_d: 260.0,
        layers,
        vias,
    }
}

fn project_riscv() -> Proj {
    // A small standard-cell-style block: 2 rows of 3 transistors (e.g., NAND2 + INV)
    // PMOS row at y = 80..110, NMOS row at y = 150..180
    // Poly gates vertical at x = 140, 180, 220 (3 fingers)
    let gate_xs = [140.0_f64, 180.0, 220.0];
    let pmos_y = (80.0_f64, 30.0_f64); // (y0, height)
    let nmos_y = (150.0_f64, 30.0_f64);

    // Diffusion (active) layer features
    let diff = vec![
        // PMOS active rail (n-well)
        Feat { x: 120.0, y: pmos_y.0, w: 120.0, d: pmos_y.1, cls: "feat-pdiff", label: Some("p+ S/D in n-well") },
        // NMOS active rail (p-substrate)
        Feat { x: 120.0, y: nmos_y.0, w: 120.0, d: nmos_y.1, cls: "feat-ndiff", label: Some("n+ S/D") },
    ];

    // Poly gates — vertical strips crossing both rails
    let mut poly: Vec<Feat> = Vec::new();
    for (i, gx) in gate_xs.iter().enumerate() {
        poly.push(Feat {
            x: gx - 4.0,
            y: 70.0,
            w: 8.0,
            d: 120.0,
            cls: "feat-poly",
            label: if i == 0 { Some("poly gate (PMOS+NMOS)") } else { None },
        });
    }

    // Contacts at S/D and gate ends
    let mut contacts: Vec<Feat> = Vec::new();
    let sd_xs = [120.0_f64, 160.0, 200.0, 240.0]; // 4 S/D columns between gates
    for sd_x in sd_xs.iter() {
        // PMOS S/D contact
        contacts.push(Feat { x: sd_x - 4.0, y: 90.0, w: 8.0, d: 8.0, cls: "feat-contact", label: None });
        // NMOS S/D contact
        contacts.push(Feat { x: sd_x - 4.0, y: 160.0, w: 8.0, d: 8.0, cls: "feat-contact", label: None });
    }
    // Gate-tap contacts
    for gx in gate_xs.iter() {
        contacts.push(Feat { x: gx - 4.0, y: 64.0, w: 8.0, d: 8.0, cls: "feat-contact-poly", label: None });
    }

    // Metal-1: power rails + intra-cell signal
    let m1 = vec![
        // VDD rail (horizontal, top)
        Feat { x: 80.0, y: 58.0, w: 220.0, d: 10.0, cls: "feat-m1-pwr", label: Some("VDD") },
        // VSS rail (horizontal, bottom)
        Feat { x: 80.0, y: 192.0, w: 220.0, d: 10.0, cls: "feat-m1-pwr", label: Some("VSS") },
        // PMOS source tap to VDD
        Feat { x: 116.0, y: 68.0, w: 12.0, d: 22.0, cls: "feat-m1", label: None },
        Feat { x: 236.0, y: 68.0, w: 12.0, d: 22.0, cls: "feat-m1", label: None },
        // NMOS source tap to VSS
        Feat { x: 116.0, y: 168.0, w: 12.0, d: 24.0, cls: "feat-m1", label: None },
        Feat { x: 236.0, y: 168.0, w: 12.0, d: 24.0, cls: "feat-m1", label: None },
        // Output node strap (between Q rails)
        Feat { x: 196.0, y: 96.0, w: 12.0, d: 70.0, cls: "feat-m1-out", label: Some("Y (output)") },
        // Input gate straps
        Feat { x: 134.0, y: 60.0, w: 14.0, d: 10.0, cls: "feat-m1", label: Some("A") },
        Feat { x: 214.0, y: 60.0, w: 14.0, d: 10.0, cls: "feat-m1", label: Some("B") },
    ];

    // Metal-2: vertical output routing leaving the cell
    let m2 = vec![
        Feat { x: 198.0, y: 30.0, w: 8.0, d: 200.0, cls: "feat-m2", label: Some("Y trunk → M2") },
        Feat { x: 60.0, y: 56.0, w: 260.0, d: 6.0, cls: "feat-m2-pwr", label: Some("VDD strap") },
    ];

    // Metal-3: top-level routing
    let m3 = vec![
        Feat { x: 200.0, y: 30.0, w: 4.0, d: 200.0, cls: "feat-m3", label: Some("Y → far macro") },
        Feat { x: 80.0, y: 100.0, w: 240.0, d: 4.0, cls: "feat-m3", label: Some("clk") },
    ];

    let layers = vec![
        Lyr {
            id: "psub".into(), name: "p-Substrate".into(), material: "p-Si bulk".into(),
            thickness_nm: 750_000.0, slab_h: 28.0, cls: "lyr-sub",
            feats: vec![], summary: "Bulk p-Si wafer · 750 µm.".into(),
        },
        Lyr {
            id: "nwell".into(), name: "n-Well".into(), material: "n-well implant".into(),
            thickness_nm: 1_500.0, slab_h: 14.0, cls: "lyr-well",
            feats: vec![Feat { x: 110.0, y: 70.0, w: 140.0, d: 50.0, cls: "feat-nwell", label: Some("n-well (PMOS host)") }],
            summary: "n-well hosts the PMOS row.".into(),
        },
        Lyr {
            id: "diff".into(), name: "Active / Diffusion".into(), material: "n+/p+ S/D".into(),
            thickness_nm: 200.0, slab_h: 12.0, cls: "lyr-diff",
            feats: diff,
            summary: "6 transistors total · 3 PMOS (p+ in n-well) + 3 NMOS (n+ in p-sub).".into(),
        },
        Lyr {
            id: "poly".into(), name: "Polysilicon (Gate)".into(), material: "doped poly-Si".into(),
            thickness_nm: 180.0, slab_h: 12.0, cls: "lyr-poly",
            feats: poly,
            summary: "3 poly fingers crossing both rails ⇒ 6 transistors (one per finger × row).".into(),
        },
        Lyr {
            id: "li".into(), name: "Local Interconnect / Contact".into(), material: "TiN / W".into(),
            thickness_nm: 250.0, slab_h: 10.0, cls: "lyr-li",
            feats: contacts,
            summary: "8 S/D contacts + 3 gate-tap contacts wire the transistors up to M1.".into(),
        },
        Lyr {
            id: "m1".into(), name: "Metal 1".into(), material: "Copper (Cu)".into(),
            thickness_nm: 220.0, slab_h: 12.0, cls: "lyr-cu",
            feats: m1,
            summary: "VDD/VSS rails + intra-cell straps · 9 segments.".into(),
        },
        Lyr {
            id: "m2".into(), name: "Metal 2".into(), material: "Copper (Cu)".into(),
            thickness_nm: 280.0, slab_h: 12.0, cls: "lyr-cu",
            feats: m2,
            summary: "Vertical output trunk routes Y out of the cell.".into(),
        },
        Lyr {
            id: "m3".into(), name: "Metal 3".into(), material: "Copper (Cu)".into(),
            thickness_nm: 360.0, slab_h: 12.0, cls: "lyr-cu",
            feats: m3,
            summary: "Top-level horizontal routing (clk distribution + far-fanout Y).".into(),
        },
    ];

    // Vias — copper pillars between metal layers (indices into `layers`)
    let vias = vec![
        // LI → M1 (S/D ties)
        Via { x: 122.0, y: 94.0, r: 4.0, from_idx: 4, to_idx: 5, cls: "via-cu" },
        Via { x: 122.0, y: 164.0, r: 4.0, from_idx: 4, to_idx: 5, cls: "via-cu" },
        Via { x: 242.0, y: 94.0, r: 4.0, from_idx: 4, to_idx: 5, cls: "via-cu" },
        Via { x: 242.0, y: 164.0, r: 4.0, from_idx: 4, to_idx: 5, cls: "via-cu" },
        Via { x: 202.0, y: 100.0, r: 4.0, from_idx: 4, to_idx: 5, cls: "via-cu" },
        Via { x: 202.0, y: 160.0, r: 4.0, from_idx: 4, to_idx: 5, cls: "via-cu" },
        // M1 → M2 (output stack)
        Via { x: 202.0, y: 100.0, r: 4.0, from_idx: 5, to_idx: 6, cls: "via-cu" },
        Via { x: 202.0, y: 160.0, r: 4.0, from_idx: 5, to_idx: 6, cls: "via-cu" },
        // M1 → M2 (VDD strap)
        Via { x: 120.0, y: 60.0, r: 4.0, from_idx: 5, to_idx: 6, cls: "via-cu" },
        Via { x: 280.0, y: 60.0, r: 4.0, from_idx: 5, to_idx: 6, cls: "via-cu" },
        // M2 → M3 (Y → far)
        Via { x: 202.0, y: 100.0, r: 4.0, from_idx: 6, to_idx: 7, cls: "via-cu" },
    ];

    Proj {
        id: "riscv-core-v1",
        name: "RISC-V Core (sample cell · NAND2 + INV)".into(),
        pdk: "sky130A · 130 nm".into(),
        die_w: 420.0,
        die_d: 260.0,
        layers,
        vias,
    }
}

fn build_project(id: &str) -> Proj {
    match id {
        "riscv-core-v1" => project_riscv(),
        _ => project_sqc_q5(),
    }
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

#[component]
pub fn SiliconStackupPage() -> impl IntoView {
    let query = use_query_map();
    let initial = query
        .with_untracked(|m| m.get("project").map(|v| v.to_string()))
        .unwrap_or_else(|| "sqc-q5".to_string());

    let project_id = RwSignal::new(initial);
    let explode = RwSignal::new(28.0_f64);
    let show_vias = RwSignal::new(true);
    let show_features = RwSignal::new(true);
    let show_labels = RwSignal::new(true);
    let selected: RwSignal<Option<String>> = RwSignal::new(None);
    let hidden: RwSignal<HashSet<String>> = RwSignal::new(HashSet::new());

    let project = Memo::new(move |_| build_project(&project_id.get()));

    view! {
        <div class="page-container silicon-stack-page">
            <div class="page-header">
                <div>
                    <span class="section-kicker">"Chip Design · 3D Stack-up"</span>
                    <h1>"Layer Stack Viewer"</h1>
                    <p class="page-subtitle">
                        "Isometric view of all process layers, vias, and per-layer features at transistor / unit level."
                    </p>
                </div>
                <div class="stack-toolbar">
                    <label class="fp-toggle">
                        <span class="ttl">"Project"</span>
                        <select
                            class="select-sm"
                            on:change=move |ev| project_id.set(event_target_value(&ev))
                            prop:value=move || project_id.get()
                        >
                            <option value="sqc-q5">"SQC-Q5 · Superconducting Qubit"</option>
                            <option value="riscv-core-v1">"RISC-V Core · CMOS (sky130A)"</option>
                        </select>
                    </label>
                    <label class="fp-toggle">
                        <span class="ttl">"Explode"</span>
                        <input type="range" min="0" max="80" step="1"
                            prop:value=move || explode.get()
                            on:input=move |ev| explode.set(event_target_value(&ev).parse().unwrap_or(28.0))/>
                    </label>
                    <label class="fp-toggle">
                        <input type="checkbox" prop:checked=move || show_vias.get()
                            on:change=move |ev| show_vias.set(event_target_checked(&ev))/>
                        <span>"Vias"</span>
                    </label>
                    <label class="fp-toggle">
                        <input type="checkbox" prop:checked=move || show_features.get()
                            on:change=move |ev| show_features.set(event_target_checked(&ev))/>
                        <span>"Features"</span>
                    </label>
                    <label class="fp-toggle">
                        <input type="checkbox" prop:checked=move || show_labels.get()
                            on:change=move |ev| show_labels.set(event_target_checked(&ev))/>
                        <span>"Labels"</span>
                    </label>
                </div>
            </div>

            <div class="stack-layout">
                <aside class="stack-layer-panel">
                    <div class="fp-inspector-header">
                        {move || format!("{} · {}", project.read().name, project.read().pdk)}
                    </div>
                    <div class="layer-list">
                        {move || {
                            let proj = project.read();
                            proj.layers.iter().enumerate().rev().map(|(i, l)| {
                                let id = l.id.clone();
                                let id_for_toggle = id.clone();
                                let id_for_select = id.clone();
                                let id_for_active = id.clone();
                                let id_for_visible = id.clone();
                                let cls = l.cls;
                                let name = l.name.clone();
                                let material = l.material.clone();
                                let thickness = l.thickness_nm;
                                view! {
                                    <div
                                        class="layer-row"
                                        class:selected=move || selected.get().as_deref() == Some(id_for_active.as_str())
                                        class:hidden=move || hidden.read().contains(&id_for_visible)
                                        on:click=move |_| {
                                            let id = id_for_select.clone();
                                            selected.update(|s| {
                                                if s.as_deref() == Some(id.as_str()) { *s = None; }
                                                else { *s = Some(id); }
                                            });
                                        }
                                    >
                                        <span class=format!("layer-swatch {}", cls)></span>
                                        <div class="layer-row-text">
                                            <strong>{format!("L{} · {}", i, name)}</strong>
                                            <span>{material} " · " {format_thickness(thickness)}</span>
                                        </div>
                                        <button
                                            class="layer-eye"
                                            title="Toggle visibility"
                                            on:click=move |ev| {
                                                ev.stop_propagation();
                                                let id = id_for_toggle.clone();
                                                hidden.update(|h| {
                                                    if !h.insert(id.clone()) { h.remove(&id); }
                                                });
                                            }
                                        >
                                            {move || if hidden.read().contains(&id) { "○" } else { "●" }}
                                        </button>
                                    </div>
                                }
                            }).collect::<Vec<_>>()
                        }}
                    </div>
                </aside>

                <section class="stack-canvas-wrap">
                    <svg viewBox="-560 -60 1120 720" class="stack-svg" preserveAspectRatio="xMidYMid meet">
                        // Backdrop axes for orientation
                        <g class="stack-axes">
                            <line x1="0" y1="0" x2={(150.0_f64 * SQRT3_2).to_string()} y2="75" />
                            <line x1="0" y1="0" x2={(-150.0_f64 * SQRT3_2).to_string()} y2="75" />
                            <line x1="0" y1="0" x2="0" y2="-150" />
                            <text x={(150.0_f64 * SQRT3_2 + 4.0).to_string()} y="80">"x"</text>
                            <text x={(-150.0_f64 * SQRT3_2 - 16.0).to_string()} y="80">"y"</text>
                            <text x="6" y="-150">"z"</text>
                        </g>

                        // Stack
                        {move || {
                            let proj = project.read();
                            let exp = explode.get();
                            let w = proj.die_w;
                            let d = proj.die_d;

                            // Compute z-base for each layer (cumulative)
                            let mut z_bases: Vec<f64> = Vec::with_capacity(proj.layers.len());
                            let mut z = 0.0_f64;
                            for (i, l) in proj.layers.iter().enumerate() {
                                if i > 0 { z += exp; }
                                z_bases.push(z);
                                z += l.slab_h;
                            }

                            let hidden_set = hidden.read().clone();
                            let sel = selected.get();
                            let show_v = show_vias.get();
                            let show_f = show_features.get();
                            let show_l = show_labels.get();

                            let mut svg_children: Vec<leptos::prelude::AnyView> = Vec::new();

                            for (i, layer) in proj.layers.iter().enumerate() {
                                if hidden_set.contains(&layer.id) { continue; }
                                let z0 = z_bases[i];
                                let z1 = z0 + layer.slab_h;
                                let dim = match &sel {
                                    Some(s) if s != &layer.id => "stack-dim",
                                    _ => "",
                                };
                                let cls = layer.cls;
                                let layer_id_click = layer.id.clone();

                                // Slab faces (right and front are darker, top is lighter)
                                svg_children.push(view! {
                                    <g class=format!("stack-layer {} {}", cls, dim)
                                       on:click=move |_| selected.set(Some(layer_id_click.clone()))>
                                        <polygon class=format!("face front {}", cls)
                                            points=slab_front(0.0, 0.0, z0, w, layer.slab_h)/>
                                        <polygon class=format!("face right {}", cls)
                                            points=slab_right(w, 0.0, z0, d, layer.slab_h)/>
                                        <polygon class=format!("face top {}", cls)
                                            points=slab_top(0.0, 0.0, z1, w, d)/>
                                    </g>
                                }.into_any());

                                // Features (on top face)
                                if show_f {
                                    for f in layer.feats.iter() {
                                        let fcls = f.cls;
                                        svg_children.push(view! {
                                            <polygon class=format!("feature {} {}", fcls, dim)
                                                points=slab_top(f.x, f.y, z1 + 0.4, f.w, f.d)/>
                                        }.into_any());
                                        if show_l {
                                            if let Some(lbl) = f.label {
                                                let (lx, ly) = p(f.x + f.w * 0.5, f.y + f.d * 0.5, z1 + 0.4);
                                                svg_children.push(view! {
                                                    <text x=lx y=ly class=format!("feature-label {}", dim)>{lbl}</text>
                                                }.into_any());
                                            }
                                        }
                                    }
                                }

                                // Layer label (front-right corner)
                                if show_l {
                                    let (tx, ty) = p(w + 6.0, 0.0, z0 + layer.slab_h * 0.5);
                                    svg_children.push(view! {
                                        <text x=tx y=ty class=format!("layer-label {}", dim)>
                                            {format!("L{}  {}", i, layer.name)}
                                        </text>
                                    }.into_any());
                                }
                            }

                            // Vias
                            if show_v {
                                for v in proj.vias.iter() {
                                    if v.from_idx >= z_bases.len() || v.to_idx >= z_bases.len() { continue; }
                                    let za = z_bases[v.from_idx] + proj.layers[v.from_idx].slab_h;
                                    let zb = z_bases[v.to_idx] + proj.layers[v.to_idx].slab_h * 0.0;
                                    if zb <= za { continue; }
                                    let r = v.r;
                                    let cls = v.cls;
                                    let h = zb - za;
                                    svg_children.push(view! {
                                        <g class=format!("via {}", cls)>
                                            <polygon class="via-face front"
                                                points=slab_front(v.x - r, v.y - r, za, 2.0*r, h)/>
                                            <polygon class="via-face right"
                                                points=slab_right(v.x + r, v.y - r, za, 2.0*r, h)/>
                                            <polygon class="via-face top"
                                                points=slab_top(v.x - r, v.y - r, zb, 2.0*r, 2.0*r)/>
                                        </g>
                                    }.into_any());
                                }
                            }

                            svg_children
                        }}
                    </svg>

                    <div class="fp-legend stack-legend">
                        <span class="legend-item"><span class="dot lyr-sub"></span>"Substrate"</span>
                        <span class="legend-item"><span class="dot lyr-well"></span>"Well / implant"</span>
                        <span class="legend-item"><span class="dot lyr-diff"></span>"Diffusion (S/D)"</span>
                        <span class="legend-item"><span class="dot lyr-poly"></span>"Polysilicon gate"</span>
                        <span class="legend-item"><span class="dot lyr-li"></span>"Contact / LI"</span>
                        <span class="legend-item"><span class="dot lyr-cu"></span>"Copper (M1–M3)"</span>
                        <span class="legend-item"><span class="dot lyr-nb"></span>"Niobium GP"</span>
                        <span class="legend-item"><span class="dot lyr-al"></span>"Aluminum"</span>
                        <span class="legend-item"><span class="dot lyr-alox"></span>"AlOx / JJ"</span>
                        <span class="legend-item"><span class="dot lyr-au"></span>"Au bandage"</span>
                    </div>
                </section>

                <aside class="stack-inspector">
                    <div class="fp-inspector-header">"Layer Inspector"</div>
                    {move || {
                        let proj = project.read();
                        let sel = selected.get();
                        match sel.as_ref().and_then(|id| proj.layers.iter().enumerate().find(|(_, l)| &l.id == id)) {
                            Some((idx, l)) => {
                                let feats_n = l.feats.len();
                                view! {
                                    <div class="fp-props">
                                        <div class="prop"><span>"Layer"</span><strong>{format!("L{} · {}", idx, l.name.clone())}</strong></div>
                                        <div class="prop"><span>"Material"</span><strong>{l.material.clone()}</strong></div>
                                        <div class="prop"><span>"Thickness"</span><strong>{format_thickness(l.thickness_nm)}</strong></div>
                                        <div class="prop"><span>"Features"</span><strong>{format!("{} unit{}", feats_n, if feats_n == 1 { "" } else { "s" })}</strong></div>
                                    </div>
                                    <p class="layer-summary">{l.summary.clone()}</p>
                                    <div class="fp-inspector-header">"Features"</div>
                                    <ul class="feature-list">
                                        {l.feats.iter().enumerate().map(|(fi, f)| view! {
                                            <li>
                                                <span class=format!("layer-swatch {}", f.cls)></span>
                                                <span class="feature-id">{format!("F{:02}", fi)}</span>
                                                <span class="feature-meta">
                                                    {f.label.unwrap_or("unit")}
                                                    " · "
                                                    {format!("{:.0}×{:.0}", f.w, f.d)}
                                                </span>
                                            </li>
                                        }).collect::<Vec<_>>()}
                                    </ul>
                                }.into_any()
                            }
                            None => view! {
                                <p class="empty-state">"Click a layer slab or a row to inspect its units."</p>
                                <div class="fp-props">
                                    <div class="prop"><span>"Project"</span><strong>{proj.name.clone()}</strong></div>
                                    <div class="prop"><span>"PDK"</span><strong>{proj.pdk.clone()}</strong></div>
                                    <div class="prop"><span>"Layers"</span><strong>{proj.layers.len()}</strong></div>
                                    <div class="prop"><span>"Vias"</span><strong>{proj.vias.len()}</strong></div>
                                </div>
                            }.into_any(),
                        }
                    }}
                </aside>
            </div>
        </div>

        <style>{r#"
            .silicon-stack-page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
            .silicon-stack-page .page-header { display: flex; justify-content: space-between; align-items: flex-start; gap: 1rem; flex-wrap: wrap; }
            .silicon-stack-page .page-header h1 { margin: 0; }
            .silicon-stack-page .page-subtitle { margin: 0.35rem 0 0; color: var(--text-secondary); max-width: 780px; }
            .stack-toolbar { display: flex; gap: 0.75rem; align-items: center; flex-wrap: wrap; }
            .stack-toolbar .ttl { color: var(--text-muted); font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.07em; }
            .stack-toolbar .fp-toggle { display: inline-flex; gap: 0.4rem; align-items: center; color: var(--text-secondary); font-size: 0.85rem; }
            .stack-toolbar input[type="range"] { width: 140px; }
            .select-sm {
                background: var(--bg-secondary); color: var(--text-primary);
                border: 1px solid var(--border-color, var(--border));
                border-radius: 6px; padding: 0.25rem 0.5rem; font-size: 0.85rem;
            }

            .stack-layout {
                display: grid;
                grid-template-columns: 260px minmax(0,1fr) 300px;
                gap: 0.75rem;
                min-height: 70vh;
            }
            .stack-layer-panel, .stack-canvas-wrap, .stack-inspector {
                background: var(--card-bg, var(--bg-card));
                border: 1px solid var(--border-color, var(--border));
                border-radius: 10px;
                padding: 0.85rem;
            }

            .fp-inspector-header {
                font-size: 0.75rem; color: var(--text-muted);
                text-transform: uppercase; letter-spacing: 0.08em;
                margin: 0.2rem 0 0.5rem;
            }
            .fp-props { display: grid; gap: 0.35rem; }
            .prop { display: flex; justify-content: space-between; gap: 0.5rem; font-size: 0.85rem; }
            .prop span { color: var(--text-muted); }
            .prop strong { color: var(--text-primary); font-weight: 600; }

            .layer-list { display: flex; flex-direction: column; gap: 0.25rem; }
            .layer-row {
                display: grid; grid-template-columns: 14px 1fr 22px; gap: 0.55rem; align-items: center;
                padding: 0.45rem 0.55rem;
                background: var(--bg-secondary);
                border: 1px solid var(--border-color, var(--border));
                border-radius: 8px;
                cursor: pointer;
            }
            .layer-row:hover { border-color: rgba(59,130,246,0.5); }
            .layer-row.selected { border-color: var(--accent, #3b82f6); box-shadow: inset 0 0 0 1px var(--accent, #3b82f6); }
            .layer-row.hidden { opacity: 0.45; }
            .layer-row-text strong { display: block; font-size: 0.82rem; color: var(--text-primary); }
            .layer-row-text span { color: var(--text-muted); font-size: 0.72rem; }
            .layer-swatch { width: 14px; height: 14px; border-radius: 3px; display: inline-block; border: 1px solid rgba(255,255,255,0.15); }
            .layer-eye {
                background: transparent; color: var(--text-muted); border: none;
                font-size: 0.95rem; cursor: pointer; padding: 0;
            }
            .layer-eye:hover { color: var(--text-primary); }

            .stack-svg { width: 100%; height: 100%; min-height: 520px; display: block; background: #0a1226; border-radius: 6px; }
            .stack-axes line { stroke: rgba(148,163,184,0.3); stroke-width: 1; stroke-dasharray: 3 3; }
            .stack-axes text { fill: rgba(148,163,184,0.7); font-size: 11px; font-family: var(--font-mono); }

            .stack-layer { cursor: pointer; }
            .face { stroke: rgba(15,23,42,0.6); stroke-width: 0.5; }
            .face.front { filter: brightness(0.78); }
            .face.right { filter: brightness(0.62); }
            .stack-dim { opacity: 0.25; }
            .stack-dim text { opacity: 0.4; }

            /* Layer slab fill colours (top face) */
            .lyr-sub  { fill: #5b6470; }
            .lyr-well { fill: #6b7280; }
            .lyr-diff { fill: #16a34a; }
            .lyr-poly { fill: #ef4444; }
            .lyr-li   { fill: #94a3b8; }
            .lyr-cu   { fill: #fb923c; }
            .lyr-nb   { fill: #38bdf8; }
            .lyr-al   { fill: #cbd5e1; }
            .lyr-al-ab{ fill: #e2e8f0; }
            .lyr-alox { fill: #facc15; }
            .lyr-au   { fill: #fde68a; }

            /* Feature fills (drawn slightly above slab top) */
            .feature { stroke: rgba(15,23,42,0.85); stroke-width: 0.4; }
            .feat-nwell    { fill: rgba(99,102,241,0.55); }
            .feat-pdiff    { fill: #15803d; }
            .feat-ndiff    { fill: #22c55e; }
            .feat-poly     { fill: #dc2626; }
            .feat-contact  { fill: #475569; stroke: #e2e8f0; }
            .feat-contact-poly { fill: #7c3aed; stroke: #e2e8f0; }
            .feat-m1       { fill: #f97316; }
            .feat-m1-pwr   { fill: #ea580c; }
            .feat-m1-out   { fill: #fdba74; }
            .feat-m2       { fill: #f59e0b; }
            .feat-m2-pwr   { fill: #d97706; }
            .feat-m3       { fill: #fbbf24; }

            .feat-xmon-data { fill: rgba(59,130,246,0.85); }
            .feat-xmon-anc  { fill: rgba(168,85,247,0.85); }
            .feat-pad       { fill: #fde68a; }
            .feat-jj        { fill: #facc15; stroke: #fef3c7; }
            .feat-jj-top    { fill: #f59e0b; }
            .feat-bandage   { fill: #fde68a; }
            .feat-airbridge { fill: #e2e8f0; }
            .feat-cpw       { fill: #06b6d4; }
            .feat-readout   { fill: #22c55e; }

            /* Vias */
            .via-face { stroke: rgba(15,23,42,0.7); stroke-width: 0.4; }
            .via-cu .via-face { fill: #fb923c; }
            .via-cu .via-face.front { filter: brightness(0.7); }
            .via-cu .via-face.right { filter: brightness(0.55); }
            .via-jj .via-face { fill: #facc15; }
            .via-jj .via-face.front { filter: brightness(0.7); }
            .via-jj .via-face.right { filter: brightness(0.55); }

            .layer-label { fill: #cbd5e1; font-size: 11px; font-family: var(--font-mono); pointer-events: none; }
            .feature-label { fill: #f1f5f9; font-size: 9px; font-family: var(--font-mono); pointer-events: none; text-anchor: middle; }

            .stack-legend { margin-top: 0.6rem; }
            .stack-legend .dot { width: 12px; height: 12px; border-radius: 3px; display: inline-block; }

            .layer-summary { margin: 0.5rem 0 0.75rem; color: var(--text-secondary); font-size: 0.85rem; }
            .feature-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.3rem; max-height: 240px; overflow: auto; }
            .feature-list li {
                display: grid; grid-template-columns: 14px 38px 1fr; gap: 0.4rem; align-items: center;
                padding: 0.3rem 0.45rem;
                background: var(--bg-secondary);
                border-radius: 6px;
                font-size: 0.78rem;
            }
            .feature-id { color: var(--text-muted); font-family: var(--font-mono); }
            .feature-meta { color: var(--text-primary); }

            @media (max-width: 1200px) {
                .stack-layout { grid-template-columns: 220px minmax(0,1fr); }
                .stack-inspector { grid-column: 1 / -1; }
            }
            @media (max-width: 800px) {
                .stack-layout { grid-template-columns: 1fr; }
            }
        "#}</style>
    }
}

fn format_thickness(nm: f64) -> String {
    if nm >= 1_000_000.0 {
        format!("{:.0} µm", nm / 1_000.0)
    } else if nm >= 1_000.0 {
        format!("{:.1} µm", nm / 1_000.0)
    } else if nm >= 10.0 {
        format!("{:.0} nm", nm)
    } else {
        format!("{:.1} nm", nm)
    }
}
