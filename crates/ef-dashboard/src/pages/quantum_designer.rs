use leptos::prelude::*;
use gloo_storage::Storage as _;
use serde_json::json;

use crate::api::{self, QHistogramBin, QSimulateRequest, QSimulateResponse};

/// Responsive SVG column chart of the measurement distribution.
fn histogram_chart(bins: &[QHistogramBin]) -> AnyView {
    if bins.is_empty() {
        return view! { <p class="hint">"No measurement outcomes returned."</p> }.into_any();
    }
    let mut sorted = bins.to_vec();
    sorted.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.state.cmp(&b.state)));
    let n = sorted.len();
    let max_count = sorted.iter().map(|b| b.count).max().unwrap_or(1).max(1) as f64;
    let w = ((n * 84) as f64).clamp(320.0, 980.0);
    let h = 220.0;
    let (pad_l, pad_r, pad_t, pad_b) = (44.0, 12.0, 26.0, 44.0);
    let plot_w = w - pad_l - pad_r;
    let plot_h = h - pad_t - pad_b;
    let slot = plot_w / n as f64;
    let bar_w = (slot * 0.62).max(8.0);
    let rotate_labels = n > 8;

    let gridlines = [0.0, 0.25, 0.5, 0.75, 1.0]
        .into_iter()
        .map(|frac| {
            let y = pad_t + plot_h * (1.0 - frac);
            let label = format!("{}", (max_count * frac).round() as u64);
            view! {
                <g>
                    <line x1={pad_l} y1={y} x2={w - pad_r} y2={y} class="chart-grid"/>
                    <text x={pad_l - 6.0} y={y + 4.0} text-anchor="end" class="chart-tick">{label}</text>
                </g>
            }
        })
        .collect::<Vec<_>>();

    let bars = sorted
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let frac = b.count as f64 / max_count;
            let bh = (plot_h * frac).max(2.0);
            let x = pad_l + slot * i as f64 + (slot - bar_w) / 2.0;
            let y = pad_t + plot_h - bh;
            let pct = format!("{:.1}%", b.probability * 100.0);
            let tip = format!("{}: {} shots ({})", b.state, b.count, pct);
            let label_y = y - 6.0;
            let lx = x + bar_w / 2.0;
            let state = b.state.clone();
            view! {
                <g>
                    <title>{tip}</title>
                    <rect x={x} y={y} width={bar_w} height={bh} rx="3" class="chart-bar"/>
                    <text x={lx} y={label_y} text-anchor="middle" class="chart-value">{b.count.to_string()}</text>
                    {if rotate_labels {
                        view! {
                            <text
                                x={lx + 4.0}
                                y={pad_t + plot_h + 8.0}
                                text-anchor="start"
                                class="chart-label chart-label-rot"
                                transform={format!("rotate(35 {lx} {})", pad_t + plot_h + 8.0)}
                            >
                                {state}
                            </text>
                        }
                        .into_any()
                    } else {
                        view! {
                            <text x={lx} y={pad_t + plot_h + 18.0} text-anchor="middle" class="chart-label">
                                {state}
                            </text>
                        }
                        .into_any()
                    }}
                </g>
            }
        })
        .collect::<Vec<_>>();

    view! {
        <div class="chart-wrap">
            <svg
                viewBox={format!("0 0 {w} {h}")}
                class="histogram-chart"
                style=format!("min-width: {w}px;")
                role="img"
                aria-label="Measurement outcome distribution chart"
            >
                {gridlines}
                // x axis
                <line x1={pad_l} y1={pad_t + plot_h} x2={w - pad_r} y2={pad_t + plot_h} class="chart-axis"/>
                {bars}
            </svg>
        </div>
    }
    .into_any()
}

// ── Template library ─────────────────────────────────────────────────────
// Every template is plain OpenQASM 2.0 using only gates the VeerOS
// state-vector backend executes (others are decomposed server-side).
// Each listing below is covered by a simulation test in
// `crates/ef-control/src/command/veeros.rs`.

struct QuantumTemplate {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    qubits: usize,
    qasm: &'static str,
}

const TEMPLATES: &[QuantumTemplate] = &[
    QuantumTemplate {
        id: "bell",
        name: "Bell State",
        description: "Maximally entangled 2-qubit pair — the hello-world of quantum.",
        qubits: 2,
        qasm: "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[2];\ncreg c[2];\nh q[0];\ncx q[0],q[1];\nmeasure q -> c;",
    },
    QuantumTemplate {
        id: "ghz3",
        name: "GHZ-3",
        description: "3-qubit Greenberger–Horne–Zeilinger state.",
        qubits: 3,
        qasm: "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[3];\ncreg c[3];\nh q[0];\ncx q[0],q[1];\ncx q[1],q[2];\nmeasure q -> c;",
    },
    QuantumTemplate {
        id: "ghz5",
        name: "GHZ-5",
        description: "5-qubit GHZ — multi-party entanglement at larger width.",
        qubits: 5,
        qasm: "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[5];\ncreg c[5];\nh q[0];\ncx q[0],q[1];\ncx q[1],q[2];\ncx q[2],q[3];\ncx q[3],q[4];\nmeasure q -> c;",
    },
    QuantumTemplate {
        id: "qft3",
        name: "QFT-3",
        description: "3-qubit Quantum Fourier Transform with controlled phases.",
        qubits: 3,
        qasm: "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[3];\ncreg c[3];\nh q[0];\ncp(pi/2) q[0],q[1];\ncp(pi/4) q[0],q[2];\nh q[1];\ncp(pi/2) q[1],q[2];\nh q[2];\nmeasure q -> c;",
    },
    QuantumTemplate {
        id: "qft5",
        name: "QFT-5",
        description: "5-qubit QFT — the core subroutine behind Shor and QPE.",
        qubits: 5,
        qasm: "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[5];\ncreg c[5];\nh q[0];\ncp(pi/2) q[0],q[1];\ncp(pi/4) q[0],q[2];\ncp(pi/8) q[0],q[3];\ncp(pi/16) q[0],q[4];\nh q[1];\ncp(pi/2) q[1],q[2];\ncp(pi/4) q[1],q[3];\ncp(pi/8) q[1],q[4];\nh q[2];\ncp(pi/2) q[2],q[3];\ncp(pi/4) q[2],q[4];\nh q[3];\ncp(pi/2) q[3],q[4];\nh q[4];\nmeasure q -> c;",
    },
    QuantumTemplate {
        id: "superdense",
        name: "Superdense Coding",
        description: "Transmit two classical bits (10) with one qubit + entanglement.",
        qubits: 2,
        qasm: "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[2];\ncreg c[2];\nh q[0];\ncx q[0],q[1];\nx q[0];\ncx q[0],q[1];\nh q[0];\nmeasure q -> c;",
    },
    QuantumTemplate {
        id: "grover2",
        name: "Grover Search (2q)",
        description: "Amplitude amplification finds the marked state |11> in one iteration.",
        qubits: 2,
        qasm: "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[2];\ncreg c[2];\nh q[0];\nh q[1];\nh q[1];\ncx q[0],q[1];\nh q[1];\nh q[0];\nh q[1];\nx q[0];\nx q[1];\nh q[1];\ncx q[0],q[1];\nh q[1];\nx q[0];\nx q[1];\nh q[0];\nh q[1];\nmeasure q -> c;",
    },
    QuantumTemplate {
        id: "deutsch-jozsa",
        name: "Deutsch–Jozsa",
        description: "One query decides constant vs balanced for f(x) = x0.",
        qubits: 3,
        qasm: "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[3];\ncreg c[3];\nx q[2];\nh q[0];\nh q[1];\nh q[2];\ncx q[0],q[2];\nh q[0];\nh q[1];\nmeasure q[0] -> c[0];\nmeasure q[1] -> c[1];\nmeasure q[2] -> c[2];",
    },
    QuantumTemplate {
        id: "bernstein-vazirani",
        name: "Bernstein–Vazirani",
        description: "Recovers hidden string s = 11 in a single query.",
        qubits: 3,
        qasm: "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[3];\ncreg c[3];\nx q[2];\nh q[0];\nh q[1];\nh q[2];\ncx q[0],q[2];\ncx q[1],q[2];\nh q[0];\nh q[1];\nmeasure q[0] -> c[0];\nmeasure q[1] -> c[1];\nmeasure q[2] -> c[2];",
    },
    QuantumTemplate {
        id: "vqe-h2",
        name: "VQE Ansatz (H₂)",
        description: "Hardware-efficient variational ansatz, 2 qubits, fixed angles.",
        qubits: 2,
        qasm: "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[2];\ncreg c[2];\nry(0.5) q[0];\nry(0.5) q[1];\ncx q[0],q[1];\nry(0.5) q[0];\nry(0.5) q[1];\nmeasure q -> c;",
    },
    QuantumTemplate {
        id: "teleport",
        name: "Teleportation",
        description: "Teleport |1> across a Bell pair — deferred-measurement form, q2 reads 1 every shot.",
        qubits: 3,
        qasm: "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[3];\ncreg c[3];\nx q[0];\nh q[1];\ncx q[1],q[2];\ncx q[0],q[1];\nh q[0];\ncx q[1],q[2];\ncz q[0],q[2];\nmeasure q -> c;",
    },
    QuantumTemplate {
        id: "qec-bitflip",
        name: "Bit-Flip Code + Correction",
        description: "3-qubit repetition code corrects an X error via Toffoli.",
        qubits: 5,
        qasm: "OPENQASM 2.0;\ninclude \"qelib1.inc\";\nqreg q[5];\ncreg c[5];\nx q[0];\ncx q[0],q[1];\ncx q[0],q[2];\nx q[1];\ncx q[0],q[3];\ncx q[1],q[3];\ncx q[1],q[4];\ncx q[2],q[4];\nccx q[3],q[4],q[1];\ncx q[0],q[1];\ncx q[0],q[2];\nmeasure q[0] -> c[0];",
    },
];

