use leptos::prelude::*;

#[component]
pub fn StatusBadge(#[prop(into)] status: String) -> impl IntoView {
    let class = match status.to_lowercase().as_str() {
        "available" | "idle" => "badge available",
        "busy" | "in_use" | "allocated" => "badge busy",
        "offline" | "maintenance" => "badge offline",
        "active" => "badge active",
        "error" | "failed" => "badge failed",
        "passed" | "completed" => "badge passed",
        "running" | "executing" => "badge running",
        "queued" | "pending" | "requested" => "badge queued",
        _ => "badge",
    };
    view! {
        <span class=class>{status}</span>
    }
}
