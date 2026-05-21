use leptos::prelude::*;

#[derive(Clone, Copy)]
struct RtlFile {
    path: &'static str,
    module: &'static str,
    lines: u32,
    warnings: u32,
}

const FILES: &[RtlFile] = &[
    RtlFile { path: "rtl/top.v",          module: "top",          lines: 142, warnings: 0 },
    RtlFile { path: "rtl/core/cpu.v",     module: "cpu",          lines: 612, warnings: 2 },
    RtlFile { path: "rtl/core/alu.v",     module: "alu",          lines: 188, warnings: 0 },
    RtlFile { path: "rtl/core/regfile.v", module: "regfile",      lines: 96,  warnings: 0 },
    RtlFile { path: "rtl/bus/axi4_lite.v",module: "axi4_lite",    lines: 274, warnings: 1 },
    RtlFile { path: "rtl/periph/uart.v",  module: "uart",         lines: 210, warnings: 0 },
    RtlFile { path: "rtl/periph/gpio.v",  module: "gpio",         lines: 88,  warnings: 0 },
];

const SAMPLE_RTL: &str = r#"// rtl/core/alu.v — RV32I integer ALU
`timescale 1ns/1ps

module alu #(
    parameter integer WIDTH = 32
) (
    input  wire [WIDTH-1:0] a,
    input  wire [WIDTH-1:0] b,
    input  wire [3:0]       op,
    output reg  [WIDTH-1:0] y,
    output wire             zero
);

    localparam OP_ADD = 4'h0;
    localparam OP_SUB = 4'h1;
    localparam OP_AND = 4'h2;
    localparam OP_OR  = 4'h3;
    localparam OP_XOR = 4'h4;
    localparam OP_SLL = 4'h5;
    localparam OP_SRL = 4'h6;
    localparam OP_SRA = 4'h7;
    localparam OP_SLT = 4'h8;

    always @* begin
        case (op)
            OP_ADD: y = a + b;
            OP_SUB: y = a - b;
            OP_AND: y = a & b;
            OP_OR : y = a | b;
            OP_XOR: y = a ^ b;
            OP_SLL: y = a << b[4:0];
            OP_SRL: y = a >> b[4:0];
            OP_SRA: y = $signed(a) >>> b[4:0];
            OP_SLT: y = ($signed(a) < $signed(b)) ? 32'd1 : 32'd0;
            default: y = {WIDTH{1'b0}};
        endcase
    end

    assign zero = (y == {WIDTH{1'b0}});

endmodule
"#;

#[component]
pub fn SiliconRtlPage() -> impl IntoView {
    let selected = RwSignal::new("rtl/core/alu.v".to_string());

    view! {
        <div class="page-container silicon-rtl-page">
            <div class="page-header">
                <div>
                    <span class="section-kicker">"Chip Design · RTL"</span>
                    <h1>"RTL Workspace"</h1>
                    <p class="page-subtitle">"Browse module hierarchy, edit HDL, and review lint diagnostics."</p>
                </div>
                <div class="rtl-actions">
                    <a href="/silicon/schematic?project=riscv-core" class="btn btn-ghost btn-sm">"View Schematic"</a>
                    <a href="/silicon/stackup?project=riscv-core-v1" class="btn btn-ghost btn-sm">"View 3D Stack-up"</a>
                    <button class="btn btn-ghost btn-sm">"Run Lint"</button>
                    <button class="btn btn-ghost btn-sm">"Compile"</button>
                    <button class="btn btn-primary btn-sm">"Simulate"</button>
                </div>
            </div>

            <div class="rtl-layout">
                <aside class="rtl-tree">
                    <div class="rtl-tree-header">"Modules"</div>
                    {FILES.iter().map(|f| {
                        let path = f.path;
                        let is_selected = {
                            let selected = selected;
                            move || selected.get() == path
                        };
                        view! {
                            <button
                                class="rtl-tree-item"
                                class:active=is_selected
                                on:click=move |_| selected.set(path.to_string())
                            >
                                <span class="rtl-tree-module">{f.module}</span>
                                <span class="rtl-tree-meta">
                                    {format!("{}L", f.lines)}
                                    {(f.warnings > 0).then(|| view! { <span class="rtl-warn">{format!(" · {}⚠", f.warnings)}</span> })}
                                </span>
                                <span class="rtl-tree-path">{f.path}</span>
                            </button>
                        }
                    }).collect::<Vec<_>>()}
                </aside>

                <section class="rtl-editor">
                    <div class="rtl-editor-tabs">
                        <span class="rtl-tab active">{move || selected.get()}</span>
                        <span class="rtl-editor-status">"Verilator · clean · 0 errors · 0 warnings"</span>
                    </div>
                    <pre class="rtl-code"><code>{SAMPLE_RTL}</code></pre>
                </section>

                <aside class="rtl-diagnostics">
                    <div class="rtl-tree-header">"Diagnostics"</div>
                    <ul class="rtl-diag-list">
                        <li class="diag warn">
                            <span class="diag-loc">"cpu.v:218"</span>
                            <span class="diag-msg">"Implicit wire `mem_stall` (lint-style)"</span>
                        </li>
                        <li class="diag warn">
                            <span class="diag-loc">"cpu.v:402"</span>
                            <span class="diag-msg">"Latch inferred in always block"</span>
                        </li>
                        <li class="diag info">
                            <span class="diag-loc">"axi4_lite.v:91"</span>
                            <span class="diag-msg">"Unused signal `prot[2]`"</span>
                        </li>
                        <li class="diag ok">
                            <span class="diag-loc">"alu.v"</span>
                            <span class="diag-msg">"Clean (Verilator + Slang)"</span>
                        </li>
                    </ul>
                </aside>
            </div>
        </div>

        <style>{r#"
            .silicon-rtl-page { padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; }
            .silicon-rtl-page .page-header { display: flex; justify-content: space-between; align-items: flex-start; gap: 1rem; }
            .silicon-rtl-page .page-header h1 { margin: 0; }
            .silicon-rtl-page .page-subtitle { margin: 0.35rem 0 0; color: var(--text-secondary); }
            .rtl-actions { display: flex; gap: 0.5rem; }
            .rtl-layout {
                display: grid;
                grid-template-columns: 260px minmax(0,1fr) 280px;
                gap: 0.75rem;
                min-height: 60vh;
            }
            .rtl-tree, .rtl-diagnostics, .rtl-editor {
                background: var(--bg-card);
                border: 1px solid var(--border);
                border-radius: 10px;
                display: flex;
                flex-direction: column;
                overflow: hidden;
            }
            .rtl-tree-header {
                padding: 0.6rem 0.85rem;
                font-size: 0.75rem;
                color: var(--text-muted);
                text-transform: uppercase;
                letter-spacing: 0.08em;
                border-bottom: 1px solid var(--border);
            }
            .rtl-tree-item {
                appearance: none;
                background: transparent;
                border: none;
                color: var(--text-secondary);
                text-align: left;
                padding: 0.55rem 0.85rem;
                display: grid;
                grid-template-columns: 1fr auto;
                grid-template-areas: "mod meta" "path path";
                gap: 0.15rem 0.5rem;
                cursor: pointer;
                border-left: 2px solid transparent;
            }
            .rtl-tree-item:hover { background: var(--bg-hover); color: var(--text-primary); }
            .rtl-tree-item.active { background: rgba(59,130,246,0.12); color: var(--text-primary); border-left-color: var(--accent); }
            .rtl-tree-module { grid-area: mod; font-weight: 600; }
            .rtl-tree-meta { grid-area: meta; color: var(--text-muted); font-size: 0.72rem; }
            .rtl-tree-path { grid-area: path; color: var(--text-muted); font-family: var(--font-mono); font-size: 0.72rem; }
            .rtl-warn { color: var(--warning); }
            .rtl-editor-tabs {
                display: flex;
                align-items: center;
                justify-content: space-between;
                padding: 0.5rem 0.85rem;
                border-bottom: 1px solid var(--border);
                background: var(--bg-secondary);
            }
            .rtl-tab { font-family: var(--font-mono); font-size: 0.8rem; color: var(--text-primary); }
            .rtl-editor-status { color: var(--text-muted); font-size: 0.75rem; }
            .rtl-code {
                margin: 0;
                padding: 0.85rem 1rem;
                font-family: var(--font-mono);
                font-size: 0.82rem;
                color: var(--text-primary);
                white-space: pre;
                overflow: auto;
                flex: 1;
                line-height: 1.5;
            }
            .rtl-diag-list { list-style: none; margin: 0; padding: 0; overflow: auto; }
            .diag {
                display: grid;
                grid-template-columns: 1fr;
                gap: 0.1rem;
                padding: 0.55rem 0.85rem;
                border-bottom: 1px solid var(--border);
            }
            .diag-loc { font-family: var(--font-mono); font-size: 0.72rem; color: var(--text-muted); }
            .diag-msg { font-size: 0.82rem; color: var(--text-primary); }
            .diag.warn .diag-msg { color: var(--warning); }
            .diag.info .diag-msg { color: var(--info); }
            .diag.ok .diag-msg { color: var(--success); }

            @media (max-width: 1100px) {
                .rtl-layout { grid-template-columns: 220px minmax(0,1fr); }
                .rtl-diagnostics { grid-column: 1 / -1; }
            }
            @media (max-width: 760px) {
                .rtl-layout { grid-template-columns: 1fr; }
            }
        "#}</style>
    }
}
