use dioxus::prelude::*;

use super::actions::{Button, ButtonVariant, LucideIcon};

/// How many bottom sheets are up, app-wide.
///
/// Back closes a sheet before it moves in history, so Android has to be told
/// whether this app intends to handle Back at all: a callback left enabled
/// unconditionally costs the predictive back animations
/// (`android/MainActivity.kt`), and Kotlin cannot see a `<dialog>`. Every sheet
/// in the app goes through `BottomSheet`, so this is the one place that knows —
/// a rule rather than a list of the sheets that exist today. A count, not a
/// flag: a confirmation can open over the catalogue picker.
#[derive(Clone, Copy)]
pub struct OpenSheets(pub Signal<usize>);

/// Saturating on the way down. The count is bookkeeping, not truth: a wrong one
/// costs an animation, while an underflow would panic on her phone.
fn adjust_open_sheets(mut sheets: Signal<usize>, opened: bool) {
    let current = *sheets.peek();
    sheets.set(if opened {
        current + 1
    } else {
        current.saturating_sub(1)
    });
}

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
    // Before the early return below, not after: a component's hooks have to run
    // in the same order on every render, and `open` flips.
    let open_sheets = use_context::<OpenSheets>().0;
    let mut counted = use_signal(|| false);
    use_effect(use_reactive!(|open| {
        if open != *counted.peek() {
            counted.set(open);
            adjust_open_sheets(open_sheets, open);
        }
    }));
    // A sheet whose screen goes away while it is still up owes its unit back —
    // the issue flow replaces the form from under an open confirmation.
    use_drop(move || {
        if *counted.peek() {
            adjust_open_sheets(open_sheets, false);
        }
    });

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
            // While a job runs the sheet is not dismissible: hiding it would
            // suggest the job is cancelled — it is not (fire-and-forget), and
            // the worker closes the sheet itself when done.
            oncancel: move |event| {
                if loading {
                    event.prevent_default();
                } else {
                    on_dismiss.call(());
                }
            },
            onmounted: move |_| {
                let script = open_bottom_sheet_script(&mounted_id);
                let _ = document::eval(&script);
            },
            section {
                class: "bottom-sheet",
                // No drag handle: in M3 the handle *is* the drag affordance, and
                // these sheets are not draggable. Dismissal is the scrim below,
                // the Back gesture, or the sheet's own cancel action.
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
                disabled: loading,
                onclick: move |_| {
                    if !loading {
                        on_dismiss.call(());
                    }
                },
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
