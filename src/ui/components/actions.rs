use dioxus::prelude::*;

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
) -> Element {
    let class = format!("m3-button {}", variant.class());
    let accessible_label = label.clone();

    rsx! {
        button {
            class,
            r#type: "button",
            aria_label: accessible_label,
            disabled: disabled || loading,
            aria_busy: loading,
            aria_invalid: error,
            onclick: move |event| onclick.call(event),
            span { class: "m3-button__label", aria_hidden: loading, "{label}" }
            if loading {
                Spinner {}
            }
        }
    }
}

#[component]
pub fn Fab(
    label: String,
    onclick: EventHandler<MouseEvent>,
    expanded: Option<bool>,
    #[props(default)] disabled: bool,
    #[props(default)] loading: bool,
    #[props(default)] error: bool,
) -> Element {
    rsx! {
        button {
            class: "fab",
            r#type: "button",
            aria_label: label,
            aria_controls: expanded.map(|_| "fab-menu"),
            aria_expanded: expanded,
            aria_busy: loading,
            aria_invalid: error,
            disabled: disabled || loading,
            onclick: move |event| onclick.call(event),
            if loading {
                Spinner {}
            } else {
                PlusIcon {}
            }
        }
    }
}

#[component]
pub fn FabMenu(
    open: bool,
    on_toggle: EventHandler<MouseEvent>,
    on_quote: EventHandler<MouseEvent>,
    on_invoice: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "fab-menu",
            if open {
                div { id: "fab-menu", class: "fab-menu__items", role: "menu", aria_label: "Type de document",
                    button {
                        class: "fab-menu__item",
                        r#type: "button",
                        role: "menuitem",
                        onclick: move |event| on_quote.call(event),
                        QuoteIcon {}
                        span { "Devis" }
                    }
                    button {
                        class: "fab-menu__item",
                        r#type: "button",
                        role: "menuitem",
                        onclick: move |event| on_invoice.call(event),
                        InvoiceIcon {}
                        span { "Facture" }
                    }
                }
            }
            Fab {
                label: if open { "Fermer le menu de création" } else { "Créer un document" },
                expanded: open,
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
) -> Element {
    rsx! {
        div {
            class: "segmented-button",
            role: "group",
            aria_label: label,
            aria_busy: loading,
            aria_invalid: error,
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
    }
}

#[component]
fn Spinner() -> Element {
    rsx! { span { class: "spinner", aria_hidden: "true" } }
}

#[component]
fn PlusIcon() -> Element {
    rsx! {
        span { aria_hidden: "true",
            svg {
                class: "lucide",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M5 12h14" }
                path { d: "M12 5v14" }
            }
        }
    }
}

#[component]
fn QuoteIcon() -> Element {
    rsx! {
        span { aria_hidden: "true",
            svg {
                class: "lucide",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" }
                path { d: "M14 2v6h6" }
                path { d: "M8 13h2" }
                path { d: "M8 17h2" }
            }
        }
    }
}

#[component]
fn InvoiceIcon() -> Element {
    rsx! {
        span { aria_hidden: "true",
            svg {
                class: "lucide",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M4 2v20l2-2 2 2 2-2 2 2 2-2 2 2 2-2 2 2V2l-2 2-2-2-2 2-2-2-2 2-2-2-2 2-2-2Z" }
                path { d: "M16 8h-6" }
                path { d: "M16 12h-6" }
                path { d: "M13 16h-3" }
            }
        }
    }
}
