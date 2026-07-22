use dioxus::prelude::*;

use super::{
    BadgeKind, BottomSheet, Button, ButtonVariant, DocumentCard, EmptyState, ErrorBlock, Fab,
    FabMenu, OutlinedField, SegmentedButton, Snackbar, StatusBadge,
};

#[derive(Clone, Copy)]
struct DemoState {
    label: &'static str,
    id: &'static str,
    class: &'static str,
    value: &'static str,
    disabled: bool,
    loading: bool,
    error: bool,
}

const DEMO_STATES: [DemoState; 6] = [
    DemoState {
        label: "Défaut",
        id: "default",
        class: "",
        value: "",
        disabled: false,
        loading: false,
        error: false,
    },
    DemoState {
        label: "Pressé",
        id: "pressed",
        class: "demo-state--pressed",
        value: "",
        disabled: false,
        loading: false,
        error: false,
    },
    DemoState {
        label: "Focus",
        id: "focus",
        class: "demo-state--focus",
        value: "42,00",
        disabled: false,
        loading: false,
        error: false,
    },
    DemoState {
        label: "Désactivé",
        id: "disabled",
        class: "",
        value: "42,00",
        disabled: true,
        loading: false,
        error: false,
    },
    DemoState {
        label: "Chargement",
        id: "loading",
        class: "",
        value: "42,00",
        disabled: false,
        loading: true,
        error: false,
    },
    DemoState {
        label: "Erreur",
        id: "error",
        class: "",
        value: "-1",
        disabled: false,
        loading: false,
        error: true,
    },
];

#[component]
pub fn ComponentsDemo() -> Element {
    let mut fab_open = use_signal(|| true);
    let mut sheet_open = use_signal(|| false);

    rsx! {
        section { class: "screen component-demo", aria_labelledby: "components-title",
            header { class: "component-demo__intro",
                h2 { id: "components-title", "Composants Material 3" }
                p { "Démonstration temporaire des variantes et états interactifs." }
            }
            ButtonShowcase {}
            FabShowcase {}
            section { class: "component-demo__section", aria_labelledby: "fab-menu-title",
                h3 { id: "fab-menu-title", "FAB menu" }
                FabMenu {
                    id: "document-fab-menu",
                    open: fab_open(),
                    on_toggle: move |_| fab_open.toggle(),
                    on_quote: move |_| {},
                    on_invoice: move |_| {},
                }
            }
            FieldShowcase {}
            DocumentShowcase {}
            SegmentedShowcase {}
            section { class: "component-demo__section", aria_labelledby: "feedback-title",
                h3 { id: "feedback-title", "Feedback et conteneurs" }
                div { class: "component-demo__stack",
                    div { class: "component-demo__badges", aria_label: "Badges de statut",
                        StatusBadge { kind: BadgeKind::Sent }
                        StatusBadge { kind: BadgeKind::Invoiced }
                    }
                    Snackbar { message: "Devis n° 10 émis", announce: false }
                    ErrorBlock {
                        title: "Émission impossible",
                        message: "Vérifiez les champs signalés avant de réessayer.",
                    }
                    EmptyState {
                        message: "Aucun document pour le moment.",
                        action_label: "Créer un devis",
                        onclick: move |_| {},
                    }
                    Button {
                        label: "Ouvrir la bottom sheet",
                        variant: ButtonVariant::Outlined,
                        onclick: move |_| sheet_open.set(true),
                    }
                }
            }
            BottomSheet {
                id: "catalog-bottom-sheet",
                title: "Choisir dans le catalogue",
                open: sheet_open(),
                on_dismiss: move |_| sheet_open.set(false),
                p { "La poignée, le scrim et le rayon supérieur suivent Material 3." }
                Button {
                    label: "Fermer",
                    variant: ButtonVariant::Text,
                    onclick: move |_| sheet_open.set(false),
                }
            }
        }
    }
}

