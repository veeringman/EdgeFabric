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
                                    let pin_fill = pin_fill_for_column.clone();
                                    let pin_rx = header.pin_corner_radius.unwrap_or(0.8);
                                    view! {
                                        <rect
                                            x={pin_x}
                                            y={pin_y}
                                            width={header.pin_width}
                                            height={header.pin_height}
                                            rx={pin_rx}
                                            fill={pin_fill}
                                        />
                                    }
                                }).collect::<Vec<_>>()
                            }).collect::<Vec<_>>()}
                            <text x={label_x} y={label_y} text-anchor="middle" fill={text_fill} font-size="6" font-family="monospace">{label}</text>
                        </>
                    }
                }).collect::<Vec<_>>()}

                <rect
                    x={module.x}
                    y={module.y}
                    width={module.width}
                    height={module.height}
                    rx={module.radius}
                    fill={module.fill.clone()}
                    stroke={module.stroke.clone()}
                    stroke-width="1.3"
                    filter="url(#chip-shadow)"
                />
                <text x={module.x + module.width / 2.0} y={module.y + (module.height / 2.0) + 4.0} text-anchor="middle" fill={module.text_fill.clone().unwrap_or_else(|| String::from("#e7f5df"))} font-size={module.font_size.unwrap_or(7.5)} font-family="monospace">{chip_label_text.clone()}</text>

                {layout.ports.iter().map(|block| {
                    let label = block.label.clone();
                    let fill = block.fill.clone();
                    let stroke = block.stroke.clone();
                    let text_fill = block.text_fill.clone().unwrap_or_else(|| String::from("#374151"));
                    let font_size = block.font_size.unwrap_or(5.4);
                    view! {
                        <>
                            <rect x={block.x} y={block.y} width={block.width} height={block.height} rx={block.radius} fill={fill} stroke={stroke} stroke-width="1"/>
                            <text x={block.x + block.width / 2.0} y={block.y + (block.height / 2.0) + 4.0} text-anchor="middle" fill={text_fill} font-size={font_size} font-family="monospace">{label}</text>
                        </>
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
                    view! {
                        <>
                            <circle cx={indicator.x} cy={indicator.y} r={indicator.radius} fill={indicator.fill.clone()} stroke={indicator.stroke.clone()} stroke-width="1"/>
                            <text x={indicator.x} y={indicator.y + 2.2} text-anchor="middle" fill={text_fill} font-size="4.8" font-family="monospace">{label}</text>
                        </>
                    }
                }).collect::<Vec<_>>()}

                <text x="130" y="304" text-anchor="middle" fill={footer_text_fill} font-size="6.1" font-family="monospace">{footer_label}</text>
            </>
        }
        .into_any(),
    )
}
