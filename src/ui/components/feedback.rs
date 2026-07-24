use dioxus::prelude::*;

use super::actions::{Button, ButtonVariant, LucideIcon};

#[component]
pub fn BottomSheet(
    id: String,
    title: String,
    open: bool,
    on_dismiss: EventHandler<()>,
    children: Element,
    #[props(default)] loading: bool,
    #[props(default)] error: bool,
) -> Element {
    if !open {
        return rsx! {};
    }

    let title_id = format!("{id}-title");
    let mounted_id = id.clone();

    rsx! {
        dialog {
            id,
            class: if error { "bottom-sheet-layer is-error" } else { "bottom-sheet-layer" },
            aria_labelledby: title_id.clone(),
            aria_busy: loading,
            oncancel: move |_| on_dismiss.call(()),
            onmounted: move |_| {
                let script = open_bottom_sheet_script(&mounted_id);
                let _ = document::eval(&script);
            },
            section {
                class: "bottom-sheet",
                div { class: "bottom-sheet__handle", aria_hidden: "true" }
                h2 { id: title_id, "{title}" }
                div {
                    class: "bottom-sheet__content",
                    aria_hidden: loading,
                    inert: loading.then_some("true"),
                    {children}
                }
                if loading {
                    span { class: "spinner", aria_hidden: "true" }
                }
            }
            button {
                class: "bottom-sheet__scrim",
                r#type: "button",
                aria_label: "Fermer",
                onclick: move |_| on_dismiss.call(()),
            }
        }
    }
}

#[component]
pub fn Snackbar(message: String, #[props(default = true)] announce: bool) -> Element {
    rsx! {
        div {
            class: "snackbar",
            role: announce.then_some("status"),
            aria_live: announce.then_some("polite"),
            span { "{message}" }
        }
    }
}

#[component]
pub fn ErrorBlock(
    title: String,
    #[props(default)] message: String,
    /// Aggregated list variant (e.g. validation errors) — rendered under the
    /// optional single message.
    #[props(default)]
    items: Vec<String>,
) -> Element {
    rsx! {
        section { class: "error-block", role: "alert",
            strong { "{title}" }
            if !message.is_empty() {
                p { "{message}" }
            }
            if !items.is_empty() {
                ul { class: "error-block__list",
                    // Index keys are acceptable here: items are stateless text,
                    // and two entries can legitimately share the same message.
                    for (index, item) in items.into_iter().enumerate() {
                        li { key: "{index}", "{item}" }
                    }
                }
            }
        }
    }
}

#[component]
pub fn EmptyState(
    message: String,
    action_label: String,
    onclick: EventHandler<MouseEvent>,
    #[props(default)] disabled: bool,
    #[props(default)] loading: bool,
    #[props(default)] error: bool,
) -> Element {
    rsx! {
        section { class: "empty-state",
            FileIcon {}
            p { "{message}" }
            Button {
                label: action_label,
                variant: ButtonVariant::Tonal,
                disabled,
                loading,
                error,
                onclick: move |event| onclick.call(event),
            }
        }
    }
}

#[component]
fn FileIcon() -> Element {
    rsx! {
        LucideIcon { class: "lucide empty-state__icon", stroke_width: "1.5",
            path { d: "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" }
            path { d: "M14 2v6h6" }
            path { d: "M9 15h6" }
        }
    }
}

fn open_bottom_sheet_script(id: &str) -> String {
    let id = serde_json::to_string(id).expect("serializing a string cannot fail");
    format!(
        "const sheet = document.getElementById({id}); if (sheet && !sheet.open) sheet.showModal();"
    )
}
