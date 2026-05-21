//! Silicon schematic editor — chip-domain skin over the shared
//! `components::schematic::SchematicEditor` engine.
//!
//! Provides a chip-grade symbol library (MOS transistors, Josephson junction,
//! passives, power, ports, transmon island, λ/4 resonator, and a RISC-V
//! block-diagram set) plus three presets selectable via `?project=`:
//!
//!   * `nand2`      — 2-input CMOS NAND standard cell (transistor level)
//!   * `sqc-q5`     — superconducting transmon unit cell (JJ + shunt + readout)
//!   * `riscv-core` — RISC-V RV32I 5-stage pipeline (block diagram)

use crate::components::schematic::{
    Endpoint, Pin, Schematic, SchematicEditor, SymbolDef, SymbolLibrary,
};
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

// ---------------------------------------------------------------------------
// Chip symbol library
// ---------------------------------------------------------------------------

struct ChipLib;

impl SymbolLibrary for ChipLib {
    fn name(&self) -> &'static str {
        "Chip / Silicon"
    }
    fn symbols(&self) -> &'static [SymbolDef] {
        SYMBOLS
    }
}

static CHIP_LIB: ChipLib = ChipLib;

// Pin index conventions per symbol (used by presets below):
//   NMOS / PMOS   : 0=G, 1=D, 2=S
//   2-pin parts   : 0,1
//   VDD / GND     : 0
//   Port          : 0
//   XMon island   : 0=N, 1=S, 2=W, 3=E
//   ALU           : 0=A, 1=B, 2=OP, 3=Y
//   Mux2          : 0=I0, 1=I1, 2=SEL, 3=Y
//   RegFile       : 0=rs1, 1=rs2, 2=rd, 3=wdata, 4=rdata1, 5=rdata2
//   Decode        : 0=instr, 1=rs1, 2=rs2, 3=rd, 4=ctrl
//   Imm-gen       : 0=instr, 1=imm
//   Ctrl          : 0=opcode, 1=ctrl
//   PC            : 0=clk, 1=pc_in, 2=pc_out
//   Add4 / Adder  : 0=A, 1=B, 2=Y
//   IMEM/DMEM     : 0=addr, 1=din, 2=dout  (IMEM ignores din)

