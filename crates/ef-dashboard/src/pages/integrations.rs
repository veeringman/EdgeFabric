use leptos::prelude::*;

use crate::api::{
    self, CloudSink, CreateCloudSinkRequest, CreateMqttPublisherRequest,
    CreateMqttSubscriptionRequest,
};

#[component]
pub fn IntegrationsPage() -> impl IntoView {
    let (refresh, set_refresh) = signal(0u32);
    let (error, set_error) = signal(Option::<String>::None);

    let subscriptions = LocalResource::new(move || {
        let _ = refresh.get();
        async move { api::list_mqtt_subscriptions().await.unwrap_or_default() }
    });

    let publishers = LocalResource::new(move || {
        let _ = refresh.get();
        async move { api::list_mqtt_publishers().await.unwrap_or_default() }
    });

    let sinks = LocalResource::new(move || {
        let _ = refresh.get();
        async move { api::list_cloud_sinks().await.unwrap_or_default() }
    });

    let attempts = LocalResource::new(move || {
        let _ = refresh.get();
        async move {
            api::list_delivery_attempts(Some("dead_letter"), 100)
                .await
                .unwrap_or_default()
        }
    });

    let (sub_name, set_sub_name) = signal(String::new());
    let (sub_host, set_sub_host) = signal(String::new());
    let (sub_port, set_sub_port) = signal("1883".to_string());
    let (sub_topic, set_sub_topic) = signal("external/sensors/#".to_string());
    let (sub_client_id, set_sub_client_id) = signal(String::new());
    let (sub_username, set_sub_username) = signal(String::new());
    let (sub_password, set_sub_password) = signal(String::new());
    let (sub_tls, set_sub_tls) = signal(false);

    let create_subscription = move |_| {
        set_error.set(None);
        let port = sub_port.get_untracked().parse::<u16>().unwrap_or(1883);
        let req = CreateMqttSubscriptionRequest {
            name: sub_name.get_untracked(),
            broker_host: sub_host.get_untracked(),
            broker_port: port,
            topic_filter: sub_topic.get_untracked(),
            client_id: opt_str(sub_client_id.get_untracked()),
            username: opt_str(sub_username.get_untracked()),
            password: opt_str(sub_password.get_untracked()),
            use_tls: sub_tls.get_untracked(),
            reconnect_secs: 5,
        };
        leptos::task::spawn_local(async move {
            match api::create_mqtt_subscription(&req).await {
                Ok(_) => set_refresh.update(|n| *n = n.wrapping_add(1)),
                Err(e) => set_error.set(Some(e)),
            }
        });
    };

    let (pub_name, set_pub_name) = signal(String::new());
    let (pub_host, set_pub_host) = signal(String::new());
    let (pub_port, set_pub_port) = signal("1883".to_string());
    let (pub_topic_root, set_pub_topic_root) = signal("edgefabric/sensors".to_string());
    let (pub_client_id, set_pub_client_id) = signal(String::new());
    let (pub_username, set_pub_username) = signal(String::new());
    let (pub_password, set_pub_password) = signal(String::new());
    let (pub_tls, set_pub_tls) = signal(false);

    let create_publisher = move |_| {
        set_error.set(None);
        let port = pub_port.get_untracked().parse::<u16>().unwrap_or(1883);
        let req = CreateMqttPublisherRequest {
            name: pub_name.get_untracked(),
            broker_host: pub_host.get_untracked(),
            broker_port: port,
            topic_root: opt_str(pub_topic_root.get_untracked()),
            client_id: opt_str(pub_client_id.get_untracked()),
            username: opt_str(pub_username.get_untracked()),
            password: opt_str(pub_password.get_untracked()),
            use_tls: pub_tls.get_untracked(),
        };
        leptos::task::spawn_local(async move {
            match api::create_mqtt_publisher(&req).await {
                Ok(_) => set_refresh.update(|n| *n = n.wrapping_add(1)),
                Err(e) => set_error.set(Some(e)),
            }
        });
    };

    let (sink_kind, set_sink_kind) = signal("aws_s3_presigned_put".to_string());
    let (sink_name, set_sink_name) = signal(String::new());
    let (sink_s3_url, set_sink_s3_url) = signal(String::new());
    let (sink_aws_iot_url, set_sink_aws_iot_url) = signal(String::new());
    let (sink_aws_auth, set_sink_aws_auth) = signal(String::new());
    let (sink_az_host, set_sink_az_host) = signal(String::new());
    let (sink_az_device, set_sink_az_device) = signal(String::new());
    let (sink_az_sas, set_sink_az_sas) = signal(String::new());
    let (sink_az_api_ver, set_sink_az_api_ver) = signal("2021-04-12".to_string());

    let create_sink = move |_| {
        set_error.set(None);
        let req = match sink_kind.get_untracked().as_str() {
            "aws_iot_core_https" => CreateCloudSinkRequest::AwsIotCoreHttps {
                name: sink_name.get_untracked(),
                publish_url: sink_aws_iot_url.get_untracked(),
                authorization_header: opt_str(sink_aws_auth.get_untracked()),
            },
            "azure_iot_hub" => CreateCloudSinkRequest::AzureIotHub {
                name: sink_name.get_untracked(),
                host_name: sink_az_host.get_untracked(),
                device_id: sink_az_device.get_untracked(),
                sas_token: sink_az_sas.get_untracked(),
                api_version: sink_az_api_ver.get_untracked(),
            },
            _ => CreateCloudSinkRequest::AwsS3PresignedPut {
                name: sink_name.get_untracked(),
                presigned_put_url: sink_s3_url.get_untracked(),
            },
        };
        leptos::task::spawn_local(async move {
            match api::create_cloud_sink(&req).await {
                Ok(_) => set_refresh.update(|n| *n = n.wrapping_add(1)),
                Err(e) => set_error.set(Some(e)),
            }
        });
    };

    view! {
        <div class="page">
            <div class="page-header">
                <h1>"External Integrations"</h1>
            </div>

            <Show when=move || error.get().is_some()>
                <div class="alert error" style="margin-bottom:1rem;">{move || error.get().unwrap_or_default()}</div>
            </Show>

            <div class="card" style="margin-bottom:1rem;">
                <div class="card-header"><h2>"Create MQTT Subscription"</h2></div>
                <div class="card-body" style="display:grid; grid-template-columns:repeat(auto-fit,minmax(220px,1fr)); gap:0.75rem;">
                    <div class="form-group"><label>"Name"</label><input class="form-control" prop:value=move || sub_name.get() on:input=move |ev| set_sub_name.set(event_target_value(&ev))/></div>
                    <div class="form-group"><label>"Broker Host"</label><input class="form-control" prop:value=move || sub_host.get() on:input=move |ev| set_sub_host.set(event_target_value(&ev))/></div>
                    <div class="form-group"><label>"Broker Port"</label><input class="form-control" prop:value=move || sub_port.get() on:input=move |ev| set_sub_port.set(event_target_value(&ev))/></div>
                    <div class="form-group"><label>"Topic Filter"</label><input class="form-control" prop:value=move || sub_topic.get() on:input=move |ev| set_sub_topic.set(event_target_value(&ev))/></div>
                    <div class="form-group"><label>"Client ID"</label><input class="form-control" prop:value=move || sub_client_id.get() on:input=move |ev| set_sub_client_id.set(event_target_value(&ev))/></div>
                    <div class="form-group"><label>"Username"</label><input class="form-control" prop:value=move || sub_username.get() on:input=move |ev| set_sub_username.set(event_target_value(&ev))/></div>
                    <div class="form-group"><label>"Password"</label><input type="password" class="form-control" prop:value=move || sub_password.get() on:input=move |ev| set_sub_password.set(event_target_value(&ev))/></div>
                    <div class="form-group"><label>"TLS"</label><label style="display:flex;align-items:center;gap:0.5rem;"><input type="checkbox" prop:checked=move || sub_tls.get() on:change=move |_| set_sub_tls.update(|v| *v = !*v)/><span>"Enable TLS"</span></label></div>
                </div>
                <div style="padding:0.75rem 1rem;"><button class="btn btn-primary btn-sm" on:click=create_subscription>"Create Subscription"</button></div>
            </div>

            <div class="card" style="margin-bottom:1rem;">
                <div class="card-header"><h2>"Create MQTT Publisher"</h2></div>
                <div class="card-body" style="display:grid; grid-template-columns:repeat(auto-fit,minmax(220px,1fr)); gap:0.75rem;">
                    <div class="form-group"><label>"Name"</label><input class="form-control" prop:value=move || pub_name.get() on:input=move |ev| set_pub_name.set(event_target_value(&ev))/></div>
                    <div class="form-group"><label>"Broker Host"</label><input class="form-control" prop:value=move || pub_host.get() on:input=move |ev| set_pub_host.set(event_target_value(&ev))/></div>
                    <div class="form-group"><label>"Broker Port"</label><input class="form-control" prop:value=move || pub_port.get() on:input=move |ev| set_pub_port.set(event_target_value(&ev))/></div>
                    <div class="form-group"><label>"Topic Root"</label><input class="form-control" prop:value=move || pub_topic_root.get() on:input=move |ev| set_pub_topic_root.set(event_target_value(&ev))/></div>
                    <div class="form-group"><label>"Client ID"</label><input class="form-control" prop:value=move || pub_client_id.get() on:input=move |ev| set_pub_client_id.set(event_target_value(&ev))/></div>
                    <div class="form-group"><label>"Username"</label><input class="form-control" prop:value=move || pub_username.get() on:input=move |ev| set_pub_username.set(event_target_value(&ev))/></div>
                    <div class="form-group"><label>"Password"</label><input type="password" class="form-control" prop:value=move || pub_password.get() on:input=move |ev| set_pub_password.set(event_target_value(&ev))/></div>
                    <div class="form-group"><label>"TLS"</label><label style="display:flex;align-items:center;gap:0.5rem;"><input type="checkbox" prop:checked=move || pub_tls.get() on:change=move |_| set_pub_tls.update(|v| *v = !*v)/><span>"Enable TLS"</span></label></div>
                </div>
                <div style="padding:0.75rem 1rem;"><button class="btn btn-primary btn-sm" on:click=create_publisher>"Create Publisher"</button></div>
            </div>

            <div class="card" style="margin-bottom:1rem;">
                <div class="card-header"><h2>"Create Cloud Sink"</h2></div>
                <div class="card-body" style="display:grid; grid-template-columns:repeat(auto-fit,minmax(220px,1fr)); gap:0.75rem;">
                    <div class="form-group"><label>"Kind"</label>
                        <select class="form-control" prop:value=move || sink_kind.get() on:change=move |ev| set_sink_kind.set(event_target_value(&ev))>
                            <option value="aws_s3_presigned_put">"AWS S3 Presigned PUT"</option>
                            <option value="aws_iot_core_https">"AWS IoT Core HTTPS"</option>
                            <option value="azure_iot_hub">"Azure IoT Hub"</option>
                        </select>
                    </div>
                    <div class="form-group"><label>"Name"</label><input class="form-control" prop:value=move || sink_name.get() on:input=move |ev| set_sink_name.set(event_target_value(&ev))/></div>

                    <Show when=move || sink_kind.get() == "aws_s3_presigned_put">
                        <div class="form-group"><label>"S3 Presigned PUT URL"</label><input class="form-control" prop:value=move || sink_s3_url.get() on:input=move |ev| set_sink_s3_url.set(event_target_value(&ev))/></div>
                    </Show>
                    <Show when=move || sink_kind.get() == "aws_iot_core_https">
                        <div class="form-group"><label>"AWS IoT Publish URL"</label><input class="form-control" prop:value=move || sink_aws_iot_url.get() on:input=move |ev| set_sink_aws_iot_url.set(event_target_value(&ev))/></div>
                        <div class="form-group"><label>"Authorization Header"</label><input class="form-control" prop:value=move || sink_aws_auth.get() on:input=move |ev| set_sink_aws_auth.set(event_target_value(&ev))/></div>
                    </Show>
                    <Show when=move || sink_kind.get() == "azure_iot_hub">
                        <div class="form-group"><label>"Azure IoT Host"</label><input class="form-control" prop:value=move || sink_az_host.get() on:input=move |ev| set_sink_az_host.set(event_target_value(&ev))/></div>
                        <div class="form-group"><label>"Device ID"</label><input class="form-control" prop:value=move || sink_az_device.get() on:input=move |ev| set_sink_az_device.set(event_target_value(&ev))/></div>
                        <div class="form-group"><label>"SAS Token"</label><input class="form-control" prop:value=move || sink_az_sas.get() on:input=move |ev| set_sink_az_sas.set(event_target_value(&ev))/></div>
                        <div class="form-group"><label>"API Version"</label><input class="form-control" prop:value=move || sink_az_api_ver.get() on:input=move |ev| set_sink_az_api_ver.set(event_target_value(&ev))/></div>
                    </Show>
                </div>
                <div style="padding:0.75rem 1rem;"><button class="btn btn-primary btn-sm" on:click=create_sink>"Create Cloud Sink"</button></div>
            </div>

            <div class="card" style="margin-bottom:1rem;">
                <div class="card-header"><h2>"MQTT Subscriptions"</h2></div>
                <Transition fallback=move || view! { <div class="loading">"Loading..."</div> }>
                    {move || subscriptions.get().map(|data| view! {
                        <div class="table-wrap">
                            <table>
                                <thead><tr><th>"Name"</th><th>"Broker"</th><th>"Topic"</th><th>"TLS"</th><th>"Actions"</th></tr></thead>
                                <tbody>
                                    {data.subscriptions.iter().cloned().map(|s| {
                                        let id = s.id.clone();
                                        view! {
                                            <tr>
                                                <td>{s.name}</td>
                                                <td>{format!("{}:{}", s.broker_host, s.broker_port)}</td>
                                                <td class="mono">{s.topic_filter}</td>
                                                <td>{if s.use_tls { "yes" } else { "no" }}</td>
                                                <td><button class="btn btn-danger btn-sm" on:click=move |_| {
                                                    let id = id.clone();
                                                    leptos::task::spawn_local(async move {
                                                        let _ = api::delete_mqtt_subscription(&id).await;
                                                        set_refresh.update(|n| *n = n.wrapping_add(1));
                                                    });
                                                } >"Delete"</button></td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        </div>
                    })}
                </Transition>
            </div>

            <div class="card" style="margin-bottom:1rem;">
                <div class="card-header"><h2>"MQTT Publishers"</h2></div>
                <Transition fallback=move || view! { <div class="loading">"Loading..."</div> }>
                    {move || publishers.get().map(|data| view! {
                        <div class="table-wrap">
                            <table>
                                <thead><tr><th>"Name"</th><th>"Broker"</th><th>"Topic Root"</th><th>"TLS"</th><th>"Actions"</th></tr></thead>
                                <tbody>
                                    {data.publishers.iter().cloned().map(|p| {
                                        let id = p.id.clone();
                                        view! {
                                            <tr>
                                                <td>{p.name}</td>
                                                <td>{format!("{}:{}", p.broker_host, p.broker_port)}</td>
                                                <td class="mono">{p.topic_root}</td>
                                                <td>{if p.use_tls { "yes" } else { "no" }}</td>
                                                <td><button class="btn btn-danger btn-sm" on:click=move |_| {
                                                    let id = id.clone();
                                                    leptos::task::spawn_local(async move {
                                                        let _ = api::delete_mqtt_publisher(&id).await;
                                                        set_refresh.update(|n| *n = n.wrapping_add(1));
                                                    });
                                                } >"Delete"</button></td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        </div>
                    })}
                </Transition>
            </div>

            <div class="card" style="margin-bottom:1rem;">
                <div class="card-header"><h2>"Cloud Sinks"</h2></div>
                <Transition fallback=move || view! { <div class="loading">"Loading..."</div> }>
                    {move || sinks.get().map(|data| view! {
                        <div class="table-wrap">
                            <table>
                                <thead><tr><th>"Name"</th><th>"Kind"</th><th>"Target"</th><th>"Actions"</th></tr></thead>
                                <tbody>
                                    {data.sinks.iter().cloned().map(|sink| {
                                        let (id, name, kind, target) = match sink.clone() {
                                            CloudSink::AwsS3PresignedPut { id, name, presigned_put_url, .. } => (id, name, "aws_s3_presigned_put".to_string(), truncate(&presigned_put_url)),
                                            CloudSink::AwsIotCoreHttps { id, name, publish_url, .. } => (id, name, "aws_iot_core_https".to_string(), truncate(&publish_url)),
                                            CloudSink::AzureIotHub { id, name, host_name, device_id, .. } => (id, name, "azure_iot_hub".to_string(), format!("{host_name}/{device_id}")),
                                        };
                                        view! {
                                            <tr>
                                                <td>{name}</td>
                                                <td class="mono">{kind}</td>
                                                <td class="mono">{target}</td>
                                                <td><button class="btn btn-danger btn-sm" on:click=move |_| {
                                                    let id = id.clone();
                                                    leptos::task::spawn_local(async move {
                                                        let _ = api::delete_cloud_sink(&id).await;
                                                        set_refresh.update(|n| *n = n.wrapping_add(1));
                                                    });
                                                } >"Delete"</button></td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        </div>
                    })}
                </Transition>
            </div>

            <div class="card">
                <div class="card-header"><h2>"Dead Letter Attempts"</h2></div>
                <Transition fallback=move || view! { <div class="loading">"Loading..."</div> }>
                    {move || attempts.get().map(|data| {
                        if data.attempts.is_empty() {
                            return view! { <p class="empty-state">"No dead-letter attempts."</p> }.into_any();
                        }
                        view! {
                            <div class="table-wrap">
                                <table>
                                    <thead><tr><th>"When"</th><th>"Endpoint"</th><th>"Device"</th><th>"Sensor"</th><th>"Attempt"</th><th>"Error"</th></tr></thead>
                                    <tbody>
                                        {data.attempts.iter().cloned().map(|a| view! {
                                            <tr>
                                                <td>{a.created_at}</td>
                                                <td class="mono">{a.endpoint_id}</td>
                                                <td class="mono">{a.device_id.unwrap_or_default()}</td>
                                                <td>{a.sensor.unwrap_or_default()}</td>
                                                <td>{a.attempt}</td>
                                                <td>{a.error_message.unwrap_or_default()}</td>
                                            </tr>
                                        }).collect::<Vec<_>>()}
                                    </tbody>
                                </table>
                            </div>
                        }
                        .into_any()
                    })}
                </Transition>
            </div>
        </div>
    }
}

fn opt_str(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn truncate(value: &str) -> String {
    if value.len() <= 72 {
        value.to_string()
    } else {
        format!("{}...", &value[..72])
    }
}
