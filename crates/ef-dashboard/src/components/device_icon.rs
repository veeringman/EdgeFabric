use leptos::prelude::*;

#[component]
pub fn DeviceOsIcon(
    os: String,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    let mut classes = "os-icon".to_string();
    if !class.is_empty() {
        classes.push(' ');
        classes.push_str(&class);
    }

    match os.to_lowercase().as_str() {
        "ios" => view! {
            <span class=classes aria-hidden="true">
                <svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" fill="currentColor">
                    <path d="M16.37 12.14c.02 2.51 2.2 3.35 2.22 3.36-.02.06-.35 1.2-1.15 2.37-.69 1.01-1.41 2.01-2.53 2.03-1.1.02-1.45-.65-2.7-.65-1.25 0-1.64.63-2.68.67-1.08.04-1.9-1.08-2.59-2.08-1.41-2.04-2.49-5.76-1.04-8.27.72-1.24 2.01-2.03 3.41-2.05 1.06-.02 2.06.71 2.7.71.64 0 1.84-.88 3.1-.75.53.02 2.02.22 2.97 1.61-.08.05-1.77 1.03-1.75 3.05zM14.68 4.23c.58-.7.97-1.67.86-2.64-.84.03-1.85.56-2.45 1.26-.54.62-1.01 1.61-.89 2.56.93.07 1.89-.47 2.48-1.18z"/>
                </svg>
            </span>
        }
        .into_any(),
        "android" => view! {
            <span class=classes aria-hidden="true">
                <svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M8 10h8a3 3 0 0 1 3 3v4a3 3 0 0 1-3 3H8a3 3 0 0 1-3-3v-4a3 3 0 0 1 3-3Z"/>
                    <path d="M9 10V8a3 3 0 0 1 6 0v2"/>
                    <path d="m8 4 1.2 1.2M16 4l-1.2 1.2"/>
                    <path d="M9.2 14h.01M14.8 14h.01"/>
                </svg>
            </span>
        }
        .into_any(),
        "iot" => view! {
            <span class=classes aria-hidden="true">
                <svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <rect x="7" y="7" width="10" height="10" rx="2"/>
                    <path d="M4 10h3M4 14h3M17 10h3M17 14h3M10 4v3M14 4v3M10 17v3M14 17v3"/>
                    <path d="M10 12h4"/>
                </svg>
            </span>
        }
        .into_any(),
        "virtual_iot" => view! {
            <span class=classes aria-hidden="true">
                <svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <rect x="6" y="3" width="12" height="18" rx="2"/>
                    <circle cx="12" cy="8" r="1.5"/>
                    <path d="M8 13h8M8 16h8"/>
                    <path d="M9 3V1M15 3V1"/>
                </svg>
            </span>
        }
        .into_any(),
        "ssh" | "server" => view! {
            <span class=classes aria-hidden="true">
                <svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <rect x="3" y="4" width="18" height="6" rx="1.8"/>
                    <rect x="3" y="14" width="18" height="6" rx="1.8"/>
                    <path d="M7 7h.01M10 7h.01M7 17h.01M10 17h.01M14 7h4M14 17h4"/>
                </svg>
            </span>
        }
        .into_any(),
        _ => view! {
            <span class=classes aria-hidden="true">
                <svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <rect x="7" y="3" width="10" height="18" rx="2"/>
                    <path d="M10 6h4M11 18h2"/>
                </svg>
            </span>
        }
        .into_any(),
    }
}