fn find_template(id: &str) -> Option<&'static QuantumTemplate> {
    TEMPLATES.iter().find(|t| t.id == id)
}

// ── QASM → op list (shared by the visual grid and Qiskit codegen) ──────────

#[derive(Clone, Debug)]
struct ParsedOp {
    name: String,
    params: String,
    qubits: Vec<usize>,
    cbit: Option<usize>,
}

fn parse_qasm_ops(src: &str) -> (usize, Vec<ParsedOp>) {
    let mut n_qubits = 0usize;
    let mut ops = Vec::new();
    for line in src.lines() {
        let t = line.trim().trim_end_matches(';').trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with("OPENQASM") || t.starts_with("include") {
            continue;
        }
        if let Some(rest) = t.strip_prefix("qreg").or_else(|| t.strip_prefix("creg")) {
            if let (Some(a), Some(b)) = (rest.find('['), rest.find(']')) {
                if let Ok(n) = rest[a + 1..b].trim().parse::<usize>() {
                    if t.starts_with("qreg") {
                        n_qubits = n_qubits.max(n);
                    }
                }
            }
            continue;
        }
        if t.starts_with("qubit[") || t.starts_with("bit[") {
            continue;
        }
        if t == "barrier" || t.starts_with("barrier ") || t.starts_with("barrier;") {
            ops.push(ParsedOp { name: "barrier".into(), params: String::new(), qubits: vec![], cbit: None });
            continue;
        }
        // name(params) operands
        let (name, params, operands) = if let Some(p) = t.find('(') {
            let name = t[..p].trim().to_string();
            let after = &t[p + 1..];
            match after.find(')') {
                Some(c) => (name, after[..c].to_string(), after[c + 1..].trim().to_string()),
                None => (name, String::new(), after.to_string()),
            }
        } else {
            let mut it = t.splitn(2, char::is_whitespace);
            (
                it.next().unwrap_or("").to_string(),
                String::new(),
                it.next().unwrap_or("").trim().to_string(),
            )
        };
        if name.is_empty() {
            continue;
        }
        if name == "barrier" {
            ops.push(ParsedOp { name, params: String::new(), qubits: vec![], cbit: None });
            continue;
        }
        let lname = name.to_lowercase();
        if lname == "measure" {
            let qs = extract_q_indices(&operands);
            let cbit = extract_c_index(&operands);
            ops.push(ParsedOp { name: "measure".into(), params: String::new(), qubits: qs, cbit });
            continue;
        }
        let qs = extract_q_indices(&operands);
        if qs.is_empty() {
            continue;
        }
        ops.push(ParsedOp { name: lname, params, qubits: qs, cbit: None });
    }
    if n_qubits == 0 {
        n_qubits = ops
            .iter()
            .flat_map(|o| o.qubits.iter())
            .max()
            .map(|m| m + 1)
            .unwrap_or(2);
    }
    (n_qubits, ops)
}