const SYMBOLS: &[SymbolDef] = &[
    // ------- Transistors -------
    SymbolDef {
        kind: "nmos", label: "NMOS", category: "Transistors", prefix: "M",
        bbox: (50.0, 60.0),
        pins: &[
            Pin { name: "G", x: 0.0, y: 30.0 },
            Pin { name: "D", x: 50.0, y: 0.0 },
            Pin { name: "S", x: 50.0, y: 60.0 },
        ],
        body: r#"
            <line x1="0" y1="30" x2="20" y2="30"/>
            <line class="nmos" x1="20" y1="14" x2="20" y2="46"/>
            <line class="nmos" x1="25" y1="14" x2="25" y2="46"/>
            <line x1="25" y1="18" x2="50" y2="6"/>
            <line x1="50" y1="6" x2="50" y2="0"/>
            <line x1="25" y1="42" x2="50" y2="54"/>
            <line x1="50" y1="54" x2="50" y2="60"/>
            <polygon class="fill" points="34,38 42,46 40,38"/>
        "#,
    },
    SymbolDef {
        kind: "pmos", label: "PMOS", category: "Transistors", prefix: "M",
        bbox: (50.0, 60.0),
        pins: &[
            Pin { name: "G", x: 0.0, y: 30.0 },
            Pin { name: "D", x: 50.0, y: 60.0 },
            Pin { name: "S", x: 50.0, y: 0.0 },
        ],
        body: r#"
            <line x1="0" y1="30" x2="14" y2="30"/>
            <circle cx="17" cy="30" r="3"/>
            <line class="pmos" x1="20" y1="14" x2="20" y2="46"/>
            <line class="pmos" x1="25" y1="14" x2="25" y2="46"/>
            <line x1="25" y1="18" x2="50" y2="6"/>
            <line x1="50" y1="6" x2="50" y2="0"/>
            <line x1="25" y1="42" x2="50" y2="54"/>
            <line x1="50" y1="54" x2="50" y2="60"/>
            <polygon class="fill" points="32,12 40,20 34,20"/>
        "#,
    },
    SymbolDef {
        kind: "jj", label: "Josephson Jn", category: "Transistors", prefix: "J",
        bbox: (40.0, 40.0),
        pins: &[
            Pin { name: "1", x: 0.0, y: 20.0 },
            Pin { name: "2", x: 40.0, y: 20.0 },
        ],
        body: r#"
            <line x1="0" y1="20" x2="14" y2="20"/>
            <line x1="26" y1="20" x2="40" y2="20"/>
            <line class="jj" x1="14" y1="10" x2="26" y2="30"/>
            <line class="jj" x1="14" y1="30" x2="26" y2="10"/>
            <rect class="jj" x="13" y="13" width="14" height="14" fill="none"/>
        "#,
    },

    // ------- Passives -------
    SymbolDef {
        kind: "res", label: "Resistor", category: "Passives", prefix: "R",
        bbox: (60.0, 20.0),
        pins: &[
            Pin { name: "1", x: 0.0, y: 10.0 },
            Pin { name: "2", x: 60.0, y: 10.0 },
        ],
        body: r#"
            <line x1="0" y1="10" x2="10" y2="10"/>
            <polyline points="10,10 14,3 22,17 30,3 38,17 46,3 50,10"/>
            <line x1="50" y1="10" x2="60" y2="10"/>
        "#,
    },
    SymbolDef {
        kind: "cap", label: "Capacitor", category: "Passives", prefix: "C",
        bbox: (40.0, 30.0),
        pins: &[
            Pin { name: "1", x: 0.0, y: 15.0 },
            Pin { name: "2", x: 40.0, y: 15.0 },
        ],
        body: r#"
            <line x1="0" y1="15" x2="17" y2="15"/>
            <line x1="17" y1="3" x2="17" y2="27"/>
            <line x1="23" y1="3" x2="23" y2="27"/>
            <line x1="23" y1="15" x2="40" y2="15"/>
        "#,
    },
    SymbolDef {
        kind: "ind", label: "Inductor", category: "Passives", prefix: "L",
        bbox: (60.0, 20.0),
        pins: &[
            Pin { name: "1", x: 0.0, y: 10.0 },
            Pin { name: "2", x: 60.0, y: 10.0 },
        ],
        body: r#"
            <line x1="0" y1="10" x2="10" y2="10"/>
            <path d="M10 10 a5 5 0 0 1 10 0 a5 5 0 0 1 10 0 a5 5 0 0 1 10 0 a5 5 0 0 1 10 0"/>
            <line x1="50" y1="10" x2="60" y2="10"/>
        "#,
    },

    // ------- Power / nets / ports -------
    SymbolDef {
        kind: "vdd", label: "VDD", category: "Power & Ports", prefix: "#VDD",
        bbox: (30.0, 30.0),
        pins: &[Pin { name: "1", x: 15.0, y: 30.0 }],
        body: r#"
            <line x1="15" y1="30" x2="15" y2="14"/>
            <line x1="3" y1="14" x2="27" y2="14"/>
            <text class="label" x="15" y="9" text-anchor="middle">VDD</text>
        "#,
    },
    SymbolDef {
        kind: "gnd", label: "GND", category: "Power & Ports", prefix: "#GND",
        bbox: (30.0, 30.0),
        pins: &[Pin { name: "1", x: 15.0, y: 0.0 }],
        body: r#"
            <line x1="15" y1="0" x2="15" y2="14"/>
            <line x1="3" y1="14" x2="27" y2="14"/>
            <line x1="7" y1="19" x2="23" y2="19"/>
            <line x1="11" y1="24" x2="19" y2="24"/>
        "#,
    },
    SymbolDef {
        kind: "port_in", label: "Input Port", category: "Power & Ports", prefix: "P",
        bbox: (60.0, 18.0),
        pins: &[Pin { name: "1", x: 60.0, y: 9.0 }],
        body: r#"
            <polygon points="0,0 50,0 60,9 50,18 0,18"/>
            <text class="label" x="25" y="12" text-anchor="middle">IN</text>
        "#,
    },
    SymbolDef {
        kind: "port_out", label: "Output Port", category: "Power & Ports", prefix: "P",
        bbox: (60.0, 18.0),
        pins: &[Pin { name: "1", x: 0.0, y: 9.0 }],
        body: r#"
            <polygon points="0,9 10,0 60,0 60,18 10,18"/>
            <text class="label" x="35" y="12" text-anchor="middle">OUT</text>
        "#,
    },

    // ------- Quantum -------
    SymbolDef {
        kind: "xmon", label: "X-mon Island", category: "Quantum", prefix: "Q",
        bbox: (60.0, 60.0),
        pins: &[
            Pin { name: "N", x: 30.0, y: 0.0 },
            Pin { name: "S", x: 30.0, y: 60.0 },
            Pin { name: "W", x: 0.0, y: 30.0 },
            Pin { name: "E", x: 60.0, y: 30.0 },
        ],
        body: r#"
            <polygon class="fill jj" points="22,0 38,0 38,22 60,22 60,38 38,38 38,60 22,60 22,38 0,38 0,22 22,22"/>
        "#,
    },
    SymbolDef {
        kind: "resonator", label: "λ/4 Resonator", category: "Quantum", prefix: "R",
        bbox: (90.0, 30.0),
        pins: &[
            Pin { name: "1", x: 0.0, y: 15.0 },
            Pin { name: "2", x: 90.0, y: 15.0 },
        ],
        body: r#"
            <line x1="0" y1="15" x2="10" y2="15"/>
            <rect x="10" y="5" width="70" height="20" rx="3"/>
            <line x1="80" y1="15" x2="90" y2="15"/>
            <text class="label" x="45" y="19" text-anchor="middle">λ/4 RES</text>
        "#,
    },

    // ------- RISC-V pipeline blocks -------
    SymbolDef {
        kind: "rv_pc", label: "PC Register", category: "RISC-V Pipeline", prefix: "U",
        bbox: (90.0, 50.0),
        pins: &[
            Pin { name: "clk",    x: 0.0,  y: 15.0 },
            Pin { name: "pc_in",  x: 0.0,  y: 35.0 },
            Pin { name: "pc_out", x: 90.0, y: 25.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="86" height="46" rx="6"/>
            <text class="label" x="45" y="30" text-anchor="middle">PC</text>
        "#,
    },
    SymbolDef {
        kind: "rv_imem", label: "Instr Memory", category: "RISC-V Pipeline", prefix: "U",
        bbox: (110.0, 80.0),
        pins: &[
            Pin { name: "addr",  x: 0.0,   y: 40.0 },
            Pin { name: "_",     x: 0.0,   y: 0.0 },
            Pin { name: "instr", x: 110.0, y: 40.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="106" height="76" rx="6"/>
            <text class="label" x="55" y="38" text-anchor="middle">I-MEM</text>
            <text class="label" x="55" y="50" text-anchor="middle">(IF)</text>
        "#,
    },
    SymbolDef {
        kind: "rv_decode", label: "Decode (ID)", category: "RISC-V Pipeline", prefix: "U",
        bbox: (100.0, 100.0),
        pins: &[
            Pin { name: "instr", x: 0.0,   y: 20.0 },
            Pin { name: "rs1",   x: 100.0, y: 20.0 },
            Pin { name: "rs2",   x: 100.0, y: 45.0 },
            Pin { name: "rd",    x: 100.0, y: 70.0 },
            Pin { name: "ctrl",  x: 100.0, y: 95.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="96" height="96" rx="6"/>
            <text class="label" x="50" y="52" text-anchor="middle">DECODE</text>
            <text class="label" x="50" y="64" text-anchor="middle">(ID)</text>
        "#,
    },
    SymbolDef {
        kind: "rv_regfile", label: "Register File", category: "RISC-V Pipeline", prefix: "U",
        bbox: (120.0, 110.0),
        pins: &[
            Pin { name: "rs1",    x: 0.0,   y: 20.0 },
            Pin { name: "rs2",    x: 0.0,   y: 45.0 },
            Pin { name: "rd",     x: 0.0,   y: 70.0 },
            Pin { name: "wdata",  x: 0.0,   y: 95.0 },
            Pin { name: "rdata1", x: 120.0, y: 30.0 },
            Pin { name: "rdata2", x: 120.0, y: 80.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="116" height="106" rx="6"/>
            <text class="label" x="60" y="50" text-anchor="middle">REGFILE</text>
            <text class="label" x="60" y="62" text-anchor="middle">32 x 32b</text>
        "#,
    },
    SymbolDef {
        kind: "rv_alu", label: "ALU (EX)", category: "RISC-V Pipeline", prefix: "U",
        bbox: (100.0, 80.0),
        pins: &[
            Pin { name: "A",  x: 0.0,   y: 20.0 },
            Pin { name: "B",  x: 0.0,   y: 60.0 },
            Pin { name: "op", x: 50.0,  y: 80.0 },
            Pin { name: "Y",  x: 100.0, y: 40.0 },
        ],
        body: r#"
            <polygon points="2,2 60,2 98,40 60,78 2,78 18,40"/>
            <text class="label" x="42" y="38" text-anchor="middle">ALU</text>
            <text class="label" x="42" y="50" text-anchor="middle">(EX)</text>
        "#,
    },
    SymbolDef {
        kind: "rv_dmem", label: "Data Memory", category: "RISC-V Pipeline", prefix: "U",
        bbox: (120.0, 90.0),
        pins: &[
            Pin { name: "addr", x: 0.0,   y: 25.0 },
            Pin { name: "din",  x: 0.0,   y: 65.0 },
            Pin { name: "dout", x: 120.0, y: 45.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="116" height="86" rx="6"/>
            <text class="label" x="60" y="44" text-anchor="middle">D-MEM</text>
            <text class="label" x="60" y="56" text-anchor="middle">(MEM)</text>
        "#,
    },
    SymbolDef {
        kind: "rv_imm", label: "Imm-Gen", category: "RISC-V Pipeline", prefix: "U",
        bbox: (90.0, 50.0),
        pins: &[
            Pin { name: "instr", x: 0.0,  y: 25.0 },
            Pin { name: "imm",   x: 90.0, y: 25.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="86" height="46" rx="6"/>
            <text class="label" x="45" y="30" text-anchor="middle">IMM-GEN</text>
        "#,
    },
    SymbolDef {
        kind: "rv_ctrl", label: "Control Unit", category: "RISC-V Pipeline", prefix: "U",
        bbox: (90.0, 60.0),
        pins: &[
            Pin { name: "opcode", x: 0.0,  y: 30.0 },
            Pin { name: "ctrl",   x: 90.0, y: 30.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="86" height="56" rx="6"/>
            <text class="label" x="45" y="34" text-anchor="middle">CTRL</text>
        "#,
    },
    SymbolDef {
        kind: "rv_mux2", label: "2:1 Mux", category: "RISC-V Pipeline", prefix: "U",
        bbox: (40.0, 60.0),
        pins: &[
            Pin { name: "0",   x: 0.0,  y: 15.0 },
            Pin { name: "1",   x: 0.0,  y: 45.0 },
            Pin { name: "sel", x: 20.0, y: 60.0 },
            Pin { name: "Y",   x: 40.0, y: 30.0 },
        ],
        body: r#"
            <polygon points="2,2 38,16 38,44 2,58"/>
            <text class="label" x="20" y="34" text-anchor="middle">MUX</text>
        "#,
    },
    SymbolDef {
        kind: "rv_add4", label: "Adder (+4)", category: "RISC-V Pipeline", prefix: "U",
        bbox: (60.0, 50.0),
        pins: &[
            Pin { name: "A", x: 0.0,  y: 15.0 },
            Pin { name: "B", x: 0.0,  y: 35.0 },
            Pin { name: "Y", x: 60.0, y: 25.0 },
        ],
        body: r#"
            <polygon points="2,2 58,16 58,34 2,48 18,25"/>
            <text class="label" x="34" y="29" text-anchor="middle">+4</text>
        "#,
    },

    // ------- PQC accelerator blocks -------
    SymbolDef {
        kind: "pqc_axi", label: "AXI Bridge", category: "PQC Accelerator", prefix: "U",
        bbox: (110.0, 80.0),
        pins: &[
            Pin { name: "clk",  x: 0.0,   y: 15.0 },
            Pin { name: "axi",  x: 0.0,   y: 45.0 },
            Pin { name: "ctrl", x: 110.0, y: 20.0 },
            Pin { name: "data", x: 110.0, y: 60.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="106" height="76" rx="6"/>
            <text class="label" x="55" y="38" text-anchor="middle">AXI-4</text>
            <text class="label" x="55" y="50" text-anchor="middle">Bridge</text>
        "#,
    },
    SymbolDef {
        kind: "pqc_ctrl", label: "PQC Controller", category: "PQC Accelerator", prefix: "U",
        bbox: (110.0, 90.0),
        pins: &[
            Pin { name: "cmd",    x: 0.0,   y: 25.0 },
            Pin { name: "status", x: 0.0,   y: 65.0 },
            Pin { name: "ctrl",   x: 110.0, y: 25.0 },
            Pin { name: "done",   x: 110.0, y: 65.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="106" height="86" rx="6"/>
            <text class="label" x="55" y="44" text-anchor="middle">PQC FSM</text>
            <text class="label" x="55" y="56" text-anchor="middle">(Kyber/Dilithium)</text>
        "#,
    },
    SymbolDef {
        kind: "pqc_trng", label: "TRNG", category: "PQC Accelerator", prefix: "U",
        bbox: (90.0, 60.0),
        pins: &[
            Pin { name: "en",   x: 0.0,  y: 30.0 },
            Pin { name: "seed", x: 90.0, y: 30.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="86" height="56" rx="6"/>
            <text class="label" x="45" y="28" text-anchor="middle">TRNG</text>
            <text class="label" x="45" y="40" text-anchor="middle">256-bit</text>
        "#,
    },
    SymbolDef {
        kind: "pqc_keccak", label: "Keccak-f[1600]", category: "PQC Accelerator", prefix: "U",
        bbox: (130.0, 100.0),
        pins: &[
            Pin { name: "in",   x: 0.0,   y: 25.0 },
            Pin { name: "ctrl", x: 0.0,   y: 75.0 },
            Pin { name: "out",  x: 130.0, y: 50.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="126" height="96" rx="6"/>
            <text class="label" x="65" y="46" text-anchor="middle">KECCAK-f</text>
            <text class="label" x="65" y="58" text-anchor="middle">SHA3 / SHAKE</text>
        "#,
    },
    SymbolDef {
        kind: "pqc_sampler", label: "CBD Sampler", category: "PQC Accelerator", prefix: "U",
        bbox: (110.0, 70.0),
        pins: &[
            Pin { name: "seed", x: 0.0,   y: 25.0 },
            Pin { name: "ctrl", x: 0.0,   y: 50.0 },
            Pin { name: "poly", x: 110.0, y: 35.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="106" height="66" rx="6"/>
            <text class="label" x="55" y="32" text-anchor="middle">SAMPLER</text>
            <text class="label" x="55" y="44" text-anchor="middle">CBD / Rejection</text>
        "#,
    },
    SymbolDef {
        kind: "pqc_ntt", label: "NTT Engine", category: "PQC Accelerator", prefix: "U",
        bbox: (130.0, 110.0),
        pins: &[
            Pin { name: "in",      x: 0.0,   y: 25.0 },
            Pin { name: "twiddle", x: 0.0,   y: 60.0 },
            Pin { name: "ctrl",    x: 0.0,   y: 90.0 },
            Pin { name: "out",     x: 130.0, y: 55.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="126" height="106" rx="6"/>
            <text class="label" x="65" y="50" text-anchor="middle">NTT / INTT</text>
            <text class="label" x="65" y="62" text-anchor="middle">n=256  q=3329</text>
        "#,
    },
    SymbolDef {
        kind: "pqc_polymul", label: "Poly Multiplier", category: "PQC Accelerator", prefix: "U",
        bbox: (120.0, 90.0),
        pins: &[
            Pin { name: "a",   x: 0.0,   y: 25.0 },
            Pin { name: "b",   x: 0.0,   y: 65.0 },
            Pin { name: "y",   x: 120.0, y: 30.0 },
            Pin { name: "acc", x: 120.0, y: 60.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="116" height="86" rx="6"/>
            <text class="label" x="60" y="40" text-anchor="middle">POINTWISE</text>
            <text class="label" x="60" y="52" text-anchor="middle">x mod q</text>
        "#,
    },
    SymbolDef {
        kind: "pqc_modred", label: "Mod-Q Reducer", category: "PQC Accelerator", prefix: "U",
        bbox: (90.0, 60.0),
        pins: &[
            Pin { name: "in",  x: 0.0,  y: 30.0 },
            Pin { name: "out", x: 90.0, y: 30.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="86" height="56" rx="6"/>
            <text class="label" x="45" y="28" text-anchor="middle">MOD-Q</text>
            <text class="label" x="45" y="40" text-anchor="middle">Barrett</text>
        "#,
    },
    SymbolDef {
        kind: "pqc_polyram", label: "Polynomial RAM", category: "PQC Accelerator", prefix: "U",
        bbox: (130.0, 110.0),
        pins: &[
            Pin { name: "addr", x: 0.0,   y: 25.0 },
            Pin { name: "din",  x: 0.0,   y: 55.0 },
            Pin { name: "we",   x: 0.0,   y: 85.0 },
            Pin { name: "dout", x: 130.0, y: 55.0 },
        ],
        body: r#"
            <rect x="2" y="2" width="126" height="106" rx="6"/>
            <text class="label" x="65" y="50" text-anchor="middle">POLY-RAM</text>
            <text class="label" x="65" y="62" text-anchor="middle">8 KB · 16-bank</text>
        "#,
    },
];

// ---------------------------------------------------------------------------
// Preset construction helpers
// ---------------------------------------------------------------------------

fn place(s: &mut Schematic, kind: &str, x: f64, y: f64, value: &str) -> u32 {
    let def = CHIP_LIB.lookup(kind).expect("symbol exists");
    let id = s.add_instance(kind, x, y, def.prefix);
    if !value.is_empty() {
        if let Some(i) = s.instances.iter_mut().find(|i| i.id == id) {
            i.value = value.into();
        }
    }
    id
}

fn pp(inst: u32, pin: u32) -> Endpoint {
    Endpoint::Pin { inst_id: inst, pin_idx: pin }
}

fn wire(s: &mut Schematic, a: Endpoint, b: Endpoint, net: Option<&str>) {
    let id = s.add_wire(a, b);
    if let Some(n) = net {
        if let Some(w) = s.wires.iter_mut().find(|w| w.id == id) {
            w.net = Some(n.into());
        }
    }
}

// ---------------------------------------------------------------------------
// Preset: CMOS NAND2 standard cell
// ---------------------------------------------------------------------------

fn preset_nand2() -> Schematic {
    let mut s = Schematic::default();
    // Power rails
    let vdd = place(&mut s, "vdd", 310.0, 30.0, "");
    let gnd = place(&mut s, "gnd", 310.0, 470.0, "");

    // Pull-up: M1, M2 (PMOS) parallel between VDD and Y
    let m1 = place(&mut s, "pmos", 200.0, 110.0, "W=0.84u L=0.13u");
    let m2 = place(&mut s, "pmos", 400.0, 110.0, "W=0.84u L=0.13u");

    // Pull-down: M3, M4 (NMOS) series between Y and GND
    let m3 = place(&mut s, "nmos", 290.0, 230.0, "W=0.55u L=0.13u");
    let m4 = place(&mut s, "nmos", 290.0, 350.0, "W=0.55u L=0.13u");

    // Ports
    let a = place(&mut s, "port_in",  60.0, 121.0, "");
    let b = place(&mut s, "port_in",  60.0, 281.0, "");
    let y = place(&mut s, "port_out", 520.0, 161.0, "");

    // VDD bus → PMOS sources
    wire(&mut s, pp(vdd, 0), pp(m1, 2), Some("VDD"));
    wire(&mut s, pp(vdd, 0), pp(m2, 2), Some("VDD"));
    // PMOS drains → Y node
    wire(&mut s, pp(m1, 1), pp(m2, 1), Some("Y"));
    wire(&mut s, pp(m1, 1), pp(m3, 1), Some("Y"));
    wire(&mut s, pp(m1, 1), pp(y, 0),  Some("Y"));
    // NMOS series: M3.S — M4.D
    wire(&mut s, pp(m3, 2), pp(m4, 1), Some("N1"));
    // M4.S → GND
    wire(&mut s, pp(m4, 2), pp(gnd, 0), Some("GND"));
    // Gates: A → M1.G & M3.G; B → M2.G & M4.G
    wire(&mut s, pp(a, 0), pp(m1, 0), Some("A"));
    wire(&mut s, pp(a, 0), pp(m3, 0), Some("A"));
    wire(&mut s, pp(b, 0), pp(m2, 0), Some("B"));
    wire(&mut s, pp(b, 0), pp(m4, 0), Some("B"));
    s
}

// ---------------------------------------------------------------------------
// Preset: SQC-Q5 transmon unit cell
// ---------------------------------------------------------------------------

fn preset_sqc_q5() -> Schematic {
    let mut s = Schematic::default();

    // X-mon island in the centre
    let q1 = place(&mut s, "xmon", 300.0, 200.0, "Q1 (X-mon)");
    // Josephson junction between island (S pin) and ground
    let j1 = place(&mut s, "jj", 290.0, 290.0, "EJ=12 GHz");
    let g1 = place(&mut s, "gnd", 295.0, 350.0, "");
    wire(&mut s, pp(q1, 1), pp(j1, 0), Some("ISL"));
    wire(&mut s, pp(j1, 1), pp(g1, 0), Some("GND"));

    // Shunt capacitor on the N arm to ground
    let cs = place(&mut s, "cap", 310.0, 100.0, "C_shunt 70 fF");
    let g2 = place(&mut s, "gnd", 315.0, 40.0, "");
    wire(&mut s, pp(q1, 0), pp(cs, 1), Some("ISL"));
    wire(&mut s, pp(cs, 0), pp(g2, 0), Some("GND"));

    // Readout: coupling cap → λ/4 resonator → readout bus port
    let cc  = place(&mut s, "cap",       400.0, 215.0, "C_c 4 fF");
    let res = place(&mut s, "resonator", 470.0, 210.0, "ν=7.2 GHz");
    let rd  = place(&mut s, "port_out",  600.0, 216.0, "");
    wire(&mut s, pp(q1, 3),  pp(cc, 0),  Some("RO"));
    wire(&mut s, pp(cc, 1),  pp(res, 0), Some("RO"));
    wire(&mut s, pp(res, 1), pp(rd, 0),  Some("RO_BUS"));

    // XY drive on the W arm
    let cxy = place(&mut s, "cap",      170.0, 215.0, "C_xy 1 fF");
    let xy  = place(&mut s, "port_in",  60.0,  216.0, "");
    wire(&mut s, pp(xy, 0),  pp(cxy, 0), Some("XY_DRIVE"));
    wire(&mut s, pp(cxy, 1), pp(q1, 2),  Some("XY_DRIVE"));
    s
}

// ---------------------------------------------------------------------------
// Preset: RISC-V RV32I 5-stage pipeline (block diagram)
// ---------------------------------------------------------------------------

fn preset_riscv() -> Schematic {
    let mut s = Schematic::default();

    // ------ IF stage ------
    let pc   = place(&mut s, "rv_pc",   80.0,  220.0, "RV32I");
    let imem = place(&mut s, "rv_imem", 230.0, 200.0, "16 KB");
    let add4 = place(&mut s, "rv_add4", 80.0,  90.0,  "+4");

    // PC -> IMEM.addr
    wire(&mut s, pp(pc, 2), pp(imem, 0), Some("PC"));
    // PC -> Adder.A ; Adder.Y -> PC.pc_in
    wire(&mut s, pp(pc, 2),  pp(add4, 0), Some("PC"));
    wire(&mut s, pp(add4, 2), pp(pc, 1),  Some("PC+4"));

    // ------ ID stage ------
    let dec  = place(&mut s, "rv_decode",  410.0, 190.0, "");
    let rf   = place(&mut s, "rv_regfile", 580.0, 280.0, "");
    let imm  = place(&mut s, "rv_imm",     410.0, 380.0, "");
    let ctrl = place(&mut s, "rv_ctrl",    410.0, 470.0, "");

    // IMEM.instr -> Decode.instr, Imm.instr, Ctrl.opcode
    wire(&mut s, pp(imem, 2), pp(dec, 0),  Some("INSTR"));
    wire(&mut s, pp(imem, 2), pp(imm, 0),  Some("INSTR"));
    wire(&mut s, pp(imem, 2), pp(ctrl, 0), Some("INSTR"));
    // Decode -> RegFile read/write addresses
    wire(&mut s, pp(dec, 1), pp(rf, 0), Some("rs1"));
    wire(&mut s, pp(dec, 2), pp(rf, 1), Some("rs2"));
    wire(&mut s, pp(dec, 3), pp(rf, 2), Some("rd"));

    // ------ EX stage ------
    let mux_b = place(&mut s, "rv_mux2", 780.0, 320.0, "ALUSrc");
    let alu   = place(&mut s, "rv_alu",  860.0, 280.0, "");

    // rdata1 -> ALU.A
    wire(&mut s, pp(rf, 4), pp(alu, 0), Some("OPA"));
    // rdata2 -> mux.0 ; imm -> mux.1 ; mux.Y -> ALU.B
    wire(&mut s, pp(rf, 5),  pp(mux_b, 0), Some("OPB_REG"));
    wire(&mut s, pp(imm, 1), pp(mux_b, 1), Some("OPB_IMM"));
    wire(&mut s, pp(mux_b, 3), pp(alu, 1), Some("OPB"));
    // CTRL -> ALU.op + mux.sel
    wire(&mut s, pp(ctrl, 1), pp(alu, 2),  Some("ALU_OP"));
    wire(&mut s, pp(ctrl, 1), pp(mux_b, 2), Some("CTRL"));

    // ------ MEM stage ------
    let dmem = place(&mut s, "rv_dmem", 1010.0, 280.0, "16 KB");
    wire(&mut s, pp(alu, 3), pp(dmem, 0), Some("ADDR"));
    wire(&mut s, pp(rf, 5),  pp(dmem, 1), Some("STORE"));

    // ------ WB stage ------
    let mux_wb = place(&mut s, "rv_mux2", 1170.0, 310.0, "MemToReg");
    wire(&mut s, pp(alu, 3),  pp(mux_wb, 0), Some("WB_ALU"));
    wire(&mut s, pp(dmem, 2), pp(mux_wb, 1), Some("WB_MEM"));
    wire(&mut s, pp(ctrl, 1), pp(mux_wb, 2), Some("CTRL"));
    wire(&mut s, pp(mux_wb, 3), pp(rf, 3),   Some("WB_DATA"));

    // External clock / reset ports
    let clk = place(&mut s, "port_in", 0.0, 235.0, "");
    wire(&mut s, pp(clk, 0), pp(pc, 0), Some("CLK"));
    s
}

// ---------------------------------------------------------------------------
// Preset: PQC accelerator — ML-KEM (Kyber-768) / ML-DSA (Dilithium) dataflow
// ---------------------------------------------------------------------------

fn preset_pqc() -> Schematic {
    let mut s = Schematic::default();

    // Host interface column
    let clk = place(&mut s, "port_in", 0.0,   100.0, "");
    let axi = place(&mut s, "port_in", 0.0,   200.0, "");
    let irq = place(&mut s, "port_out", 0.0,  920.0, "");

    let bridge = place(&mut s, "pqc_axi",   120.0, 130.0, "AXI4-Lite");
    wire(&mut s, pp(clk, 0), pp(bridge, 0), Some("CLK"));
    wire(&mut s, pp(axi, 0), pp(bridge, 1), Some("AXI"));

    // Controller / FSM column
    let ctrl = place(&mut s, "pqc_ctrl", 280.0, 130.0, "Kyber-768");
    wire(&mut s, pp(bridge, 2), pp(ctrl, 0), Some("CMD"));
    wire(&mut s, pp(bridge, 3), pp(ctrl, 1), Some("STATUS"));

    // Entropy / hashing column
    let trng   = place(&mut s, "pqc_trng",   460.0, 60.0,  "");
    let keccak = place(&mut s, "pqc_keccak", 460.0, 200.0, "1600-bit state");
    wire(&mut s, pp(ctrl, 2), pp(trng, 0),   Some("CTRL"));
    wire(&mut s, pp(trng, 1), pp(keccak, 0), Some("SEED"));
    wire(&mut s, pp(ctrl, 2), pp(keccak, 1), Some("CTRL"));

    // Sampler — turns SHAKE output into polynomial coefficients
    let sampler = place(&mut s, "pqc_sampler", 660.0, 220.0, "η=2");
    wire(&mut s, pp(keccak, 2),  pp(sampler, 0), Some("XOF"));
    wire(&mut s, pp(ctrl, 2),    pp(sampler, 1), Some("CTRL"));

    // Polynomial RAM
    let ram = place(&mut s, "pqc_polyram", 660.0, 420.0, "16 banks");
    wire(&mut s, pp(sampler, 2), pp(ram, 1), Some("POLY"));
    wire(&mut s, pp(ctrl, 2),    pp(ram, 0), Some("ADDR"));
    wire(&mut s, pp(ctrl, 2),    pp(ram, 2), Some("WE"));

    // NTT / INTT engine
    let ntt = place(&mut s, "pqc_ntt", 860.0, 380.0, "radix-2 BFU");
    wire(&mut s, pp(ram, 3),  pp(ntt, 0), Some("POLY_T"));
    wire(&mut s, pp(ctrl, 2), pp(ntt, 1), Some("TW_ROM"));
    wire(&mut s, pp(ctrl, 2), pp(ntt, 2), Some("CTRL"));

    // Pointwise polynomial multiplier (in NTT domain)
    let mul = place(&mut s, "pqc_polymul", 1050.0, 380.0, "256 lanes");
    wire(&mut s, pp(ntt, 3), pp(mul, 0), Some("A_NTT"));
    wire(&mut s, pp(ram, 3), pp(mul, 1), Some("B_NTT"));

    // Modular reducer on the multiplier output (Barrett / Montgomery)
    let modq = place(&mut s, "pqc_modred", 1210.0, 395.0, "q=3329");
    wire(&mut s, pp(mul, 2), pp(modq, 0), Some("C"));
    // Result back to poly RAM (writeback)
    wire(&mut s, pp(modq, 1), pp(ram, 1), Some("C_RED"));
    // Accumulator path also reduced
    wire(&mut s, pp(mul, 3),  pp(modq, 0), Some("ACC"));

    // Done / IRQ out
    wire(&mut s, pp(ctrl, 3), pp(irq, 0), Some("DONE"));
    s
}

fn build_preset(id: &str) -> (String, String, Schematic) {
    match id {
        "nand2" => (
            "CMOS NAND2 — Standard Cell".into(),
            "sky130A · transistor-level · 2 PMOS pull-up · 2 NMOS pull-down".into(),
            preset_nand2(),
        ),
        "riscv-core" | "riscv-core-v1" | "riscv" => (
            "RISC-V RV32I — 5-Stage Pipeline".into(),
            "Block diagram · IF / ID / EX / MEM / WB".into(),
            preset_riscv(),
        ),
        "pqc-kyber" | "pqc" => (
            "PQC Accelerator — Kyber/Dilithium KEM".into(),
            "ML-KEM 768 dataflow · Keccak + NTT + Poly-Mul + CBD sampler".into(),
            preset_pqc(),
        ),
        _ => (
            "SQC-Q5 — Transmon Unit Cell".into(),
            "Superconducting qubit · JJ + shunt cap + λ/4 readout · XY drive".into(),
            preset_sqc_q5(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Page component
// ---------------------------------------------------------------------------

#[component]
pub fn SiliconSchematicPage() -> impl IntoView {
    let qmap = use_query_map();
    let initial = qmap
        .with_untracked(|q| q.get("project").map(|s| s.to_string()))
        .unwrap_or_else(|| "nand2".to_string());

    let project = RwSignal::new(initial);

    // Pick preset reactively by remounting editor when project changes.
    view! {
        <div style="display:flex; flex-direction:column; height:100%;">
            <div style="display:flex; gap:.5rem; align-items:center; padding:.55rem .85rem;
                background:#0c1322; border-bottom:1px solid #1d2740;">
                <span style="font-size:.78rem; color:#8c98b3;">"Preset:"</span>
                <select
                    style="background:#1a2540; color:#dbe2f1; border:1px solid #2a3654;
                        border-radius:5px; padding:.3rem .5rem; font-size:.8rem;"
                    on:change=move |ev| project.set(event_target_value(&ev))
                    prop:value=move || project.get()
                >
                    <option value="nand2">"CMOS NAND2 (standard cell)"</option>
                    <option value="sqc-q5">"SQC-Q5 transmon unit cell"</option>
                    <option value="riscv-core">"RISC-V RV32I (5-stage pipeline)"</option>
                    <option value="pqc-kyber">"PQC Accelerator (Kyber/Dilithium KEM)"</option>
                </select>
                <span style="margin-left:auto; font-size:.72rem; color:#7e8db0;">
                    "Drag symbols from the left palette · click pins to wire · scroll to zoom"
                </span>
            </div>

            {move || {
                let id = project.get();
                let (title, subtitle, preset) = build_preset(&id);
                view! {
                    <SchematicEditor
                        title=title
                        subtitle=subtitle
                        library=&CHIP_LIB
                        preset=preset
                    />
                }
            }}
        </div>
    }
}
