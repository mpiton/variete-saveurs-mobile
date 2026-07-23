//! Draft form screen: stacked sections (client, dates, payment terms) bound
//! to the domain `DocumentInput`, with debounced auto-save to the draft store.
//! The line editor lands in task 15, the issue flow in task 20.

use std::time::Duration;

use chrono::Utc;
use dioxus::prelude::*;
use tokio::time::sleep;

use crate::domain::{
    db::{load_draft, save_draft},
    models::{ClientKind, DocumentInput, DocumentKind},
};

use super::{
    app::{DatabaseContext, Route},
    components::{Button, ButtonVariant, ErrorBlock, OutlinedField, SegmentedButton},
};

const AUTOSAVE_DEBOUNCE: Duration = Duration::from_millis(500);

#[component]
pub(super) fn Form() -> Element {
    let database = use_context::<DatabaseContext>();
    let navigator = use_navigator();
    let initial_database = database.clone();
    let draft = use_signal(move || load_initial_draft(&initial_database));
    let edit_generation = use_signal(|| 0_u64);
    let mut save_error = use_signal(|| None::<String>);

    // Debounced auto-save: every edit bumps `edit_generation`; the save only
    // runs once the generation has been stable for AUTOSAVE_DEBOUNCE.
    use_effect(move || {
        let generation = edit_generation();
        if generation == 0 {
            return;
        }
        let database = database.clone();
        spawn(async move {
            sleep(AUTOSAVE_DEBOUNCE).await;
            if *edit_generation.peek() != generation {
                return;
            }
            let snapshot = draft.read().clone();
            let Some(current) = snapshot else { return };
            match persist_draft(&database, &current) {
                Ok(()) => save_error.set(None),
                Err(error) => save_error.set(Some(error)),
            }
        });
    });

    let Some(current) = draft.read().clone() else {
        return rsx! {
            section { class: "screen", aria_label: "Brouillon introuvable",
                ErrorBlock {
                    title: "Brouillon introuvable".to_string(),
                    message: "Aucun brouillon en cours. Créez un devis ou une facture depuis l’accueil.".to_string(),
                }
                Button {
                    label: "Retour à l’accueil".to_string(),
                    variant: ButtonVariant::Tonal,
                    onclick: move |_| {
                        navigator.push(Route::Home {});
                    },
                }
            }
        };
    };

    let draft_title = draft_title(&current.kind).to_string();
    let issue_label = issue_label(&current.kind).to_string();
    let is_professional = current.client.kind == ClientKind::Professional;
    let save_error_message = save_error();

    rsx! {
        section { class: "screen form-screen", aria_labelledby: "form-draft-title",
            h2 { id: "form-draft-title", "{draft_title}" }

            section { class: "form-section", aria_labelledby: "form-client-title",
                h2 { id: "form-client-title", "Client" }
                SegmentedButton {
                    label: "Type de client".to_string(),
                    options: vec!["Particulier".to_string(), "Professionnel".to_string()],
                    selected: client_kind_index(&current.client.kind),
                    on_select: move |index| {
                        apply_edit(draft, edit_generation, |draft| {
                            draft.client.kind = client_kind_for_index(index);
                        });
                    },
                }
                OutlinedField {
                    label: "Nom".to_string(),
                    name: "client-name".to_string(),
                    value: current.client.name.clone(),
                    oninput: move |event: FormEvent| {
                        apply_edit(draft, edit_generation, |draft| {
                            draft.client.name = event.value();
                        });
                    },
                }
                OutlinedField {
                    label: "Adresse".to_string(),
                    name: "client-address".to_string(),
                    value: current.client.address.clone(),
                    oninput: move |event: FormEvent| {
                        apply_edit(draft, edit_generation, |draft| {
                            draft.client.address = event.value();
                        });
                    },
                }
                OutlinedField {
                    label: "Email".to_string(),
                    name: "client-email".to_string(),
                    input_type: "email".to_string(),
                    input_mode: "email".to_string(),
                    value: current.client.email.clone().unwrap_or_default(),
                    oninput: move |event: FormEvent| {
                        apply_edit(draft, edit_generation, |draft| {
                            draft.client.email = optional_text(&event.value());
                        });
                    },
                }
                OutlinedField {
                    label: "Téléphone".to_string(),
                    name: "client-phone".to_string(),
                    input_type: "tel".to_string(),
                    input_mode: "tel".to_string(),
                    value: current.client.phone.clone().unwrap_or_default(),
                    oninput: move |event: FormEvent| {
                        apply_edit(draft, edit_generation, |draft| {
                            draft.client.phone = optional_text(&event.value());
                        });
                    },
                }
                if is_professional {
                    OutlinedField {
                        label: "SIRET".to_string(),
                        name: "client-business-id".to_string(),
                        input_mode: "numeric".to_string(),
                        value: current.client.business_id.clone().unwrap_or_default(),
                        oninput: move |event: FormEvent| {
                            apply_edit(draft, edit_generation, |draft| {
                                draft.client.business_id = optional_text(&event.value());
                            });
                        },
                    }
                    OutlinedField {
                        label: "Adresse de facturation".to_string(),
                        name: "client-billing-address".to_string(),
                        value: current.client.billing_address.clone().unwrap_or_default(),
                        oninput: move |event: FormEvent| {
                            apply_edit(draft, edit_generation, |draft| {
                                draft.client.billing_address = optional_text(&event.value());
                            });
                        },
                    }
                }
            }

            section { class: "form-section", aria_labelledby: "form-dates-title",
                h2 { id: "form-dates-title", "Dates" }
                OutlinedField {
                    label: "Date d’émission".to_string(),
                    name: "issue-date".to_string(),
                    input_type: "date".to_string(),
                    value: current.issue_date.clone(),
                    oninput: move |event: FormEvent| {
                        apply_edit(draft, edit_generation, |draft| {
                            draft.issue_date = event.value();
                        });
                    },
                }
                OutlinedField {
                    label: "Date de l’événement".to_string(),
                    name: "event-date".to_string(),
                    input_type: "date".to_string(),
                    value: current.event_date.clone(),
                    oninput: move |event: FormEvent| {
                        apply_edit(draft, edit_generation, |draft| {
                            draft.event_date = event.value();
                        });
                    },
                }
            }

            section { class: "form-section", aria_labelledby: "form-terms-title",
                h2 { id: "form-terms-title", "Conditions" }
                OutlinedField {
                    label: "Conditions de paiement".to_string(),
                    name: "payment-terms".to_string(),
                    placeholder: "À réception".to_string(),
                    value: current.payment_terms.clone(),
                    oninput: move |event: FormEvent| {
                        apply_edit(draft, edit_generation, |draft| {
                            draft.payment_terms = event.value();
                        });
                    },
                }
            }

            if let Some(error) = save_error_message {
                ErrorBlock {
                    title: "Sauvegarde impossible".to_string(),
                    message: error,
                }
            }

            footer { class: "form-action-bar",
                Button {
                    label: "Aperçu".to_string(),
                    variant: ButtonVariant::Tonal,
                    onclick: move |_| {
                        navigator.push(Route::Preview {});
                    },
                }
                Button {
                    label: issue_label,
                    // Branché sur le flux d’émission dans la tâche 20.
                    disabled: true,
                    onclick: move |_| {},
                }
            }
        }
    }
}

