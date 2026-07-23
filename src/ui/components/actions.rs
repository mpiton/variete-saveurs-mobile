use dioxus::prelude::*;

use crate::domain::models::DocumentKind;

/// Label of the issue action — one action (the task-20 issuance flow), one
/// label shared by every screen that offers it.
pub fn issue_label(kind: &DocumentKind) -> &'static str {
    match kind {
        DocumentKind::Quote => "Émettre le devis",
        DocumentKind::Invoice => "Émettre la facture",
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ButtonVariant {
    #[default]
    Filled,
    Tonal,
    Outlined,
    Text,
}

impl ButtonVariant {
    const fn class(self) -> &'static str {
        match self {
            Self::Filled => "m3-button--filled",
            Self::Tonal => "m3-button--tonal",
            Self::Outlined => "m3-button--outlined",
            Self::Text => "m3-button--text",
        }
    }
}

#[component]
pub fn Button(
    label: String,
    onclick: EventHandler<MouseEvent>,
    #[props(default)] variant: ButtonVariant,
    #[props(default)] disabled: bool,
    #[props(default)] loading: bool,
    #[props(default)] error: bool,
    #[props(default = true)] announce_error: bool,
) -> Element {
    let class = format!(
        "m3-button {}{}",
        variant.class(),
        if error { " is-error" } else { "" }
    );
    let accessible_label = if error {
        format!("{label}, erreur")
    } else {
        label.clone()
    };

    rsx! {
        button {
            class,
            r#type: "button",
            aria_label: accessible_label,
            disabled: disabled || loading,
            aria_busy: loading,
            onclick: move |event| onclick.call(event),
            span { class: "m3-button__label", aria_hidden: loading, "{label}" }
            if loading {
                Spinner {}
            }
        }
        if error && announce_error {
            ActionErrorStatus { message: "L’action a échoué." }
        }
    }
}

#[component]
pub fn Fab(
    label: String,
    onclick: EventHandler<MouseEvent>,
    expanded: Option<bool>,
    controls: Option<String>,
    #[props(default)] disabled: bool,
    #[props(default)] loading: bool,
    #[props(default)] error: bool,
    #[props(default = true)] announce_error: bool,
) -> Element {
    let class = if error { "fab is-error" } else { "fab" };
    let accessible_label = if error {
        format!("{label}, erreur")
    } else {
        label
    };

    rsx! {
        button {
            class,
            r#type: "button",
            aria_label: accessible_label,
            aria_controls: expanded.filter(|open| *open).and(controls),
            aria_expanded: expanded,
            aria_busy: loading,
            disabled: disabled || loading,
            onclick: move |event| onclick.call(event),
            if loading {
                Spinner {}
            } else {
                PlusIcon {}
            }
        }
        if error && announce_error {
            ActionErrorStatus { message: "L’action a échoué." }
        }
    }
}

#[component]
pub fn FabMenu(
    id: String,
    open: bool,
    on_toggle: EventHandler<MouseEvent>,
    on_quote: EventHandler<MouseEvent>,
    on_invoice: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "fab-menu",
            div { id: id.clone(), class: "fab-menu__items", role: "group", aria_label: "Type de document", hidden: !open,
                button {
                    class: "fab-menu__item",
                    r#type: "button",
                    onclick: move |event| on_quote.call(event),
                    QuoteIcon {}
                    span { "Devis" }
                }
                button {
                    class: "fab-menu__item",
                    r#type: "button",
                    onclick: move |event| on_invoice.call(event),
                    InvoiceIcon {}
                    span { "Facture" }
                }
            }
            Fab {
                label: if open { "Fermer le menu de création" } else { "Créer un document" },
                expanded: Some(open),
                controls: Some(id.clone()),
                onclick: move |event| on_toggle.call(event),
            }
        }
    }
}

#[component]
pub fn SegmentedButton(
    label: String,
    options: Vec<String>,
    selected: usize,
    on_select: EventHandler<usize>,
    #[props(default)] disabled: bool,
    #[props(default)] loading: bool,
    #[props(default)] error: bool,
    #[props(default = true)] announce_error: bool,
) -> Element {
    let class = if error {
        "segmented-button is-error"
    } else {
        "segmented-button"
    };
    let accessible_label = if error {
        format!("{label}, erreur")
    } else {
        label
    };

    rsx! {
        div {
            class,
            role: "group",
            aria_label: accessible_label,
            aria_busy: loading,
            for (index, option) in options.into_iter().enumerate() {
                button {
                    class: "segmented-button__option",
                    r#type: "button",
                    aria_label: option.clone(),
                    aria_pressed: index == selected,
                    disabled: disabled || loading,
                    onclick: move |_| on_select.call(index),
                    span { class: "segmented-button__label", aria_hidden: loading, "{option}" }
                }
            }
            if loading {
                Spinner {}
            }
        }
        if error && announce_error {
            ActionErrorStatus { message: "L’action a échoué." }
        }
    }
}

#[component]
fn Spinner() -> Element {
    rsx! { span { class: "spinner", aria_hidden: "true" } }
}

#[component]
pub(super) fn ActionErrorStatus(message: String) -> Element {
    rsx! {
        span { class: "visually-hidden", role: "status", aria_live: "polite",
            "{message}"
        }
    }
}

#[component]
pub(super) fn LucideIcon(
    children: Element,
    #[props(default = "lucide".to_string())] class: String,
    #[props(default = "2".to_string())] stroke_width: String,
) -> Element {
    rsx! {
        span { aria_hidden: "true",
            svg {
                class,
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width,
                stroke_linecap: "round",
                stroke_linejoin: "round",
                {children}
            }
        }
    }
}

#[component]
fn PlusIcon() -> Element {
    rsx! {
        LucideIcon {
                path { d: "M5 12h14" }
                path { d: "M12 5v14" }
        }
    }
}

#[component]
fn QuoteIcon() -> Element {
    rsx! {
        LucideIcon {
            path { d: "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" }
            path { d: "M14 2v6h6" }
            path { d: "M8 13h2" }
            path { d: "M8 17h2" }
        }
    }
}

#[component]
fn InvoiceIcon() -> Element {
    rsx! {
        LucideIcon {
            path { d: "M4 2v20l2-2 2 2 2-2 2 2 2-2 2 2 2-2 2 2V2l-2 2-2-2-2 2-2-2-2 2-2-2-2 2-2-2Z" }
            path { d: "M16 8h-6" }
            path { d: "M16 12h-6" }
            path { d: "M13 16h-3" }
        }
    }
}
