use leptos::prelude::*;

use crate::api::{self, Device};
use crate::components::device_icon::DeviceOsIcon;
use crate::components::status_badge::StatusBadge;

/// Device browser — list, filter, and manage devices.
#[component]
pub fn DevicesPage() -> impl IntoView {
    let (os_filter, set_os_filter) = signal(String::new());
    let (status_filter, set_status_filter) = signal(String::new());
    let (refresh, set_refresh) = signal(0u32);

    // Auto-refresh every 5 seconds
    Effect::new(move || {
        let handle = gloo_timers::callback::Interval::new(5_000, move || {
            set_refresh.update(|n| *n = n.wrapping_add(1));
        });
        std::mem::forget(handle);
    });

    let devices = LocalResource::new(move || {
        let _ = refresh.get();
        async move { api::fetch_all_devices().await }
    });

    let agents = LocalResource::new(move || {
        let _ = refresh.get();
        async move { api::fetch_all_agents().await }
    });

    let filtered = move || {
        devices.get().as_deref().map(|data| {
            let os = os_filter.get();
            let status = status_filter.get();
            data.devices
                .iter()
                .filter(|d| os.is_empty() || d.os.eq_ignore_ascii_case(&os))
                .filter(|d| status.is_empty() || d.status.eq_ignore_ascii_case(&status))
                .cloned()
                .collect::<Vec<_>>()
        })
    };

    let do_refresh = move |_| set_refresh.update(|n| *n += 1);

    view! {
        <h1 style="margin-bottom: 1rem">"Devices"</h1>

        <div class="filters">
            <div class="form-group">
                <label>"OS"</label>
                <select on:change=move |ev| set_os_filter.set(event_target_value(&ev))>
                    <option value="">"All"</option>
                    <option value="android">"Android"</option>
                    <option value="ios">"iOS"</option>
                    <option value="iot">"IoT"</option>
                    <option value="virtual_iot">"Virtual IoT"</option>
                    <option value="veeros">"VeerOS"</option>
                    <option value="ssh">"SSH hosts (macOS / Linux / Windows)"</option>
                </select>
            </div>
            <div class="form-group">
                <label>"Status"</label>
                <select on:change=move |ev| set_status_filter.set(event_target_value(&ev))>
                    <option value="">"All"</option>
                    <option value="available">"Available"</option>
                    <option value="busy">"Busy"</option>
                    <option value="offline">"Offline"</option>
                    <option value="maintenance">"Maintenance"</option>
                </select>
            </div>
            <button class="btn btn-ghost btn-sm" style="align-self: flex-end" on:click=do_refresh>"↻ Refresh"</button>
            <a href="/virtual-iot/new" class="btn btn-primary btn-sm" style="align-self: flex-end">"+ New Virtual IoT"</a>
            <a href="/resources" class="btn btn-ghost btn-sm" style="align-self: flex-end">"Host installer"</a>
        </div>

        <div class="card">
            <Transition fallback=move || view! { <div class="loading">"Loading devices…"</div> }>
                {move || {
                    let agents_map = agents.get().as_deref().map(|a| {
                        let mut map = std::collections::HashMap::new();
                        for agent in &a.agents {
                            map.insert(agent.id.clone(), agent.clone());
                        }
                        map
                    });
                    filtered().map(|devs| {
                        if devs.is_empty() {
                            return view! { <p class="empty-state">"No devices match the filters"</p> }.into_any();
                        }
                        view! { <DeviceTable devices=devs agents=agents_map/> }.into_any()
                    })
                }}
            </Transition>
        </div>
    }
}

