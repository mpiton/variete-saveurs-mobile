use dioxus::prelude::*;

use super::actions::{Button, ButtonVariant};

const OPEN_BOTTOM_SHEET: &str = r#"
    const sheet = document.getElementById('bottom-sheet-dialog');
    if (sheet && !sheet.open) sheet.showModal();
"#;

#[component]
pub fn BottomSheet(
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

    rsx! {
        dialog {
            id: "bottom-sheet-dialog",
            class: "bottom-sheet-layer",
            aria_labelledby: "bottom-sheet-title",
            aria_busy: loading,
            aria_invalid: error,
            oncancel: move |_| on_dismiss.call(()),
            onmounted: move |_| {
                let _ = document::eval(OPEN_BOTTOM_SHEET);
            },
            section {
                class: "bottom-sheet",
                div { class: "bottom-sheet__handle", aria_hidden: "true" }
                h2 { id: "bottom-sheet-title", "{title}" }
                div { class: "bottom-sheet__content", aria_hidden: loading, {children} }
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
pub fn Snackbar(message: String) -> Element {
    rsx! {
        div { class: "snackbar", role: "status", aria_live: "polite",
            span { "{message}" }
        }
    }
}

#[component]
pub fn ErrorBlock(title: String, message: String) -> Element {
    rsx! {
        section { class: "error-block", role: "alert",
            strong { "{title}" }
            p { "{message}" }
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
        span { aria_hidden: "true",
            svg {
                class: "lucide empty-state__icon",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.5",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" }
                path { d: "M14 2v6h6" }
                path { d: "M9 15h6" }
            }
        }
    }
}