fn apply_edit(
    mut draft: Signal<Option<DocumentInput>>,
    mut edit_generation: Signal<u64>,
    mutate: impl FnOnce(&mut DocumentInput),
) {
    if let Some(current) = draft.write().as_mut() {
        mutate(current);
        *edit_generation.write() += 1;
    }
}

fn load_initial_draft(database: &DatabaseContext) -> Option<DocumentInput> {
    let database = database.as_ref().ok()?;
    let connection = database.lock().ok()?;
    match load_draft(&connection) {
        Ok(draft) => draft,
        Err(error) => {
            eprintln!("Draft load failed: {error}");
            None
        }
    }
}

fn persist_draft(database: &DatabaseContext, draft: &DocumentInput) -> Result<(), String> {
    let database = database.as_ref().map_err(Clone::clone)?;
    let connection = database
        .lock()
        .map_err(|_| "Impossible d’accéder aux données locales.".to_string())?;
    save_draft(&connection, draft, &Utc::now().to_rfc3339()).map_err(|error| {
        eprintln!("Draft auto-save failed: {error}");
        "Les modifications ne sont pas enregistrées.".to_string()
    })
}

fn draft_title(kind: &DocumentKind) -> &'static str {
    match kind {
        DocumentKind::Quote => "Brouillon de devis",
        DocumentKind::Invoice => "Brouillon de facture",
    }
}

fn issue_label(kind: &DocumentKind) -> &'static str {
    match kind {
        DocumentKind::Quote => "Émettre le devis",
        DocumentKind::Invoice => "Émettre la facture",
    }
}

fn client_kind_index(kind: &ClientKind) -> usize {
    match kind {
        ClientKind::Individual => 0,
        ClientKind::Professional => 1,
    }
}

fn client_kind_for_index(index: usize) -> ClientKind {
    match index {
        1 => ClientKind::Professional,
        _ => ClientKind::Individual,
    }
}

fn optional_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        client_kind_for_index, client_kind_index, draft_title, issue_label, optional_text,
    };
    use crate::domain::models::{ClientKind, DocumentKind};

    #[test]
    fn draft_title_and_issue_label_adapt_to_the_document_kind() {
        assert_eq!(draft_title(&DocumentKind::Quote), "Brouillon de devis");
        assert_eq!(draft_title(&DocumentKind::Invoice), "Brouillon de facture");
        assert_eq!(issue_label(&DocumentKind::Quote), "Émettre le devis");
        assert_eq!(issue_label(&DocumentKind::Invoice), "Émettre la facture");
    }

    #[test]
    fn client_kind_segment_index_round_trips() {
        for (index, kind) in [(0, ClientKind::Individual), (1, ClientKind::Professional)] {
            assert_eq!(client_kind_index(&kind), index);
            assert_eq!(client_kind_for_index(index), kind);
        }
        assert_eq!(client_kind_for_index(2), ClientKind::Individual);
    }

    #[test]
    fn optional_text_trims_and_drops_empty_values() {
        assert_eq!(optional_text(""), None);
        assert_eq!(optional_text("   "), None);
        assert_eq!(
            optional_text("  client@example.com "),
            Some("client@example.com".to_string())
        );
    }
}
