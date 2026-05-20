use leptos::prelude::*;

use crate::board_config::{accessory_profile, ControllerProfile};

/// Renders a catalog-driven board layout as SVG inner content (no outer `<svg>` shell).
/// Returns `None` if the profile has no `visual_layout` defined.
pub fn render_catalog_board_layout(profile: &ControllerProfile, chip_label: &str) -> Option<AnyView> {
    let layout = profile.visual_layout.as_ref()?;
    let module = &layout.module;
    let chip_label_text = chip_label.to_string();
    let footer_label = layout.footer_label.clone();
    let footer_text_fill = layout
        .footer_text_fill
        .clone()
        .unwrap_or_else(|| String::from("#dfe7dc"));

    Some(
        view! {
            <>
                {layout.headers.iter().map(|header| {
                    let label = header.label.clone();
                    let base_fill = header.base_fill.clone();
                    let base_stroke = header.base_stroke.clone();
                    let pin_fill = header.pin_fill.clone();
                    let text_fill = header.text_fill.clone().unwrap_or_else(|| String::from("#d5f5d2"));
                    let label_x = header.label_x.unwrap_or(header.x + ((header.columns as f32 * header.pitch_x) / 2.0));
                    let label_y = header.label_y.unwrap_or(header.y - 4.0);
                    let width = (header.columns as f32 - 1.0) * header.pitch_x + header.pin_width + 12.0;
                    let height = (header.rows as f32 - 1.0) * header.pitch_y + header.pin_height + 10.0;
                    view! {
                        <>
                            <rect
                                x={header.x - 6.0}
                                y={header.y - 4.0}
                                width={width}
                                height={height}
                                rx="4"
                                fill={base_fill}
                                stroke={base_stroke}
                                stroke-width="1"
                            />
                            {(0..header.columns).map(|column| {
                                let pin_fill_for_column = pin_fill.clone();
                                (0..header.rows).map(move |row| {
                                    let pin_x = header.x + column as f32 * header.pitch_x;
                                    let pin_y = header.y + row as f32 * header.pitch_y;
                                    let _pin_fill = pin_fill_for_column.clone();
                                    let pin_rx = header.pin_corner_radius.unwrap_or(0.8);
                                    // The catalog's "pin" rect is the solder pad itself. Render it
                                    // as a polished gold pad (gradient + outline + top-edge sheen)
                                    // with a dark drilled through-hole in the center — looks like a
                                    // real PTH (plated through-hole) on top of the soldermask.
                                    let cx = pin_x + header.pin_width / 2.0;
                                    let cy = pin_y + header.pin_height / 2.0;
                                    let hole_r = (header.pin_width.min(header.pin_height) * 0.30).max(0.6);
                                    view! {
                                        <>
                                            // Pad shadow under the copper to lift it off the board
                                            <rect
                                                x={pin_x + 0.4}
                                                y={pin_y + 0.6}
                                                width={header.pin_width}
                                                height={header.pin_height}
                                                rx={pin_rx}
                                                fill="rgba(0,0,0,0.55)"
                                            />
                                            // Gold pad
                                            <rect
                                                x={pin_x}
                                                y={pin_y}
                                                width={header.pin_width}
                                                height={header.pin_height}
                                                rx={pin_rx}
                                                fill="url(#pcb-pad-gold)"
                                                stroke="#3b2a08"
                                                stroke-width="0.5"
                                            />
                                            // Top sheen band (thin highlight on the upper edge)
                                            <rect
                                                x={pin_x + 0.5}
                                                y={pin_y + 0.5}
                                                width={(header.pin_width - 1.0).max(0.4)}
                                                height={(header.pin_height * 0.32).max(0.4)}
                                                rx={(pin_rx - 0.2).max(0.2)}
                                                fill="rgba(255,255,255,0.45)"
                                            />
                                            // Dark drilled hole — the actual through-hole
                                            <circle cx={cx} cy={cy} r={hole_r + 0.4} fill="#0a0e16"/>
                                            <circle cx={cx} cy={cy} r={hole_r} fill="#04070c" stroke="rgba(0,0,0,0.9)" stroke-width="0.3"/>
                                            // Tiny inner-rim highlight on the hole (suggests depth)
                                            <circle cx={cx - hole_r * 0.25} cy={cy - hole_r * 0.25} r={hole_r * 0.35} fill="rgba(255,255,255,0.18)"/>
                                        </>
                                    }
                                }).collect::<Vec<_>>()
                            }).collect::<Vec<_>>()}
                            <text x={label_x} y={label_y} text-anchor="middle" fill={text_fill} font-size="6" font-family="monospace">{label}</text>
                        </>
                    }
                }).collect::<Vec<_>>()}

                // ── IC chip body: dark plastic with pin teeth + pin-1 notch ──
                <g filter="url(#chip-shadow)">
                    // Pin teeth on left and right edges (8 per side, evenly spaced)
                    {{
                        let teeth = 8;
                        let tooth_w = 1.6_f32;
                        let tooth_h = (module.height / (teeth as f32 * 1.6)).max(2.0);
                        let step = module.height / (teeth as f32 + 1.0);
                        (1..=teeth).map(|i| {
                            let ty = module.y + i as f32 * step - tooth_h / 2.0;
                            view! {
                                <>
                                    <rect x={module.x - tooth_w + 0.4} y={ty} width={tooth_w} height={tooth_h} rx="0.3" fill="#cdd5e0" stroke="#3a4250" stroke-width="0.2"/>
                                    <rect x={module.x + module.width - 0.4} y={ty} width={tooth_w} height={tooth_h} rx="0.3" fill="#cdd5e0" stroke="#3a4250" stroke-width="0.2"/>
                                </>
                            }
                        }).collect::<Vec<_>>()
                    }}
                    // Chip body
                    <rect
                        x={module.x}
                        y={module.y}
                        width={module.width}
                        height={module.height}
                        rx={module.radius}
                        fill="url(#pcb-chip-body)"
                        stroke={module.stroke.clone()}
                        stroke-width="0.8"
                    />
                    // Top sheen overlay (ceramic finish)
                    <rect
                        x={module.x + 1.5}
                        y={module.y + 1.5}
                        width={module.width - 3.0}
                        height={(module.height * 0.32).max(1.0)}
                        rx={(module.radius - 0.6).max(0.4)}
                        fill="rgba(255,255,255,0.05)"
                    />
                    // Pin-1 notch — small dark circle on top-left
                    <circle cx={module.x + 4.0} cy={module.y + 4.0} r="1.6" fill="#04070c" stroke="rgba(255,255,255,0.18)" stroke-width="0.3"/>
                </g>
                <text x={module.x + module.width / 2.0} y={module.y + (module.height / 2.0) + 4.0} text-anchor="middle" fill={module.text_fill.clone().unwrap_or_else(|| String::from("#e7f5df"))} font-size={module.font_size.unwrap_or(7.5)} font-family="monospace">{chip_label_text.clone()}</text>

                {layout.ports.iter().map(|block| {
                    let label = block.label.clone();
                    let fill = block.fill.clone();
                    let stroke = block.stroke.clone();
                    let text_fill = block.text_fill.clone().unwrap_or_else(|| String::from("#374151"));
                    let font_size = block.font_size.unwrap_or(5.4);
                    let label_lower = label.to_lowercase();
                    let is_usb = label_lower.contains("usb") || label_lower.contains("type-c") || label_lower.contains("type c");
                    if is_usb {
                        // Realistic USB-C shell: outer metallic shroud + inner dark cavity with
                        // rounded lozenge mouth + two tiny contact lines. Label below the shell.
                        let cavity_x = block.x + 2.5;
                        let cavity_y = block.y + 2.5;
                        let cavity_w = (block.width - 5.0).max(3.0);
                        let cavity_h = (block.height - 5.0).max(2.0);
                        let cavity_r = (cavity_h / 2.0).min(cavity_w / 2.0);
                        view! {
                            <>
                                // Metallic shroud
                                <rect x={block.x} y={block.y} width={block.width} height={block.height} rx={block.radius} fill="url(#pcb-usbc-shell)" stroke="#04070c" stroke-width="0.6" filter="url(#chip-shadow)"/>
                                // Top highlight strip
                                <rect x={block.x + 1.0} y={block.y + 0.8} width={(block.width - 2.0).max(1.0)} height="1.2" rx="0.6" fill="rgba(255,255,255,0.55)"/>
                                // Bottom shadow strip
                                <rect x={block.x + 1.0} y={block.y + block.height - 1.6} width={(block.width - 2.0).max(1.0)} height="1.0" rx="0.5" fill="rgba(0,0,0,0.5)"/>
                                // Dark inner cavity (the port mouth)
                                <rect x={cavity_x} y={cavity_y} width={cavity_w} height={cavity_h} rx={cavity_r} fill="#04070c" stroke="rgba(255,255,255,0.08)" stroke-width="0.3"/>
                                // Center contact tongue
                                <rect x={cavity_x + cavity_w * 0.2} y={cavity_y + cavity_h * 0.42} width={cavity_w * 0.6} height={cavity_h * 0.16} rx="0.4" fill="#1c2230"/>
                                // Side anchor tabs
                                <rect x={block.x - 1.4} y={block.y + block.height * 0.25} width="1.6" height={block.height * 0.5} rx="0.4" fill="url(#pcb-usbc-shell)" stroke="#04070c" stroke-width="0.4"/>
                                <rect x={block.x + block.width - 0.2} y={block.y + block.height * 0.25} width="1.6" height={block.height * 0.5} rx="0.4" fill="url(#pcb-usbc-shell)" stroke="#04070c" stroke-width="0.4"/>
                                <text x={block.x + block.width / 2.0} y={block.y + block.height + 5.5} text-anchor="middle" fill={text_fill} font-size={font_size} font-family="monospace">{label}</text>
                            </>
                        }.into_any()
                    } else {
                        view! {
                            <>
                                <rect x={block.x} y={block.y} width={block.width} height={block.height} rx={block.radius} fill={fill} stroke={stroke} stroke-width="1"/>
                                <text x={block.x + block.width / 2.0} y={block.y + (block.height / 2.0) + 4.0} text-anchor="middle" fill={text_fill} font-size={font_size} font-family="monospace">{label}</text>
                            </>
                        }.into_any()
                    }
                }).collect::<Vec<_>>()}

                {layout.power.iter().map(|block| {
                    let label = block.label.clone();
                    let fill = block.fill.clone();
                    let stroke = block.stroke.clone();
                    let text_fill = block.text_fill.clone().unwrap_or_else(|| String::from("#d8f6d2"));
                    let font_size = block.font_size.unwrap_or(5.2);
                    view! {
                        <>
                            <rect x={block.x} y={block.y} width={block.width} height={block.height} rx={block.radius} fill={fill} stroke={stroke} stroke-width="1"/>
                            <text x={block.x + block.width / 2.0} y={block.y + (block.height / 2.0) + 4.0} text-anchor="middle" fill={text_fill} font-size={font_size} font-family="monospace">{label}</text>
                        </>
                    }
                }).collect::<Vec<_>>()}

                {layout.accessories.iter().filter_map(|mount| {
                    accessory_profile(&mount.accessory_id).map(|accessory| {
                        let accessory_label = format!("{} {}", mount.label, accessory.interface);
                        let tooltip = format!("{}: {}", accessory.label, accessory.summary);
                        view! {
                            <>
                                <rect
                                    x={mount.x}
                                    y={mount.y}
                                    width={mount.width}
                                    height={mount.height}
                                    rx={mount.radius}
                                    fill={accessory.accent.clone()}
                                    fill-opacity="0.2"
                                    stroke={accessory.accent.clone()}
                                    stroke-width="0.9"
                                >
                                    <title>{tooltip}</title>
                                </rect>
                                <text
                                    x={mount.x + mount.width / 2.0}
                                    y={mount.y + (mount.height / 2.0) + 3.8}
                                    text-anchor="middle"
                                    fill={accessory.accent.clone()}
                                    font-size="4.7"
                                    font-family="monospace"
                                >
                                    {accessory_label}
                                </text>
                            </>
                        }
                    })
                }).collect::<Vec<_>>()}

                {layout.indicators.iter().map(|indicator| {
                    let label = indicator.label.clone();
                    let text_fill = indicator.text_fill.clone().unwrap_or_else(|| String::from("#d6f7d0"));
                    let body_fill = indicator.fill.clone();
                    let r = indicator.radius;
                    view! {
                        <>
                            // Solder pad ring under the LED
                            <circle cx={indicator.x} cy={indicator.y} r={r + 1.4} fill="url(#pcb-pad-gold)" stroke="#3b2a08" stroke-width="0.4"/>
                            // LED dome — body color
                            <circle cx={indicator.x} cy={indicator.y} r={r} fill={body_fill} stroke={indicator.stroke.clone()} stroke-width="0.6"/>
                            // Domed shading (light from top-left, shadow bottom-right)
                            <circle cx={indicator.x} cy={indicator.y} r={r} fill="url(#pcb-led-dome)"/>
                            // Bright specular highlight
                            <circle cx={indicator.x - r * 0.32} cy={indicator.y - r * 0.36} r={r * 0.42} fill="url(#pcb-led-spec)"/>
                            // Small bright dot — the lit die inside the dome
                            <circle cx={indicator.x - r * 0.2} cy={indicator.y - r * 0.18} r={r * 0.18} fill="rgba(255,255,255,0.9)"/>
                            <text x={indicator.x} y={indicator.y + r + 4.5} text-anchor="middle" fill={text_fill} font-size="4.6" font-family="monospace">{label}</text>
                        </>
                    }
                }).collect::<Vec<_>>()}

                <text x="130" y="304" text-anchor="middle" fill={footer_text_fill} font-size="6.1" font-family="monospace">{footer_label}</text>
            </>
        }
        .into_any(),
    )
}