#[component]
fn DeviceTable(
    devices: Vec<Device>,
    agents: Option<std::collections::HashMap<String, api::Agent>>,
) -> impl IntoView {
    let rows = devices
        .into_iter()
        .map(|d| {
            let status = d.status.clone();
            let is_virtual_iot = d.os == "virtual_iot";
            let is_veeros = d.os == "veeros";
            let edit_link = format!("/virtual-iot/edit?edit={}", d.id);
            let device_id_str = d.id.clone();

            let device_caps: Vec<String> = d
                .capabilities
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|value| value.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            let agent_info = d
                .agent_id
                .as_ref()
                .and_then(|agent_id| agents.as_ref().and_then(|map| map.get(agent_id)))
                .cloned();
            let agent_caps = agent_info
                .as_ref()
                .map(|agent| agent.capabilities.clone())
                .unwrap_or_default();

            let mut all_caps = device_caps.clone();
            all_caps.extend(agent_caps.clone());
            all_caps.sort();
            all_caps.dedup();

            let shell_url = if is_veeros {
                let ip_from_caps = device_caps
                    .iter()
                    .find_map(|cap| cap.strip_prefix("veeros_ip:").map(str::to_string));
                let port_from_caps = device_caps
                    .iter()
                    .find_map(|cap| cap.strip_prefix("veeros_port:").map(str::to_string));
                let agent_from_caps = device_caps
                    .iter()
                    .find_map(|cap| cap.strip_prefix("veeros_agent_id:").map(str::to_string));
                let (fallback_ip, fallback_port) = d
                    .serial
                    .as_deref()
                    .and_then(|serial| serial.rsplit_once(':'))
                    .map(|(host, port)| (host.to_string(), port.to_string()))
                    .unwrap_or_else(|| ("127.0.0.1".to_string(), "22".to_string()));
                let host = ip_from_caps.unwrap_or(fallback_ip);
                let port = port_from_caps.unwrap_or(fallback_port);
                let mut href = format!("/veeros/shell?host={host}&port={port}");
                if let Some(agent_id) = agent_from_caps {
                    href.push_str(&format!("&agent_id={agent_id}"));
                }
                href
            } else {
                String::new()
            };

            let can_control = status == "available" || status == "busy";
            let has_ssh = all_caps.iter().any(|cap| cap == "ssh");
            let has_xcode = all_caps.iter().any(|cap| cap == "xcode");

            let name_cell = if is_virtual_iot {
                view! { <a href={edit_link.clone()} title="Edit Virtual IoT config">{d.name.clone()}</a> }.into_any()
            } else if is_veeros {
                view! { <a href={shell_url.clone()} target="_blank" title="Open VeerOS Shell">{format!("{} 🖥", d.name.clone())}</a> }.into_any()
            } else {
                view! { <>{d.name.clone()}</> }.into_any()
            };

            let agent_cell = if let Some(agent) = agent_info.clone() {
                let agent_name = agent.name.clone();
                let agent_status = agent.status.clone();
                view! {
                    <div title={agent_name.clone()}>
                        <small class="text-muted">{agent_name.clone()}</small>
                        <br/>
                        <span class="badge badge-sm badge-primary">{agent_status}</span>
                    </div>
                }
                .into_any()
            } else {
                view! { <span class="text-muted">"—"</span> }.into_any()
            };

            let capabilities_cell = if all_caps.is_empty() {
                view! { <span class="text-muted">"—"</span> }.into_any()
            } else {
                let badges = all_caps
                    .iter()
                    .map(|cap| {
                        let is_agent_cap = agent_caps.contains(cap) && !device_caps.contains(cap);
                        let cap_color = match cap.as_str() {
                            "xcode" => "badge-blue",
                            "macos_deploy" | "macos_runner" => "badge-blue",
                            "linux_runner" | "linux_deploy" => "badge-green",
                            "windows_runner" | "windows_deploy" | "msbuild" | "powershell" | "wsl" => "badge-orange",
                            "cuda" | "docker" => "badge-purple",
                            "mlx" | "llm_infer" | "llm_train" | "ane" => "badge-purple",
                            "gradle" => "badge-green",
                            "cargo" => "badge-orange",
                            "ssh" => "badge-gray",
                            "docker" | "containerization" => "badge-purple",
                            "android" | "ios" | "iot" => "badge-yellow",
                            _ => "badge-default",
                        };
                        let badge_style = if is_agent_cap {
                            "font-size: 0.75rem; padding: 2px 6px; opacity: 0.7; border: 1px solid currentColor;"
                        } else {
                            "font-size: 0.75rem; padding: 2px 6px;"
                        };
                        let badge_title = if is_agent_cap {
                            format!(
                                "Agent capability ({})",
                                agent_info
                                    .as_ref()
                                    .map(|agent| agent.name.as_str())
                                    .unwrap_or("agent")
                            )
                        } else {
                            "Device capability".to_string()
                        };
                        let label = if is_agent_cap {
                            format!("⧉ {}", cap)
                        } else {
                            cap.clone()
                        };

                        view! {
                            <span class=format!("badge {}", cap_color) style=badge_style title=badge_title>
                                {label}
                            </span>
                        }
                    })
                    .collect_view();

                view! {
                    <div class="capability-badges" style="display: flex; gap: 4px; flex-wrap: wrap;">
                        {badges}
                    </div>
                }
                .into_any()
            };

            let action_cell = if can_control && has_ssh {
                view! {
                    <a href={format!("/device/{}/terminal", device_id_str)} class="btn btn-xs btn-outline" title="Open terminal">
                        "⌨ Shell"
                    </a>
                }
                .into_any()
            } else if can_control && has_xcode {
                view! {
                    <a href={format!("/device/{}/build", device_id_str)} class="btn btn-xs btn-outline" title="Run build">
                        "🔨 Build"
                    </a>
                }
                .into_any()
            } else {
                view! { <span class="text-muted">"—"</span> }.into_any()
            };

            view! {
                <tr>
                    <td><strong>{name_cell}</strong></td>
                    <td class="mono truncate">{d.serial.unwrap_or_else(|| "—".to_string())}</td>
                    <td><DeviceOsIcon os=d.os.clone()/> " " {d.os.clone()}</td>
                    <td>{d.os_version.clone()}</td>
                    <td>{d.kind.clone()}</td>
                    <td>{d.region.clone()}</td>
                    <td><StatusBadge status=status.clone()/></td>
                    <td>{agent_cell}</td>
                    <td class="capabilities-cell">{capabilities_cell}</td>
                    <td class="actions-cell">{action_cell}</td>
                </tr>
            }
        })
        .collect_view();

    view! {
        <div class="table-wrap">
            <table>
                <thead>
                    <tr>
                        <th>"Name"</th>
                        <th>"Serial"</th>
                        <th>"OS"</th>
                        <th>"Version"</th>
                        <th>"Kind"</th>
                        <th>"Region"</th>
                        <th>"Status"</th>
                        <th>"Agent"</th>
                        <th>"Capabilities"</th>
                        <th>"Actions"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
    }
}
