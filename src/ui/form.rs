//! Draft form screen: stacked sections (client, dates, lines, payment terms)
//! bound to the domain `DocumentInput`, with debounced auto-save to the draft
//! store. Lines are summarized as rows and edited in a bottom sheet
//! (`LineSheet`); the issue flow lands in task 20.

use std::time::Duration;

use chrono::Utc;
use dioxus::prelude::*;
use tokio::time::sleep;

use crate::domain::{
    db::{load_draft, save_draft},
    models::{ClientKind, DocumentInput, DocumentKind, LineInput},
    money::{format_eur, parse_eur_to_cents},
    validation::{MAX_LINE_AMOUNT_CENTS, MAX_LINE_QUANTITY, MAX_UNIT_PRICE_CENTS},
};

use super::{
    app::{DatabaseContext, Route},
    components::{
        Button, ButtonVariant, ErrorBlock, LineEditorState, LineSheet, OutlinedField,
        SegmentedButton,
    },
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
    let line_editor = use_signal(|| None::<LineEditorState>);

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
    let line_count = current.lines.len();
    let (can_move_up, can_move_down) = line_editor
        .read()
        .as_ref()
        .and_then(|state| state.index)
        .map_or((false, false), |index| (index > 0, index + 1 < line_count));

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

            section { class: "form-section", aria_labelledby: "form-lines-title",
                h2 { id: "form-lines-title", "Prestations" }
                if current.lines.is_empty() {
                    p { "Aucune prestation pour l’instant." }
                } else {
                    ul { class: "line-list",
                        // Index keys are acceptable here: rows are stateless
                        // (content is derived from the line, no local state or
                        // focus to preserve across reorders), and `LineInput`
                        // has no stable identifier to key on.
                        for (index, line) in current.lines.iter().enumerate() {
                            li { key: "{index}",
                                button {
                                    class: "line-row__main",
                                    r#type: "button",
                                    aria_label: line_row_label(index, line),
                                    onclick: move |_| open_line_editor(draft, line_editor, index),
                                    if let Some(group) = &line.group {
                                        span { class: "line-row__group", "{group}" }
                                    }
                                    span { class: "line-row__description", "{line_description(line)}" }
                                    span { class: "line-row__detail",
                                        "{line.quantity} × {format_eur(line.unit_price_cents)}"
                                    }
                                    span { class: "line-row__amount", "{format_eur(line.amount_cents())}" }
                                }
                            }
                        }
                    }
                }
                Button {
                    label: "Ajouter une prestation".to_string(),
                    variant: ButtonVariant::Tonal,
                    onclick: move |_| open_new_line_editor(line_editor),
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

            div { class: "form-sticky",
                p { class: "total-pill", aria_live: "polite",
                    span { class: "total-pill__label", "Total" }
                    span { class: "total-pill__amount", "{format_eur(current.total_cents())}" }
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

            LineSheet {
                editor: line_editor,
                can_move_up,
                can_move_down,
                on_save: move |_| save_line(draft, edit_generation, line_editor),
                on_delete: move |_| delete_line(draft, edit_generation, line_editor),
                on_move_up: move |_| move_draft_line(draft, edit_generation, line_editor, true),
                on_move_down: move |_| move_draft_line(draft, edit_generation, line_editor, false),
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

fn open_new_line_editor(mut line_editor: Signal<Option<LineEditorState>>) {
    line_editor.set(Some(LineEditorState {
        quantity: "1".to_string(),
        ..LineEditorState::default()
    }));
}

fn open_line_editor(
    draft: Signal<Option<DocumentInput>>,
    mut line_editor: Signal<Option<LineEditorState>>,
    index: usize,
) {
    let line = draft
        .read()
        .as_ref()
        .and_then(|draft| draft.lines.get(index))
        .cloned();
    if let Some(line) = line {
        line_editor.set(Some(LineEditorState {
            index: Some(index),
            description: line.description,
            quantity: line.quantity.to_string(),
            price: cents_to_euro_input(line.unit_price_cents),
            group: line.group.unwrap_or_default(),
            ..LineEditorState::default()
        }));
    }
}

fn save_line(
    draft: Signal<Option<DocumentInput>>,
    edit_generation: Signal<u64>,
    mut line_editor: Signal<Option<LineEditorState>>,
) {
    let Some(mut state) = line_editor.read().clone() else {
        return;
    };
    match line_from_editor(&mut state) {
        Some(line) => {
            apply_edit(draft, edit_generation, |draft| match state.index {
                Some(index) if index < draft.lines.len() => draft.lines[index] = line,
                _ => draft.lines.push(line),
            });
            line_editor.set(None);
        }
        None => line_editor.set(Some(state)),
    }
}

/// Commits the sheet draft to a line, annotating the draft with per-field
/// errors when a field is unusable. Bounds mirror the domain validation
/// limits so mistakes are flagged here instead of at issue time.
fn line_from_editor(state: &mut LineEditorState) -> Option<LineInput> {
    let quantity = parse_quantity(&state.quantity);
    let unit_price_cents = parse_eur_to_cents(&state.price);
    state.quantity_error = quantity_error(quantity);
    state.price_error = price_error(unit_price_cents);
    if state.quantity_error.is_some() || state.price_error.is_some() {
        return None;
    }
    let (Some(quantity), Some(unit_price_cents)) = (quantity, unit_price_cents) else {
        return None;
    };
    if quantity
        .checked_mul(unit_price_cents)
        .is_none_or(|amount| amount > MAX_LINE_AMOUNT_CENTS)
    {
        state.price_error = Some("Le montant de la ligne dépasse la limite autorisée.".to_string());
        return None;
    }
    Some(LineInput {
        group: optional_text(&state.group),
        description: state.description.trim().to_string(),
        quantity,
        unit_price_cents,
    })
}

fn delete_line(
    draft: Signal<Option<DocumentInput>>,
    edit_generation: Signal<u64>,
    mut line_editor: Signal<Option<LineEditorState>>,
) {
    let index = line_editor.read().as_ref().and_then(|state| state.index);
    if let Some(index) = index {
        apply_edit(draft, edit_generation, |draft| {
            if index < draft.lines.len() {
                draft.lines.remove(index);
            }
        });
        line_editor.set(None);
    }
}

fn move_draft_line(
    draft: Signal<Option<DocumentInput>>,
    edit_generation: Signal<u64>,
    mut line_editor: Signal<Option<LineEditorState>>,
    up: bool,
) {
    let index = line_editor.read().as_ref().and_then(|state| state.index);
    let Some(index) = index else { return };
    let mut new_index = None;
    apply_edit(draft, edit_generation, |draft| {
        new_index = move_line(&mut draft.lines, index, up);
    });
    if let (Some(new_index), Some(state)) = (new_index, line_editor.write().as_mut()) {
        state.index = Some(new_index);
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

/// Strict integer shape (digits only, like the desktop form) — range checks
/// stay with the domain validation.
fn parse_quantity(value: &str) -> Option<i64> {
    let trimmed = value.trim();
    if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    trimmed.parse().ok()
}

fn quantity_error(parsed: Option<i64>) -> Option<String> {
    match parsed {
        Some(quantity) if (1..=MAX_LINE_QUANTITY).contains(&quantity) => None,
        Some(0) => Some("La quantité doit être positive.".to_string()),
        Some(_) => Some("La quantité dépasse la limite autorisée.".to_string()),
        None => Some("Saisir une quantité entière (ex. 12).".to_string()),
    }
}

fn price_error(parsed: Option<i64>) -> Option<String> {
    match parsed {
        Some(cents) if cents > MAX_UNIT_PRICE_CENTS => {
            Some("Le prix dépasse la limite autorisée.".to_string())
        }
        Some(_) => None,
        None => Some("Saisir un prix au format 12,34.".to_string()),
    }
}

/// Euro prefill for the sheet, parseable by `money::parse_eur_to_cents`
/// (no thousands separator). Negative amounts are display-only: they cannot
/// be typed back and the validation rejects them at issue time.
fn cents_to_euro_input(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    format!("{sign}{},{:02}", abs / 100, abs % 100)
}

/// Swaps the line with its neighbour. Returns the line's new index, or
/// `None` when the move is out of bounds.
fn move_line(lines: &mut [LineInput], index: usize, up: bool) -> Option<usize> {
    let target = if up {
        index.checked_sub(1)?
    } else {
        index.checked_add(1)?
    };
    if index < lines.len() && target < lines.len() {
        lines.swap(index, target);
        Some(target)
    } else {
        None
    }
}

fn line_description(line: &LineInput) -> &str {
    if line.description.is_empty() {
        "Sans désignation"
    } else {
        line.description.as_str()
    }
}

fn line_row_label(index: usize, line: &LineInput) -> String {
    let description = line_description(line);
    let group = line
        .group
        .as_deref()
        .map_or(String::new(), |group| format!(" ({group})"));
    format!(
        "Ligne {} : {}{}, {} × {}, {}",
        index + 1,
        description,
        group,
        line.quantity,
        format_eur(line.unit_price_cents),
        format_eur(line.amount_cents())
    )
}

#[cfg(test)]
mod tests {
    use super::{
        LineEditorState, cents_to_euro_input, client_kind_for_index, client_kind_index,
        draft_title, issue_label, line_from_editor, line_row_label, move_line, optional_text,
        parse_quantity, price_error, quantity_error,
    };
    use crate::domain::{
        models::{ClientKind, DocumentKind, LineInput},
        money::parse_eur_to_cents,
        validation::{MAX_LINE_QUANTITY, MAX_UNIT_PRICE_CENTS},
    };

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

    #[test]
    fn parse_quantity_accepts_digit_only_integers() {
        assert_eq!(parse_quantity("1"), Some(1));
        assert_eq!(parse_quantity(" 12 "), Some(12));
        assert_eq!(parse_quantity("0"), Some(0));
        for value in [
            "",
            "  ",
            "abc",
            "1,5",
            "1.5",
            "-3",
            "+3",
            "99999999999999999999",
        ] {
            assert_eq!(parse_quantity(value), None, "{value:?} should be rejected");
        }
    }

    #[test]
    fn quantity_error_flags_only_unusable_values() {
        assert_eq!(quantity_error(Some(12)), None);
        assert_eq!(quantity_error(Some(MAX_LINE_QUANTITY)), None);
        assert_eq!(
            quantity_error(Some(0)),
            Some("La quantité doit être positive.".to_string())
        );
        assert_eq!(
            quantity_error(Some(MAX_LINE_QUANTITY + 1)),
            Some("La quantité dépasse la limite autorisée.".to_string())
        );
        assert_eq!(
            quantity_error(None),
            Some("Saisir une quantité entière (ex. 12).".to_string())
        );
    }

    #[test]
    fn price_error_flags_only_unusable_values() {
        assert_eq!(price_error(Some(0)), None);
        assert_eq!(price_error(Some(MAX_UNIT_PRICE_CENTS)), None);
        assert_eq!(
            price_error(Some(MAX_UNIT_PRICE_CENTS + 1)),
            Some("Le prix dépasse la limite autorisée.".to_string())
        );
        assert_eq!(
            price_error(None),
            Some("Saisir un prix au format 12,34.".to_string())
        );
    }

    #[test]
    fn line_from_editor_builds_a_line_from_valid_fields() {
        let mut state = LineEditorState {
            index: None,
            description: " Mini Burgers ".to_string(),
            quantity: "50".to_string(),
            price: "0,85".to_string(),
            group: " Salé ".to_string(),
            ..LineEditorState::default()
        };

        assert_eq!(
            line_from_editor(&mut state),
            Some(LineInput {
                group: Some("Salé".to_string()),
                description: "Mini Burgers".to_string(),
                quantity: 50,
                unit_price_cents: 85,
            })
        );
        assert_eq!(state.quantity_error, None);
        assert_eq!(state.price_error, None);
    }

    #[test]
    fn line_from_editor_annotates_invalid_fields_without_losing_input() {
        let mut state = LineEditorState {
            index: Some(2),
            description: "Mini Burgers".to_string(),
            quantity: "0".to_string(),
            price: "douze".to_string(),
            group: String::new(),
            ..LineEditorState::default()
        };

        assert_eq!(line_from_editor(&mut state), None);
        assert_eq!(
            state.quantity_error,
            Some("La quantité doit être positive.".to_string())
        );
        assert_eq!(
            state.price_error,
            Some("Saisir un prix au format 12,34.".to_string())
        );
        assert_eq!(state.index, Some(2));
        assert_eq!(state.description, "Mini Burgers");
        assert_eq!(state.quantity, "0");
        assert_eq!(state.price, "douze");
    }

    #[test]
    fn cents_to_euro_input_prefills_a_parseable_price() {
        assert_eq!(cents_to_euro_input(0), "0,00");
        assert_eq!(cents_to_euro_input(85), "0,85");
        assert_eq!(cents_to_euro_input(1_234), "12,34");
        assert_eq!(cents_to_euro_input(8_500), "85,00");
        assert_eq!(cents_to_euro_input(-85), "-0,85");
    }

    #[test]
    fn euro_input_prefill_round_trips_through_the_domain_parser() {
        for cents in [0, 5, 50, 85, 1_234, 1_000_000, i64::MAX] {
            assert_eq!(parse_eur_to_cents(&cents_to_euro_input(cents)), Some(cents));
        }
    }

    #[test]
    fn line_from_editor_rejects_line_amount_above_the_domain_cap() {
        let mut state = LineEditorState {
            quantity: "11".to_string(),
            price: cents_to_euro_input(MAX_UNIT_PRICE_CENTS),
            ..LineEditorState::default()
        };

        assert_eq!(line_from_editor(&mut state), None);
        assert_eq!(
            state.price_error,
            Some("Le montant de la ligne dépasse la limite autorisée.".to_string())
        );

        // One unit fewer: exactly at the cap — accepted.
        state.quantity = "10".to_string();
        assert!(line_from_editor(&mut state).is_some());
    }

    #[test]
    fn move_line_swaps_with_neighbour_and_reports_the_new_index() {
        let mut lines = sample_lines();

        assert_eq!(move_line(&mut lines, 1, true), Some(0));
        assert_eq!(lines[0].description, "B");
        assert_eq!(lines[1].description, "A");

        assert_eq!(move_line(&mut lines, 0, false), Some(1));
        assert_eq!(lines[1].description, "B");
    }

    #[test]
    fn move_line_refuses_out_of_bounds_moves() {
        let mut lines = sample_lines();

        assert_eq!(move_line(&mut lines, 0, true), None);
        assert_eq!(move_line(&mut lines, 2, false), None);
        assert_eq!(move_line(&mut lines, 3, true), None);
        assert_eq!(lines[0].description, "A");
        assert_eq!(lines[2].description, "C");
    }

    #[test]
    fn line_row_label_summarizes_the_line_in_french() {
        let line = LineInput {
            group: Some("Salé".to_string()),
            description: "Mini Burgers".to_string(),
            quantity: 50,
            unit_price_cents: 85,
        };
        assert_eq!(
            line_row_label(2, &line),
            "Ligne 3 : Mini Burgers (Salé), 50 × 0,85 €, 42,50 €"
        );

        let untitled = LineInput {
            description: String::new(),
            ..line
        };
        assert!(line_row_label(0, &untitled).starts_with("Ligne 1 : Sans désignation (Salé),"));
    }

    fn sample_lines() -> Vec<LineInput> {
        ["A", "B", "C"]
            .into_iter()
            .map(|description| LineInput {
                group: None,
                description: description.to_string(),
                quantity: 1,
                unit_price_cents: 100,
            })
            .collect()
    }
}
