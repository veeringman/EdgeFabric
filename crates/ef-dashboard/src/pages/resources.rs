use leptos::prelude::*;

use crate::api::{self, ResourceItem, ResourceListResponse};

#[component]
pub fn ResourcesPage() -> impl IntoView {
    let catalog = LocalResource::new(|| async move { api::list_resources().await });

    view! {
        <h1 style="margin-bottom: 0.35rem">"Downloads & Resources"</h1>
        <p class="muted" style="margin-bottom: 1.25rem">
            "Installers that turn a Mac mini, MacBook, Linux box, or Windows server into a generic EdgeFabric host. Cloud never needs a LAN IP — OpenSSH plus VeerSetu overlay is enough."
        </p>

        <div class="card" style="margin-bottom: 1rem">
            <div class="card-header">
                <h2>"Join this machine"</h2>
            </div>
            <ol class="resources-steps">
                <li>"Enable OpenSSH (Remote Login on macOS, OpenSSH Server on Linux/Windows)."</li>
                <li>"Download the host package for this OS, unpack it, and run " <code class="mono">"./install.sh"</code> " with " <code class="mono">"EF_API_KEY"</code> "."</li>
                <li>"Leave " <code class="mono">"veersetu-agent"</code> " running so EdgeFabric can SSH to local port 22."</li>
                <li>"Authorize the control plane SSH key, then confirm the host under Devices & Hosts."</li>
            </ol>
            <pre class="resources-snippet">{
                "export EF_API_KEY=ef-...\n\
export EF_WITH_ML=1   # optional LLM worker\n\
export VEERSETU_CONTROL_TOKEN=...\n\
./install.sh"
            }</pre>
            <p class="muted" style="margin-top: 0.75rem">
                "Manual: "
                <code class="mono">"ef --server https://edgefabric.veerlabs.solutions host join --api-key $EF_API_KEY [--with-ml]"</code>
            </p>
        </div>

        <Transition fallback=move || view! { <div class="loading">"Loading catalog…"</div> }>
            {move || catalog.get().map(|res| match (*res).clone() {
                Err(e) => view! {
                    <div class="alert error">{format!("Could not load catalog: {e}")}</div>
                }.into_any(),
                Ok(data) => view! { <ResourceGrid data=data/> }.into_any(),
            })}
        </Transition>
    }
}

#[component]
fn ResourceGrid(data: ResourceListResponse) -> impl IntoView {
    let items = data.resources;
    if items.is_empty() {
        return view! { <p class="empty-state">"No packages listed yet."</p> }.into_any();
    }
    view! {
        <div class="resources-grid">
            {items.into_iter().map(|item| view! { <ResourceCard item=item/> }).collect_view()}
        </div>
    }
    .into_any()
}

#[component]
fn ResourceCard(item: ResourceItem) -> impl IntoView {
    let available = item.available;
    let href = item.download_path.clone();
    let filename = item.filename.clone();
    let size = match (item.available, item.size_bytes) {
        (true, Some(n)) => format_bytes(n),
        (true, None) => "Ready".into(),
        (false, _) => "Not published".into(),
    };
    let platform = format!("{} · {}", item.platform, item.arch);
    view! {
        <div class="card resource-card">
            <div class="resource-card-kicker">{item.kind}</div>
            <h3>{item.name}</h3>
            <p class="muted">{item.summary}</p>
            <p class="mono" style="margin: 0.5rem 0">{filename.clone()}</p>
            <p class="muted">{platform}" · "{size}</p>
            <pre class="resources-snippet">{item.install_hint}</pre>
            {if available {
                view! {
                    <a class="btn btn-primary btn-sm" href=href>"Download"</a>
                }.into_any()
            } else {
                view! {
                    <button class="btn btn-ghost btn-sm" disabled>"Not published yet"</button>
                }.into_any()
            }}
        </div>
    }
}

fn format_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let v = n as f64;
    if v >= MB {
        format!("{:.1} MB", v / MB)
    } else if v >= KB {
        format!("{:.1} KB", v / KB)
    } else {
        format!("{n} B")
    }
}