#[component]
fn ButtonShowcase() -> Element {
    rsx! {
        section { class: "component-demo__section", aria_labelledby: "buttons-title",
            h3 { id: "buttons-title", "Boutons" }
            div { class: "component-demo__variants",
                for (label, variant) in [
                    ("Filled", ButtonVariant::Filled),
                    ("Tonal", ButtonVariant::Tonal),
                    ("Outlined", ButtonVariant::Outlined),
                    ("Text", ButtonVariant::Text),
                ] {
                    Button { label, variant, onclick: move |_| {} }
                }
            }
            div { class: "component-demo__states",
                for state in DEMO_STATES {
                    StateExample { label: state.label, state_class: state.class,
                        Button {
                            label: if state.error { "Réessayer" } else { "Enregistrer" },
                            disabled: state.disabled,
                            loading: state.loading,
                            error: state.error,
                            announce_error: false,
                            onclick: move |_| {},
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FabShowcase() -> Element {
    rsx! {
        section { class: "component-demo__section", aria_labelledby: "fab-title",
            h3 { id: "fab-title", "FAB 56 dp" }
            div { class: "component-demo__states component-demo__states--compact",
                for state in DEMO_STATES {
                    StateExample { label: state.label, state_class: state.class,
                        Fab {
                            label: "Créer",
                            disabled: state.disabled,
                            loading: state.loading,
                            error: state.error,
                            announce_error: false,
                            onclick: move |_| {},
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FieldShowcase() -> Element {
    rsx! {
        section { class: "component-demo__section", aria_labelledby: "fields-title",
            h3 { id: "fields-title", "Inputs outlined" }
            div { class: "component-demo__states",
                for state in DEMO_STATES {
                    StateExample { label: state.label, state_class: state.class,
                        DemoField {
                            id_suffix: state.id,
                            value: state.value,
                            disabled: state.disabled,
                            loading: state.loading,
                            error: state.error,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DemoField(
    id_suffix: String,
    value: String,
    #[props(default)] disabled: bool,
    #[props(default)] loading: bool,
    #[props(default)] error: bool,
) -> Element {
    let error_message = error.then(|| "Le prix doit être positif.".to_string());

    rsx! {
        OutlinedField {
            label: "Prix unitaire",
            name: "unit-price",
            id_suffix: Some(id_suffix),
            value,
            input_type: "text",
            input_mode: "decimal",
            placeholder: "Ex. 42,00",
            disabled,
            loading,
            error: error_message,
            oninput: move |_| {},
        }
    }
}

#[component]
fn DocumentShowcase() -> Element {
    rsx! {
        section { class: "component-demo__section", aria_labelledby: "cards-title",
            h3 { id: "cards-title", "Cartes document" }
            div { class: "component-demo__states",
                for state in DEMO_STATES {
                    StateExample { label: state.label, state_class: state.class,
                        DocumentCard {
                            document_type: "Devis",
                            number: 10,
                            client: "Mairie de Lyon",
                            total: "125,00 €",
                            sent: true,
                            invoiced: true,
                            disabled: state.disabled,
                            loading: state.loading,
                            error: state.error,
                            announce_error: false,
                            onclick: move |_| {},
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SegmentedShowcase() -> Element {
    rsx! {
        section { class: "component-demo__section", aria_labelledby: "segmented-title",
            h3 { id: "segmented-title", "Segmented button" }
            div { class: "component-demo__states",
                for state in DEMO_STATES {
                    StateExample { label: state.label, state_class: state.class,
                        SegmentedButton {
                            label: "Filtrer les documents",
                            options: vec!["Tous".to_string(), "Devis".to_string(), "Factures".to_string()],
                            selected: 0,
                            disabled: state.disabled,
                            loading: state.loading,
                            error: state.error,
                            announce_error: false,
                            on_select: move |_| {},
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StateExample(label: String, state_class: String, children: Element) -> Element {
    let class = format!("component-state {state_class}");

    rsx! {
        div { class,
            span { class: "component-state__label", "{label}" }
            div { class: "component-state__preview", {children} }
        }
    }
}
