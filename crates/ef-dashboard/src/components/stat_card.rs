use leptos::prelude::*;

#[component]
pub fn StatCard(
    label: &'static str,
    #[prop(into)] value: Signal<String>,
    #[prop(default = "")] class: &'static str,
) -> impl IntoView {
    view! {
        <div class="stat-card">
            <span class="stat-label">{label}</span>
            <span class={format!("stat-value {class}")}>{value}</span>
        </div>
    }
}