fn extract_q_indices(s: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b'q' && b[i + 1] == b'[' {
            if let Some(end) = s[i..].find(']') {
                if let Ok(idx) = s[i + 2..i + end].parse::<usize>() {
                    out.push(idx);
                }
                i += end + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn extract_c_index(s: &str) -> Option<usize> {
    let arrow = s.find("->")?;
    let after = s[arrow + 2..].trim().trim_start_matches('c').trim();
    let after = after.strip_prefix('[').unwrap_or(after);
    let end = after.find(']').unwrap_or(after.len());
    after[..end].trim().parse::<usize>().ok()
}

// ── Qiskit codegen (QASM → runnable Python; runs in the user's Qiskit env) ─

fn qasm_to_qiskit(src: &str, shots: u32, seed: Option<u64>) -> String {
    let (n, ops) = parse_qasm_ops(src);
    let mut out = String::new();
    out.push_str("# Generated by EdgeFabric Quantum Designer — run with Qiskit Aer\n");
    out.push_str("#   pip install qiskit qiskit-aer matplotlib\n");
    out.push_str("from qiskit import QuantumCircuit\n");
    out.push_str("from qiskit_aer import AerSimulator\n\n");
    out.push_str(&format!("qc = QuantumCircuit({n}, {n})\n"));
    for op in &ops {
        let q = |i: usize| op.qubits.get(i).copied().unwrap_or(0).to_string();
        let line = match op.name.as_str() {
            "h" | "x" | "y" | "z" | "s" | "t" | "id" => format!("qc.{}({})", op.name, q(0)),
            "sdg" => format!("qc.sdg({})", q(0)),
            "tdg" => format!("qc.tdg({})", q(0)),
            "rx" | "ry" | "rz" => format!("qc.{}({}, {})", op.name, op.params.trim(), q(0)),
            "u" | "u3" => format!("qc.u({}, {})", op.params.trim(), q(0)),
            "cx" | "cnot" => format!("qc.cx({}, {})", q(0), q(1)),
            "cy" => format!("qc.cy({}, {})", q(0), q(1)),
            "cz" => format!("qc.cz({}, {})", q(0), q(1)),
            "swap" => format!("qc.swap({}, {})", q(0), q(1)),
            "ccx" | "toffoli" => format!("qc.ccx({}, {}, {})", q(0), q(1), q(2)),
            "cp" | "cu1" | "cphase" => format!("qc.cp({}, {}, {})", op.params.trim(), q(0), q(1)),
            "crz" => format!("qc.crz({}, {}, {})", op.params.trim(), q(0), q(1)),
            "barrier" => format!("qc.barrier({})", (0..n).map(|i| i.to_string()).collect::<Vec<_>>().join(", ")),
            "measure" => {
                if op.qubits.is_empty() {
                    "qc.measure_all()".to_string()
                } else {
                    let c = op.cbit.unwrap_or(op.qubits[0]);
                    format!("qc.measure({}, {})", q(0), c)
                }
            }
            other => format!("# unsupported for Qiskit export: {other}"),
        };
        out.push_str(&line);
        out.push('\n');
    }
    out.push('\n');
    out.push_str(&format!("shots = {shots}\n"));
    match seed {
        Some(s) => out.push_str(&format!(
            "result = AerSimulator().run(qc, shots=shots, seed_simulator={s}).result()\n"
        )),
        None => out.push_str("result = AerSimulator().run(qc, shots=shots).result()\n"),
    }
    out.push_str("counts = result.get_counts()\nprint(counts)\n");
    out.push_str("# from qiskit.visualization import plot_histogram\n# plot_histogram(counts).show()\n");
    out
}

// ── Visual grid layout ─────────────────────────────────────────────────────

#[derive(Clone)]
struct GridCell {
    /// Display text (gate label, control dot, target cross, …).
    text: String,
    /// CSS class suffix: box | ctrl | targ | swap | meas | link | empty | barrier.
    kind: &'static str,
}

fn empty_cell() -> GridCell {
    GridCell { text: String::new(), kind: "empty" }
}

fn place_col(
    cols: &mut Vec<Vec<GridCell>>,
    cols_op: &mut Vec<Vec<Option<usize>>>,
    next_col: &mut [usize],
    n: usize,
    wires: &[usize],
    cells: Vec<(String, &'static str)>,
    op_idx: Option<usize>,
) {
    let col = wires.iter().map(|&w| next_col[w.min(n - 1)]).max().unwrap_or(0);
    while cols.len() <= col {
        cols.push(vec![empty_cell(); n]);
        cols_op.push(vec![None; n]);
    }
    for (i, w) in wires.iter().enumerate() {
        let w = (*w).min(n - 1);
        let (text, kind) = cells.get(i).cloned().unwrap_or((String::new(), "link"));
        cols[col][w] = GridCell { text, kind };
        // Only the wires named by the gate own the op (links don't).
        cols_op[col][w] = if i < cells.len() { op_idx } else { None };
        next_col[w] = col + 1;
    }
    // Fill intermediate wires with a vertical link.
    if wires.len() > 1 {
        let lo = wires.iter().min().copied().unwrap_or(0).min(n - 1);
        let hi = wires.iter().max().copied().unwrap_or(0).min(n - 1);
        for w in lo..=hi {
            if cols[col][w].kind == "empty" {
                cols[col][w] = GridCell { text: String::new(), kind: "link" };
            }
        }
    }
}

/// A rendered cell: what to draw, where it sits, and which op it belongs to
/// (for click-to-remove).
#[derive(Clone)]
struct RenderCell {
    cell: GridCell,
    col: usize,
    op_idx: Option<usize>,
}

fn layout_grid(n_qubits: usize, ops: &[ParsedOp]) -> (Vec<Vec<RenderCell>>, Vec<usize>) {
    let mut cols: Vec<Vec<GridCell>> = Vec::new();
    let mut cols_op: Vec<Vec<Option<usize>>> = Vec::new();
    let mut next_col = vec![0usize; n_qubits.max(1)];
    let mut op_cols = vec![0usize; ops.len()];
    let n = n_qubits.max(1);
    let all_wires: Vec<usize> = (0..n).collect();

    // Local helper macro to keep call sites short.
    macro_rules! put {
        ($wires:expr, $cells:expr, $idx:expr) => {{
            let ws: &[usize] = $wires;
            let c = ws.iter().map(|&w| next_col[w.min(n - 1)]).max().unwrap_or(0);
            place_col(&mut cols, &mut cols_op, &mut next_col, n, ws, $cells, Some($idx));
            op_cols[$idx] = c;
        }};
    }

    for (idx, op) in ops.iter().enumerate() {
        match op.name.as_str() {
            "barrier" => {
                put!(&all_wires, vec![(String::new(), "barrier"); n], idx);
            }
            "measure" => {
                if op.qubits.is_empty() {
                    put!(&all_wires, vec![("M".to_string(), "meas"); n], idx);
                } else {
                    put!(&op.qubits.clone(), vec![("M".to_string(), "meas")], idx);
                }
            }
            "cx" | "cnot" | "cy" | "cz" => {
                if op.qubits.len() >= 2 {
                    put!(
                        &op.qubits[..2].to_vec(),
                        vec![("●".to_string(), "ctrl"), ("⊕".to_string(), "targ")],
                        idx
                    );
                }
            }
            "cp" | "cu1" | "cphase" | "crz" => {
                if op.qubits.len() >= 2 {
                    let label = if op.params.trim().is_empty() {
                        "P".to_string()
                    } else {
                        format!("P({})", short_param(&op.params))
                    };
                    put!(&op.qubits[..2].to_vec(), vec![("●".to_string(), "ctrl"), (label, "pbox")], idx);
                }
            }
            "swap" => {
                if op.qubits.len() >= 2 {
                    put!(
                        &op.qubits[..2].to_vec(),
                        vec![("×".to_string(), "swap"), ("×".to_string(), "swap")],
                        idx
                    );
                }
            }
            "ccx" | "toffoli" => {
                if op.qubits.len() >= 3 {
                    put!(
                        &op.qubits[..3].to_vec(),
                        vec![
                            ("●".to_string(), "ctrl"),
                            ("●".to_string(), "ctrl"),
                            ("⊕".to_string(), "targ"),
                        ],
                        idx
                    );
                }
            }
            _ => {
                if op.qubits.len() == 1 {
                    let label = if op.params.trim().is_empty() {
                        op.name.to_uppercase()
                    } else {
                        format!("{}({})", op.name.to_uppercase(), short_param(&op.params))
                    };
                    put!(&op.qubits.clone(), vec![(label, "box")], idx);
                }
            }
        }
    }

    // Transpose to rows for rendering.
    let rows = (0..n)
        .map(|w| {
            cols.iter()
                .enumerate()
                .map(|(c, col)| RenderCell {
                    cell: col[w].clone(),
                    col: c,
                    op_idx: cols_op[c][w],
                })
                .collect::<Vec<_>>()
        })
        .collect();
    (rows, op_cols)
}

fn short_param(p: &str) -> String {
    let t = p.trim();
    if t.len() > 8 {
        format!("{}…", &t[..7])
    } else {
        t.to_string()
    }
}

// ── QASM regen (visual edits → canonical source) ───────────────────────────

fn op_to_qasm_line(op: &ParsedOp, nq: usize) -> String {
    let wires = op.qubits.iter().map(|w| format!("q[{w}]")).collect::<Vec<_>>().join(",");
    match op.name.as_str() {
        "measure" => {
            if op.qubits.is_empty() {
                "measure q -> c;".to_string()
            } else {
                format!("measure q[{}] -> c[{}];", op.qubits[0], op.cbit.unwrap_or(op.qubits[0]))
            }
        }
        "barrier" => {
            let all = (0..nq.max(1)).map(|w| format!("q[{w}]")).collect::<Vec<_>>().join(",");
            format!("barrier {all};")
        }
        _ if op.params.trim().is_empty() => format!("{} {wires};", op.name),
        _ => format!("{}({}) {wires};", op.name, op.params.trim()),
    }
}

fn ops_to_qasm(nq: usize, ops: &[ParsedOp]) -> String {
    let mut out = String::from("OPENQASM 2.0;\ninclude \"qelib1.inc\";\n");
    out.push_str(&format!("qreg q[{nq}];\ncreg c[{nq}];\n"));
    for op in ops {
        out.push_str(&op_to_qasm_line(op, nq));
        out.push('\n');
    }
    out
}

/// Insert an op so it serializes at visual column `col`.
fn insert_op_at(ops: &mut Vec<ParsedOp>, op_cols: &[usize], col: usize, op: ParsedOp) {
    let pos = op_cols.iter().position(|&c| c > col).unwrap_or(ops.len());
    ops.insert(pos.min(ops.len()), op);
}

/// How many distinct wires a palette gate needs (barrier/measure handled separately).
fn gate_arity(name: &str) -> usize {
    match name {
        "cx" | "cnot" | "cy" | "cz" | "swap" | "cp" | "cu1" | "cphase" | "crz" => 2,
        "ccx" | "toffoli" => 3,
        _ => 1,
    }
}

/// Default angle text for parametric gates placed from the palette.
fn default_params(name: &str) -> String {
    match name {
        "rx" | "ry" | "rz" | "cp" | "cu1" | "cphase" | "crz" => "pi/2".to_string(),
        "u" | "u3" => "pi/2,0,pi".to_string(),
        _ => String::new(),
    }
}

const KNOWN_GATES: &[&str] = &[
    "h", "x", "y", "z", "s", "sdg", "si", "t", "tdg", "ti", "rx", "ry", "rz", "u", "u3",
    "cx", "cnot", "cy", "cz", "swap", "ccx", "toffoli", "cp", "cu1", "cphase", "crz",
    "id", "barrier", "measure",
];

/// Lines the simulator will silently skip — surfaced as a lint warning.
fn lint_qasm(src: &str) -> Vec<String> {
    let mut skipped = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let t = line.trim().trim_end_matches(';').trim();
        if t.is_empty()
            || t.starts_with("//")
            || t.starts_with("OPENQASM")
            || t.starts_with("include")
            || t.starts_with("qreg")
            || t.starts_with("creg")
            || t.starts_with("qubit[")
            || t.starts_with("bit[")
        {
            continue;
        }
        let name = t
            .split(|c: char| c == '(' || c.is_whitespace())
            .next()
            .unwrap_or("")
            .to_lowercase();
        if !KNOWN_GATES.contains(&name.as_str()) {
            skipped.push(format!("line {}: `{}`", i + 1, t.chars().take(42).collect::<String>()));
        }
    }
    skipped
}

// ── Page ───────────────────────────────────────────────────────────────────

#[component]
pub fn QuantumDesignerPage() -> impl IntoView {
    let (mode, set_mode) = signal("visual".to_string());
    let (qasm, set_qasm) = signal(
        TEMPLATES.first().map(|t| t.qasm.to_string()).unwrap_or_default(),
    );
    let (selected_program, set_selected_program) = signal("bell".to_string());
    let (agent_id, set_agent_id) = signal(String::new());
    let (shots, set_shots) = signal("1024".to_string());
    let (seed, set_seed) = signal(String::new());
    // Interactive placement: armed palette gate + accumulated wire clicks.
    let (armed, set_armed) = signal(Option::<String>::None);
    let (pending_wires, set_pending_wires) = signal(Vec::<(usize, usize)>::new());
    let (hover_col, set_hover_col) = signal(Option::<usize>::None);

    let agents = LocalResource::new(|| async move { api::list_agents().await });

    let run_result: RwSignal<Option<Result<QSimulateResponse, String>>> = RwSignal::new(None);
    let is_running = RwSignal::new(false);

    let run_circuit = move |_| {
        let current_qasm = qasm.get();
        let current_agent = agent_id.get();
        if current_agent.is_empty() {
            run_result.set(Some(Err("Select an agent/target first.".to_string())));
            return;
        }
        let shot_count = shots.get().parse::<u32>().unwrap_or(1024).clamp(1, 100_000);
        let seed_val = {
            let t = seed.get();
            let t = t.trim();
            if t.is_empty() {
                None
            } else {
                t.parse::<u64>().ok()
            }
        };
        is_running.set(true);
        run_result.set(None);
        let req = QSimulateRequest { program: None, qasm: Some(current_qasm.clone()), shots: shot_count, seed: seed_val };
        let circuit_label = selected_program.get();
        wasm_bindgen_futures::spawn_local(async move {
            let result = api::quantum_simulate(&current_agent, &req).await;
            if let Ok(ref resp) = result {
                let job_id = format!("job-{}", js_sys::Date::now() as u64);
                let result_blob = json!({
                    "backend": resp.backend,
                    "shots": resp.shots,
                    "histogram": resp.histogram.iter().map(|b| json!({
                        "state": b.state,
                        "count": b.count,
                        "probability": b.probability,
                    })).collect::<Vec<_>>(),
                });
                let _ = gloo_storage::LocalStorage::set("ef_quantum_last_result", &result_blob);
                let mut log: Vec<serde_json::Value> =
                    gloo_storage::LocalStorage::get("ef_quantum_jobs").unwrap_or_default();
                log.push(json!({
                    "id": job_id,
                    "circuit": circuit_label,
                    "status": "Completed",
                    "backend": resp.backend.as_deref().unwrap_or("simulator"),
                    "shots": resp.shots.unwrap_or(shot_count),
                }));
                let _ = gloo_storage::LocalStorage::set("ef_quantum_jobs", &log);
            }
            run_result.set(Some(result));
            is_running.set(false);
        });
    };

    // Update QASM when a template is chosen.
    let on_program_change = move |ev: web_sys::Event| {
        let val = event_target_value(&ev);
        set_selected_program.set(val.clone());
        if let Some(t) = find_template(&val) {
            set_qasm.set(t.qasm.to_string());
        }
        set_armed.set(None);
        set_pending_wires.set(Vec::new());
    };

    // Snapshot of the editable circuit: width, ops, and each op's column.
    let snapshot = move || {
        let (nq, ops) = parse_qasm_ops(&qasm.get_untracked());
        let (_, op_cols) = layout_grid(nq, &ops);
        (nq, ops, op_cols)
    };

    let commit_ops = move |(nq, ops): (usize, Vec<ParsedOp>)| {
        set_qasm.set(ops_to_qasm(nq, &ops));
        set_selected_program.set("custom".to_string());
        set_pending_wires.set(Vec::new());
    };

    // Palette arming (click again to disarm).
    let on_palette_click = move |gate: &'static str| {
        let g = gate.to_string();
        if armed.get_untracked().as_deref() == Some(g.as_str()) {
            set_armed.set(None);
        } else {
            set_armed.set(Some(g));
        }
        set_pending_wires.set(Vec::new());
    };

    // Grid click: occupied cell removes its op; empty cell places the armed gate.
    let on_cell_click = move |(wire, col, op_idx): (usize, usize, Option<usize>)| {
        if let Some(i) = op_idx {
            let (nq, mut ops, _) = snapshot();
            if i < ops.len() {
                ops.remove(i);
                commit_ops((nq, ops));
            }
            return;
        }
        let Some(gate) = armed.get_untracked() else { return };
        let arity = gate_arity(&gate);
        if arity <= 1 {
            let (nq, mut ops, op_cols) = snapshot();
            let op = if gate == "measure" {
                ParsedOp { name: gate.clone(), params: String::new(), qubits: vec![wire], cbit: None }
            } else if gate == "barrier" {
                ParsedOp { name: gate.clone(), params: String::new(), qubits: vec![], cbit: None }
            } else {
                ParsedOp { name: gate.clone(), params: default_params(&gate), qubits: vec![wire], cbit: None }
            };
            insert_op_at(&mut ops, &op_cols, col, op);
            commit_ops((nq, ops));
            return;
        }
        // Multi-wire gate: accumulate distinct wire clicks, complete in click order.
        let mut acc = pending_wires.get_untracked();
        if acc.iter().any(|(w, _)| *w == wire) {
            acc = vec![(wire, col)];
        } else {
            acc.push((wire, col));
        }
        if acc.len() >= arity {
            let wires: Vec<usize> = acc.iter().map(|(w, _)| *w).collect();
            let at_col = acc.iter().map(|(_, c)| *c).max().unwrap_or(col);
            let (nq, mut ops, op_cols) = snapshot();
            let op = ParsedOp {
                name: gate.clone(),
                params: default_params(&gate),
                qubits: wires,
                cbit: None,
            };
            insert_op_at(&mut ops, &op_cols, at_col, op);
            commit_ops((nq, ops));
        } else {
            set_pending_wires.set(acc);
        }
    };

    let add_wire = move |_| {
        let (nq, ops) = parse_qasm_ops(&qasm.get_untracked());
        if nq < 12 {
            set_qasm.set(ops_to_qasm(nq + 1, &ops));
            set_selected_program.set("custom".to_string());
        }
    };

    let remove_wire = move |_| {
        let (nq, ops) = parse_qasm_ops(&qasm.get_untracked());
        if nq > 1 && !ops.iter().any(|o| o.qubits.iter().any(|&w| w + 1 >= nq)) {
            set_qasm.set(ops_to_qasm(nq - 1, &ops));
            set_selected_program.set("custom".to_string());
        }
    };

    let clear_circuit = move |_| {
        let (nq, _) = parse_qasm_ops(&qasm.get_untracked());
        set_qasm.set(ops_to_qasm(nq.max(1), &[]));
        set_selected_program.set("custom".to_string());
        set_armed.set(None);
        set_pending_wires.set(Vec::new());
    };

    // Interactive grid: hover traces the column, clicking a gate removes it,
    // clicking empty space places the armed palette gate.
    let grid_rows = move || {
        let (n, ops) = parse_qasm_ops(&qasm.get());
        let (rows, _) = layout_grid(n, &ops);
        let hot = hover_col.get();
        let armed_now = armed.get().is_some();
        rows.into_iter()
            .enumerate()
            .map(|(w, cells)| {
                let ncols = cells.len().max(1);
                view! {
                    <div class="qgrid-row">
                        <span class="wire-label">{format!("q{w}")}</span>
                        <div class="qgrid-cells" style=format!("grid-template-columns: repeat({ncols}, 46px);")>
                            {cells
                                .into_iter()
                                .map(|rc| {
                                    let is_hot = hot == Some(rc.col) && rc.cell.kind != "empty";
                                    let cls = format!(
                                        "qcell qcell-{}{}",
                                        rc.cell.kind,
                                        if is_hot { " qcell-hot" } else { "" },
                                    );
                                    let title = match rc.op_idx {
                                        Some(_) => "Click to remove this gate".to_string(),
                                        None if armed_now => "Click to place the armed gate here".to_string(),
                                        None => String::new(),
                                    };
                                    let cell_col = rc.col;
                                    let cell_op = rc.op_idx;
                                    view! {
                                        <div
                                            class=cls
                                            title=title
                                            on:mouseenter=move |_| set_hover_col.set(Some(cell_col))
                                            on:mouseleave=move |_| set_hover_col.set(None)
                                            on:click=move |_| on_cell_click((w, cell_col, cell_op))
                                        >
                                            {rc.cell.text}
                                        </div>
                                    }
                                })
                                .collect::<Vec<_>>()}
                        </div>
                    </div>
                }
            })
            .collect::<Vec<_>>()
    };

    // Circuit stats + validation, recomputed from the live source.
    let circuit_stats = move || {
        let (n, ops) = parse_qasm_ops(&qasm.get());
        let (_, op_cols) = layout_grid(n, &ops);
        let depth = op_cols.iter().copied().max().map(|m| m + 1).unwrap_or(0);
        let mut measured = vec![false; n.max(1)];
        for op in &ops {
            if op.name == "measure" {
                if op.qubits.is_empty() {
                    measured.iter_mut().for_each(|m| *m = true);
                } else {
                    for &q in &op.qubits {
                        if q < measured.len() {
                            measured[q] = true;
                        }
                    }
                }
            }
        }
        let measured_count = measured.iter().filter(|&&m| m).count();
        (n, ops.len(), depth, measured_count)
    };

    let lint_warnings = move || lint_qasm(&qasm.get());

    let placement_hint = move || {
        if let Some(g) = armed.get() {
            let need = gate_arity(&g);
            let have = pending_wires.get().len();
            if need <= 1 {
                format!("Placing {g} — click a wire cell. Click the palette chip again to disarm.")
            } else if have == 0 {
                format!("Placing {g} — click the first (control) wire.")
            } else {
                format!("Placing {g} — {have}/{need} wires chosen, click the next wire.")
            }
        } else {
            "Pick a gate from the palette, then click the grid to place it. Click a placed gate to remove it.".to_string()
        }
    };

    let template_desc = move || {
        find_template(&selected_program.get())
            .map(|t| t.description.to_string())
            .unwrap_or_else(|| "Hand-written OpenQASM 2.0 — runs on the VeerOS simulator.".to_string())
    };

    let qiskit_code = move || qasm_to_qiskit(&qasm.get(), shots.get().parse::<u32>().unwrap_or(1024), {
        let t = seed.get();
        let t = t.trim();
        if t.is_empty() { None } else { t.parse::<u64>().ok() }
    });

    let copy_qiskit = move |_| {
        let code = qiskit_code();
        let payload = serde_json::to_string(&code).unwrap_or_default();
        let _ = js_sys::eval(&format!("navigator.clipboard && navigator.clipboard.writeText({payload})"));
    };

    let download_qiskit = move |_| {
        let code = qiskit_code();
        let payload = serde_json::to_string(&code).unwrap_or_default();
        let _ = js_sys::eval(&format!(
            "(function(){{ var b = new Blob([{payload}], {{type: 'text/x-python'}}); var a = document.createElement('a'); a.href = URL.createObjectURL(b); a.download = 'edgefabric_circuit.py'; a.click(); setTimeout(function(){{ URL.revokeObjectURL(a.href); }}, 4000); }})()"
        ));
    };

    let download_qasm = move |_| {
        let src = qasm.get();
        let payload = serde_json::to_string(&src).unwrap_or_default();
        let _ = js_sys::eval(&format!(
            "(function(){{ var b = new Blob([{payload}], {{type: 'text/plain'}}); var a = document.createElement('a'); a.href = URL.createObjectURL(b); a.download = 'edgefabric_circuit.qasm'; a.click(); setTimeout(function(){{ URL.revokeObjectURL(a.href); }}, 4000); }})()"
        ));
    };

    view! {
        <div class="page-container quantum-designer-page">
            <div class="page-header">
                <div>
                    <h1>"Quantum Circuit Designer"</h1>
                    <p>"Design visually or in OpenQASM, export Qiskit Python, and run on a connected agent."</p>
                </div>
                <div class="mode-switch">
                    <button class:active=move || mode.get() == "visual"
                        on:click=move |_| set_mode.set("visual".to_string())>"Visual"</button>
                    <button class:active=move || mode.get() == "code"
                        on:click=move |_| set_mode.set("code".to_string())>"QASM"</button>
                    <button class:active=move || mode.get() == "qiskit"
                        on:click=move |_| set_mode.set("qiskit".to_string())>"Qiskit"</button>
                </div>
            </div>

            <div class="designer-layout">
                // ── control panel ───────────────────────────────────────────
                <section class="designer-panel control-panel">
                    <h2>"Circuit Setup"</h2>

                    <label class="field-row">
                        <span>"Template"</span>
                        <select on:change=on_program_change prop:value=move || selected_program.get()>
                            <option value="custom">"Custom QASM"</option>
                            {TEMPLATES
                                .iter()
                                .map(|t| {
                                    view! { <option value=t.id>{format!("{} · {}q", t.name, t.qubits)}</option> }
                                })
                                .collect::<Vec<_>>()}
                        </select>
                    </label>
                    <p class="hint">{template_desc}</p>

                    <div class="wire-controls">
                        <span>"Wires"</span>
                        <div class="wire-btn-row">
                            <button class="btn btn-ghost btn-sm" on:click=remove_wire title="Remove the last wire (must be unused)">"−"</button>
                            <strong>{move || circuit_stats().0.to_string()}</strong>
                            <button class="btn btn-ghost btn-sm" on:click=add_wire title="Add a wire (max 12)">"+"</button>
                            <button class="btn btn-ghost btn-sm" on:click=clear_circuit title="Remove all gates">"Clear"</button>
                        </div>
                    </div>

                    <div class="gate-palette">
                        <span>"Gate Palette — click to arm, then click the grid"</span>
                        <div class="gate-list">
                            {["H", "X", "Y", "Z", "S", "T", "RX", "RY", "RZ", "U", "CX", "CY", "CZ", "SWAP", "CCX", "CP", "M", "BARRIER"]
                                .iter()
                                .map(|g| {
                                    let name = g.to_string();
                                    let label = g.to_string();
                                    let gate_key: &'static str = match *g {
                                        "RX" => "rx", "RY" => "ry", "RZ" => "rz",
                                        "U" => "u", "CX" => "cx", "CY" => "cy",
                                        "CZ" => "cz", "SWAP" => "swap", "CCX" => "ccx",
                                        "CP" => "cp", "M" => "measure", "BARRIER" => "barrier",
                                        "H" => "h", "X" => "x", "Y" => "y",
                                        "Z" => "z", "S" => "s", "T" => "t",
                                        _ => "h",
                                    };
                                    let tip = format!("Place {label} (multi-wire gates need several clicks)");
                                    view! {
                                        <button
                                            type="button"
                                            class="gate-chip"
                                            class:armed=move || armed.get().as_deref() == Some(name.as_str())
                                            on:click=move |_| on_palette_click(gate_key)
                                            title=tip
                                        >
                                            {label}
                                        </button>
                                    }
                                })
                                .collect::<Vec<_>>()}
                        </div>
                        <span class="hint">{placement_hint}</span>
                    </div>

                    <div class="stat-grid">
                        {move || {
                            let (w, gates, depth, meas) = circuit_stats();
                            view! {
                                <>
                                    <div class="stat"><span>"Width"</span><strong>{w.to_string()}</strong></div>
                                    <div class="stat"><span>"Gates"</span><strong>{gates.to_string()}</strong></div>
                                    <div class="stat"><span>"Depth"</span><strong>{depth.to_string()}</strong></div>
                                    <div class="stat"><span>"Measured"</span><strong>{format!("{meas}/{w}")}</strong></div>
                                </>
                            }
                        }}
                    </div>

                    {move || {
                        let warns = lint_warnings();
                        let (n, _, _, _) = circuit_stats();
                        if n > 12 {
                            view! {
                                <p class="error-msg">"Too many wires for the simulator (max 12). Remove wires or shrink qreg."</p>
                            }.into_any()
                        } else if !warns.is_empty() {
                            let shown: Vec<String> = warns.iter().take(3).cloned().collect();
                            let extra = warns.len().saturating_sub(3);
                            view! {
                                <div class="lint-warn">
                                    <strong>"Simulator will skip:"</strong>
                                    {shown.into_iter().map(|w| view! { <div>{w}</div> }).collect::<Vec<_>>()}
                                    {if extra > 0 {
                                        view! { <div>{format!("…and {extra} more line(s)")}</div> }.into_any()
                                    } else {
                                        view! { <></> }.into_any()
                                    }}
                                </div>
                            }.into_any()
                        } else {
                            view! { <></> }.into_any()
                        }
                    }}

                    <hr/>

                    <h2>"Run"</h2>

                    <label class="field-row">
                        <span>"Target Agent"</span>
                        <Suspense fallback=move || view! { <select disabled><option>"Loading..."</option></select> }>
                            {move || {
                                agents.get().map(|res| {
                                    let list = (*res).clone().unwrap_or_default().agents;
                                    view! {
                                        <select on:change=move |ev| set_agent_id.set(event_target_value(&ev))>
                                            <option value="">"-- select agent --"</option>
                                            {list.into_iter().map(|a| {
                                                let label = a.hostname.or(a.name).unwrap_or_else(|| a.id.clone());
                                                let id = a.id.clone();
                                                view! { <option value=id>{label}</option> }
                                            }).collect::<Vec<_>>()}
                                        </select>
                                    }
                                })
                            }}
                        </Suspense>
                    </label>

                    <div class="run-grid">
                        <label class="field-row">
                            <span>"Shots"</span>
                            <input type="number" min="1" max="100000" prop:value=shots
                                on:input=move |ev| set_shots.set(event_target_value(&ev))/>
                        </label>
                        <label class="field-row">
                            <span>"Seed (optional)"</span>
                            <input type="number" min="0" placeholder="random" prop:value=seed
                                on:input=move |ev| set_seed.set(event_target_value(&ev))/>
                        </label>
                    </div>

                    {move || {
                        let reason = if is_running.get() {
                            None
                        } else if agent_id.get().is_empty() {
                            Some("Select a target agent to run.".to_string())
                        } else {
                            let (n, ops) = parse_qasm_ops(&qasm.get());
                            if n > 12 {
                                Some("Too many wires for the simulator (max 12).".to_string())
                            } else if ops.is_empty() {
                                Some("Place gates or pick a template first.".to_string())
                            } else {
                                None
                            }
                        };
                        let blocked = reason.is_some() || is_running.get();
                        view! {
                            <button class="run-btn"
                                disabled=blocked
                                on:click=run_circuit>
                                {if is_running.get() { "Running..." } else { "Run Simulation" }}
                            </button>
                            {reason.map(|r| view! { <p class="hint">{r}</p> })}
                        }
                    }}
                </section>

                // ── circuit editor ──────────────────────────────────────────
                <section class="designer-panel editor-panel">
                    <Show when=move || mode.get() == "visual">
                        <h2>"Visual Circuit"</h2>
                        <div class="circuit-canvas qcanvas">{grid_rows}</div>
                    </Show>
                    <Show when=move || mode.get() == "code">
                        <div class="editor-head">
                            <h2>"OpenQASM Source"</h2>
                            <button class="btn btn-ghost btn-sm" on:click=download_qasm>"Download .qasm"</button>
                        </div>
                        <textarea class="qasm-editor" rows="22"
                            prop:value=qasm
                            on:input=move |ev| {
                                set_qasm.set(event_target_value(&ev));
                                set_selected_program.set("custom".to_string());
                            }>
                        </textarea>
                    </Show>
                    <Show when=move || mode.get() == "qiskit">
                        <div class="editor-head">
                            <h2>"Qiskit Python"</h2>
                            <div style="display:flex; gap:0.5rem;">
                                <button class="btn btn-ghost btn-sm" on:click=copy_qiskit>"Copy"</button>
                                <button class="btn btn-ghost btn-sm" on:click=download_qiskit>"Download .py"</button>
                            </div>
                        </div>
                        <p class="hint">
                            "Generated from the current circuit. Run it in your own Qiskit environment
                            (pip install qiskit qiskit-aer) — or press Run Simulation to execute
                            the same circuit on the VeerOS runtime."
                        </p>
                        <pre class="qiskit-code">{qiskit_code}</pre>
                    </Show>
                </section>

                // ── results panel ───────────────────────────────────────────
                <section class="designer-panel results-panel-inline">
                    <h2>"Results"</h2>
                    {move || match run_result.get() {
                        None => view! {
                            <p class="hint">"Run a circuit to see measurement results here."</p>
                        }.into_any(),
                        Some(Err(e)) => view! {
                            <p class="error-msg">"Error: " {e}</p>
                        }.into_any(),
                        Some(Ok(resp)) => {
                            let shots = resp.shots.unwrap_or(1024);
                            let backend = resp.backend.clone().unwrap_or_else(|| "simulator".to_string());
                            let bins = resp.histogram.clone();
                            view! {
                                <p class="run-meta">"Backend: " <strong>{backend}</strong> " | Shots: " <strong>{shots}</strong></p>
                                {histogram_chart(&bins)}
                                <div class="histogram-list">
                                    {resp.histogram.into_iter().map(|bin| {
                                        let width = (bin.probability * 100.0).min(100.0);
                                        view! {
                                            <div class="histogram-row">
                                                <code class="bitstring">{bin.state}</code>
                                                <div class="bar-track">
                                                    <div class="bar-fill" style:width=format!("{width:.1}%")></div>
                                                </div>
                                                <span class="bar-count">{format!("{} ({:.1}%)", bin.count, bin.probability * 100.0)}</span>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            }.into_any()
                        }
                    }}
                </section>
            </div>
        </div>

        <style>
            {r#"
                .quantum-designer-page {
                    padding: 2rem;
                    display: flex;
                    flex-direction: column;
                    gap: 1.5rem;
                }

                .page-header {
                    display: flex;
                    align-items: flex-end;
                    justify-content: space-between;
                    gap: 1rem;
                }

                .page-header h1 { margin: 0; color: var(--text-primary); }
                .page-header p { margin: 0.35rem 0 0; color: var(--text-secondary); }

                .mode-switch {
                    display: flex;
                    border: 1px solid var(--border-color);
                    border-radius: 8px;
                    overflow: hidden;
                }

                .mode-switch button {
                    padding: 0.5rem 1rem;
                    border: none;
                    background: var(--bg-secondary);
                    color: var(--text-secondary);
                    cursor: pointer;
                    font-size: 0.9rem;
                }

                .mode-switch button.active {
                    background: var(--accent-color);
                    color: white;
                }

                .designer-layout {
                    display: grid;
                    grid-template-columns: 280px minmax(0, 1fr) 300px;
                    gap: 1rem;
                    align-items: start;
                    width: 100%;
                    max-width: 100%;
                }

                @media (max-width: 1400px) {
                    .designer-layout { grid-template-columns: 240px minmax(0, 1fr); }
                    .results-panel-inline { grid-column: 1 / -1; }
                }

                @media (max-width: 1100px) {
                    .designer-layout { grid-template-columns: minmax(0, 1fr); }
                    .results-panel-inline { grid-column: auto; }
                }

                .designer-panel {
                    background: var(--card-bg);
                    border: 1px solid var(--border-color);
                    border-radius: 10px;
                    padding: 1.25rem;
                    display: flex;
                    flex-direction: column;
                    gap: 0.9rem;
                    min-width: 0;
                    max-width: 100%;
                    box-sizing: border-box;
                }

                .editor-panel {
                    overflow: hidden;
                }

                .designer-panel h2 {
                    margin: 0;
                    font-size: 0.95rem;
                    font-weight: 600;
                    color: var(--text-primary);
                }

                .field-row {
                    display: flex;
                    flex-direction: column;
                    gap: 0.35rem;
                    font-size: 0.85rem;
                    color: var(--text-secondary);
                }

                .field-row input,
                .field-row select {
                    padding: 0.55rem 0.7rem;
                    border: 1px solid var(--border-color);
                    border-radius: 6px;
                    background: var(--bg-secondary);
                    color: var(--text-primary);
                    font-size: 0.9rem;
                }

                .run-grid {
                    display: grid;
                    grid-template-columns: 1fr 1fr;
                    gap: 0.6rem;
                }

                .gate-palette {
                    display: flex;
                    flex-direction: column;
                    gap: 0.5rem;
                    font-size: 0.85rem;
                    color: var(--text-secondary);
                }

                .gate-list {
                    display: flex;
                    flex-wrap: wrap;
                    gap: 0.4rem;
                }

                .gate-chip {
                    background: var(--bg-secondary);
                    border: 1px solid var(--border-color);
                    border-radius: 6px;
                    padding: 0.3rem 0.6rem;
                    font-size: 0.8rem;
                    font-family: monospace;
                    color: var(--text-primary);
                }

                hr { border: none; border-top: 1px solid var(--border-color); }

                .run-btn {
                    background: hsl(140, 60%, 38%);
                    color: white;
                    border: none;
                    border-radius: 8px;
                    padding: 0.75rem;
                    font-size: 0.95rem;
                    cursor: pointer;
                    font-weight: 600;
                    width: 100%;
                }

                .run-btn:disabled {
                    background: var(--bg-secondary);
                    color: var(--text-secondary);
                    cursor: not-allowed;
                }

                .editor-head {
                    display: flex;
                    align-items: center;
                    justify-content: space-between;
                    gap: 0.75rem;
                    flex-wrap: wrap;
                }

                .editor-head h2 { margin: 0; }

                .circuit-canvas {
                    display: flex;
                    flex-direction: column;
                    gap: 0.75rem;
                    padding: 1rem;
                    background: var(--bg-secondary);
                    border-radius: 8px;
                    overflow-x: auto;
                    overflow-y: hidden;
                    max-width: 100%;
                    box-sizing: border-box;
                    scrollbar-width: thin;
                }

                .qgrid-row {
                    display: flex;
                    align-items: stretch;
                    gap: 0.5rem;
                    min-height: 44px;
                    width: max-content;
                    min-width: 100%;
                }

                .wire-label {
                    font-family: monospace;
                    font-size: 0.85rem;
                    color: var(--text-secondary);
                    width: 28px;
                    flex-shrink: 0;
                    align-self: center;
                }

                .qgrid-cells {
                    display: grid;
                    gap: 2px;
                    align-items: stretch;
                    background:
                        linear-gradient(to bottom, transparent calc(50% - 1px), var(--border-color) calc(50% - 1px), var(--border-color) calc(50% + 1px), transparent calc(50% + 1px));
                }

                .qcell {
                    min-height: 42px;
                    display: flex;
                    align-items: center;
                    justify-content: center;
                    font-family: monospace;
                    font-size: 0.72rem;
                    font-weight: 600;
                    border-radius: 6px;
                    padding: 0 2px;
                    text-align: center;
                    position: relative;
                }

                .qcell-box {
                    background: var(--accent-color);
                    color: white;
                }

                .qcell-pbox {
                    background: hsl(265, 70%, 55%);
                    color: white;
                }

                .qcell-ctrl {
                    color: var(--text-primary);
                    font-size: 1.1rem;
                }

                .qcell-targ {
                    color: var(--text-primary);
                    font-size: 1.1rem;
                }

                .qcell-targ::after {
                    content: "";
                    position: absolute;
                    inset: 8px;
                    border: 2px solid currentColor;
                    border-radius: 50%;
                    pointer-events: none;
                }

                .qcell-swap {
                    color: var(--text-primary);
                    font-size: 1rem;
                }

                .qcell-meas {
                    background: hsl(45, 80%, 45%);
                    color: #111;
                }

                .qcell-link {
                    color: var(--text-secondary);
                }

                .qcell-link::before {
                    content: "";
                    position: absolute;
                    top: -2px;
                    bottom: -2px;
                    width: 2px;
                    background: var(--text-secondary);
                    opacity: 0.7;
                }

                .qcell-barrier {
                    border-left: 2px dashed var(--text-secondary);
                    border-radius: 0;
                }

                .qcell-empty {
                    background: transparent;
                }

                .qasm-editor {
                    width: 100%;
                    font-family: monospace;
                    font-size: 0.85rem;
                    background: hsl(220, 20%, 8%);
                    color: hsl(0, 0%, 88%);
                    border: 1px solid var(--border-color);
                    border-radius: 8px;
                    padding: 1rem;
                    resize: vertical;
                    box-sizing: border-box;
                    line-height: 1.6;
                }

                .qiskit-code {
                    font-family: monospace;
                    font-size: 0.8rem;
                    background: hsl(220, 20%, 8%);
                    color: hsl(95, 60%, 82%);
                    border: 1px solid var(--border-color);
                    border-radius: 8px;
                    padding: 1rem;
                    overflow-x: auto;
                    white-space: pre;
                    line-height: 1.55;
                    margin: 0;
                    max-width: 100%;
                    box-sizing: border-box;
                }

                .results-panel-inline {
                    overflow: hidden;
                }

                .histogram-row {
                    min-width: 0;
                }

                .bitstring {
                    overflow-wrap: anywhere;
                    flex-shrink: 0;
                }

                .bar-count {
                    flex-shrink: 0;
                }

                @media (max-width: 700px) {
                    .quantum-designer-page {
                        padding: 1rem;
                    }

                    .page-header {
                        flex-direction: column;
                        align-items: stretch;
                    }

                    .run-grid {
                        grid-template-columns: 1fr;
                    }
                }

                .histogram-list {
                    display: flex;
                    flex-direction: column;
                    gap: 0.6rem;
                }

                .histogram-row {
                    display: flex;
                    align-items: center;
                    gap: 0.75rem;
                    font-size: 0.85rem;
                }

                .bitstring {
                    font-family: monospace;
                    background: var(--bg-secondary);
                    border-radius: 4px;
                    padding: 0.15rem 0.4rem;
                    min-width: 36px;
                    text-align: center;
                    color: var(--text-primary);
                }

                .bar-track {
                    flex: 1 1 auto;
                    min-width: 40px;
                    background: var(--bg-secondary);
                    border-radius: 999px;
                    height: 12px;
                    overflow: hidden;
                }

                .bar-fill {
                    height: 100%;
                    background: var(--accent-color);
                    border-radius: 999px;
                    transition: width 0.4s ease;
                }

                .bar-count { color: var(--text-secondary); min-width: 90px; text-align: right; }

                .hint, .run-meta { color: var(--text-secondary); font-size: 0.9rem; }
                .hint { margin: 0; }
                .error-msg { color: hsl(0, 70%, 55%); font-size: 0.9rem; }

                /* ── interactive grid ── */
                .qcell {
                    cursor: pointer;
                    transition: filter 0.12s ease, box-shadow 0.12s ease;
                }

                .qcell-empty { cursor: crosshair; }

                .qcell-hot {
                    filter: brightness(1.35);
                    box-shadow: 0 0 0 2px var(--accent-color);
                }

                button.gate-chip {
                    font: inherit;
                    cursor: pointer;
                }

                button.gate-chip:hover {
                    border-color: var(--accent-color);
                }

                .gate-chip.armed {
                    background: var(--accent-color);
                    color: white;
                    border-color: var(--accent-color);
                    box-shadow: 0 0 0 2px hsla(210, 90%, 55%, 0.35);
                }

                .wire-controls {
                    display: flex;
                    flex-direction: column;
                    gap: 0.4rem;
                    font-size: 0.85rem;
                    color: var(--text-secondary);
                }

                .wire-btn-row {
                    display: flex;
                    align-items: center;
                    gap: 0.5rem;
                }

                .wire-btn-row strong {
                    min-width: 2ch;
                    text-align: center;
                    color: var(--text-primary);
                    font-family: monospace;
                }

                .stat-grid {
                    display: grid;
                    grid-template-columns: repeat(4, 1fr);
                    gap: 0.5rem;
                }

                .stat {
                    display: flex;
                    flex-direction: column;
                    align-items: center;
                    gap: 0.15rem;
                    background: var(--bg-secondary);
                    border: 1px solid var(--border-color);
                    border-radius: 8px;
                    padding: 0.45rem 0.25rem;
                }

                .stat span {
                    font-size: 0.68rem;
                    text-transform: uppercase;
                    letter-spacing: 0.06em;
                    color: var(--text-secondary);
                }

                .stat strong {
                    font-family: monospace;
                    font-size: 0.95rem;
                    color: var(--text-primary);
                }

                .lint-warn {
                    background: hsla(45, 80%, 45%, 0.12);
                    border: 1px solid hsla(45, 80%, 45%, 0.45);
                    border-radius: 8px;
                    padding: 0.6rem 0.75rem;
                    font-size: 0.8rem;
                    color: var(--text-primary);
                    display: flex;
                    flex-direction: column;
                    gap: 0.2rem;
                }

                .lint-warn div { font-family: monospace; }

                /* ── results chart ── */
                .chart-wrap {
                    overflow-x: auto;
                    max-width: 100%;
                    background: var(--bg-secondary);
                    border-radius: 8px;
                    padding: 0.5rem 0.25rem 0.25rem;
                }

                .histogram-chart {
                    width: 100%;
                    height: auto;
                    display: block;
                }

                .chart-grid {
                    stroke: var(--border-color);
                    stroke-width: 1;
                    stroke-dasharray: 3 3;
                }

                .chart-axis { stroke: var(--text-secondary); stroke-width: 1.5; }

                .chart-tick {
                    fill: var(--text-secondary);
                    font-size: 10px;
                    font-family: monospace;
                }

                .chart-bar { fill: var(--accent-color); }

                .chart-bar:hover { filter: brightness(1.25); }

                .chart-value {
                    fill: var(--text-primary);
                    font-size: 11px;
                    font-weight: 700;
                    font-family: monospace;
                }

                .chart-label {
                    fill: var(--text-secondary);
                    font-size: 11px;
                    font-family: monospace;
                }
            "#}
        </style>
    }
}

#[cfg(test)]
mod designer_tests {
    use super::{gate_arity, insert_op_at, layout_grid, ops_to_qasm, parse_qasm_ops, ParsedOp};

    fn op(name: &str, qubits: Vec<usize>) -> ParsedOp {
        ParsedOp { name: name.into(), params: String::new(), qubits, cbit: None }
    }

    #[test]
    fn templates_round_trip_through_regen() {
        for t in super::TEMPLATES {
            let (nq, ops) = parse_qasm_ops(t.qasm);
            assert_eq!(nq, t.qubits, "width mismatch in {}", t.id);
            let regen = ops_to_qasm(nq, &ops);
            let (nq2, ops2) = parse_qasm_ops(&regen);
            assert_eq!((nq2, ops2.len()), (nq, ops.len()), "unstable regen in {}", t.id);
        }
    }

    #[test]
    fn insert_orders_by_column() {
        let mut ops = vec![op("h", vec![0]), op("x", vec![1])];
        // layout: h at col 0, x at col 0 (different wires share column 0)
        let (_, cols) = layout_grid(2, &ops);
        assert_eq!(cols, vec![0, 0]);
        insert_op_at(&mut ops, &cols, 0, op("z", vec![0]));
        // z serializes after the col-0 ops
        assert_eq!(ops.len(), 3);
        assert_eq!(ops[2].name, "z");
        let regen = ops_to_qasm(2, &ops);
        assert!(regen.contains("z q[0];"));
    }

    #[test]
    fn gate_arities() {
        assert_eq!(gate_arity("h"), 1);
        assert_eq!(gate_arity("measure"), 1);
        assert_eq!(gate_arity("barrier"), 1);
        assert_eq!(gate_arity("cx"), 2);
        assert_eq!(gate_arity("swap"), 2);
        assert_eq!(gate_arity("cp"), 2);
        assert_eq!(gate_arity("ccx"), 3);
    }

    #[test]
    fn layout_tracks_op_ownership() {
        let (_, ops) = parse_qasm_ops(super::TEMPLATES[0].qasm); // bell
        let (rows, op_cols) = layout_grid(2, &ops);
        assert_eq!(op_cols.len(), ops.len());
        // h(q0) col 0, cx col 1, measure-all col 2
        assert_eq!(op_cols, vec![0, 1, 2]);
        // The cx control cell owns op index 1.
        assert_eq!(rows[0][1].op_idx, Some(1));
        assert_eq!(rows[1][1].op_idx, Some(1));
    }
}
