//! Issued document record (DESIGN.md §5): full summary (kind + number,
//! client, dates, total, status badges) with read-only collapsible lines, and
//! the action stack in the bottom third (Règle du Pouce). An issued document
//! is frozen (émis = figé, CONTEXT.md): this screen has no edit entry point —
//! no button, no hidden long-press. Actions are wired as their tasks land
//! (22 share, 23 convert, 24 duplicate, 26/27 send); until then they render
//! as disabled placeholders.

use std::time::Duration;

use dioxus::prelude::*;
use rusqlite::Connection;
use tokio::time::sleep;

use crate::domain::{
    db::get_document,
    models::{Document, DocumentKind},
    money::format_eur,
    render::format_date,
};

use super::{
    app::{DatabaseContext, Route},
    components::{BadgeKind, Button, ButtonVariant, ErrorBlock, Snackbar, StatusBadge},
    issue::{ExportPhase, IssueFlow, IssuePhase, dismiss_notice, reset_issue_flow, retry_export},
};

const NOTICE_DURATION: Duration = Duration::from_secs(4);

/// What the fiche shows, loaded fresh from the local database: the query
/// recomputes the derived statuses (facturé, envoyé — task 08) on every
/// visit, so conversions and sends are reflected without any cache to bust.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct RecordData {
    pub document: Document,
    /// Number of the quote this invoice was converted from — the reference
    /// the gérante recognizes, shown discreetly on the invoice.
    pub source_quote_number: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecordError {
    DocumentNotFound,
    Unavailable,
}

pub(super) fn load_record(connection: &Connection, id: i64) -> Result<RecordData, RecordError> {
    let document = get_document(connection, id).map_err(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => RecordError::DocumentNotFound,
        error => {
            eprintln!("Record document query failed: {error}");
            RecordError::Unavailable
        }
    })?;
    let source_quote_number = match document.source_quote_id {
        Some(source_id) => match get_document(connection, source_id) {
            Ok(quote) => Some(quote.number),
            Err(error) => {
                // The foreign key guarantees the quote exists; if the lookup
                // still fails, the fiche simply omits the reference.
                eprintln!("Record source quote query failed: {error}");
                None
            }
        },
        None => None,
    };
    Ok(RecordData {
        document,
        source_quote_number,
    })
}

/// « Convertir en facture » only makes sense on a quote not yet converted —
/// the derived `is_invoiced` status (task 08) drives the visibility.
pub(super) fn convert_action_visible(kind: &DocumentKind, is_invoiced: bool) -> bool {
    matches!(kind, DocumentKind::Quote) && !is_invoiced
}

#[component]
pub(super) fn Record(id: i64) -> Element {
    let database = use_context::<DatabaseContext>();
    let navigator = use_navigator();
    let issue_flow = use_context::<IssueFlow>();

    // Post-issue state published by the flow: the fiche confirms the emission
    // (snackbar) and carries the re-export path when the PDF failed (ARCHI §4
    // — the number is never rolled back after commit).
    let (notice, export_running, export_failed) = match &*issue_flow.0.read() {
        IssuePhase::Issued(state) if state.document.id == id => (
            state.notice.clone(),
            state.export == ExportPhase::Running,
            state.export == ExportPhase::Failed,
        ),
        _ => (None, false, false),
    };

    // The snackbar is transient (DESIGN.md §6): auto-dismiss after a few
    // seconds, and the timer only ever dismisses ITS notice — a newer one
    // (retry result, newer issuance) survives an older timer.
    let notice_flow = issue_flow;
    use_effect(move || {
        let expected = match &*notice_flow.0.read() {
            IssuePhase::Issued(state) => state.notice.clone(),
            _ => None,
        };
        if let Some(expected) = expected {
            spawn(async move {
                sleep(NOTICE_DURATION).await;
                dismiss_notice(notice_flow, &expected);
            });
        }
    });

    // Leaving the fiche ends the post-emission moment: no stale snackbar or
    // retry block on later visits (the aperçu's « Exporter » stays the
    // standing re-export path).
    let reset_flow = issue_flow;
    use_drop(move || reset_issue_flow(reset_flow));

    match load_from_context(&database, id) {
        Err(error) => {
            let (title, message) = error_message(error);
            rsx! {
                section { class: "screen record-screen",
                    div { class: "placeholder-panel",
                        ErrorBlock {
                            title: title.to_string(),
                            message: message.to_string(),
                        }
                        Button {
                            label: "Retour".to_string(),
                            variant: ButtonVariant::Outlined,
                            onclick: move |_| navigator.go_back(),
                        }
                    }
                }
            }
        }
        Ok(data) => {
            let document = &data.document;
            let input = &document.input;
            let client = &input.client;
            let title = format!("{} n° {}", input.kind.label(), document.number);
            let issue_date = format_date(&input.issue_date);
            let event_date = format_date(&input.event_date);
            let total = format_eur(document.total_cents);
            let payment_terms = input.payment_terms.trim();
            let contact = [client.email.as_deref(), client.phone.as_deref()]
                .into_iter()
                .flatten()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            let sent = document.is_sent();
            let invoiced = document.is_invoiced;
            let show_convert = convert_action_visible(&input.kind, invoiced);
            let line_count = input.lines.len();
            let source_reference = data
                .source_quote_number
                .map(|number| format!("Issue du devis n° {number}"));

            rsx! {
                section { class: "screen record-screen",
                    section { class: "record-summary", aria_labelledby: "record-title",
                        div { class: "record-summary__heading",
                            h2 { id: "record-title", "{title}" }
                            strong { class: "record-summary__total", "{total}" }
                        }
                        p { class: "record-summary__client", "{client.name}" }
                        p { class: "record-summary__detail", "{client.address}" }
                        if !contact.is_empty() {
                            p { class: "record-summary__detail", "{contact}" }
                        }
                        p { class: "record-summary__detail", "Date d’émission : {issue_date}" }
                        p { class: "record-summary__detail", "Date de l’événement : {event_date}" }
                        if !payment_terms.is_empty() {
                            p { class: "record-summary__detail", "Conditions de paiement : {payment_terms}" }
                        }
                        if let Some(reference) = source_reference {
                            p { class: "record-summary__source", "{reference}" }
                        }
                        if sent || invoiced {
                            div { class: "record-summary__badges",
                                if sent {
                                    StatusBadge { kind: BadgeKind::Sent }
                                }
                                if invoiced {
                                    StatusBadge { kind: BadgeKind::Invoiced }
                                }
                            }
                        }
                    }

                    if export_running {
                        p { role: "status", aria_live: "polite", "Génération du PDF en cours…" }
                    }
                    if export_failed {
                        ErrorBlock {
                            title: "PDF non généré".to_string(),
                            message: "Le document est bien émis et son numéro est conservé. Réessayez l’export.".to_string(),
                        }
                        Button {
                            label: "Réessayer l’export".to_string(),
                            variant: ButtonVariant::Tonal,
                            onclick: move |_| retry_export(issue_flow),
                        }
                    }

                    details { class: "record-lines",
                        summary { "Prestations ({line_count})" }
                        ul { class: "line-list",
                            // Same index-key rationale as the form: rows are
                            // stateless and `LineInput` has no stable id.
                            for (index, line) in input.lines.iter().enumerate() {
                                li { key: "{index}",
                                    div { class: "record-line",
                                        if let Some(group) = &line.group {
                                            span { class: "line-row__group", "{group}" }
                                        }
                                        span { class: "line-row__description", "{line.description}" }
                                        span { class: "line-row__detail",
                                            "{line.quantity} × {format_eur(line.unit_price_cents)}"
                                        }
                                        span { class: "line-row__amount",
                                            "{format_eur(line.amount_cents())}"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div { class: "record-sticky",
                        footer { class: "chrome-action-bar record-action-bar", aria_label: "Actions du document",
                            Button {
                                label: "Aperçu".to_string(),
                                variant: ButtonVariant::Tonal,
                                onclick: move |_| {
                                    navigator.push(Route::Preview { document: Some(id) });
                                },
                            }
                            Button {
                                label: "Exporter le PDF / PNG".to_string(),
                                variant: ButtonVariant::Tonal,
                                // Branché avec le partage dans la tâche 22 ;
                                // l’aperçu garde l’export manuel d’ici là.
                                disabled: true,
                                onclick: move |_| {},
                            }
                            Button {
                                label: "Partager".to_string(),
                                variant: ButtonVariant::Tonal,
                                // Branché sur le share sheet dans la tâche 22.
                                disabled: true,
                                onclick: move |_| {},
                            }
                            Button {
                                label: "Envoyer par email".to_string(),
                                variant: ButtonVariant::Tonal,
                                // Branché sur l’envoi Brevo dans les tâches 26/27.
                                disabled: true,
                                onclick: move |_| {},
                            }
                            if show_convert {
                                Button {
                                    label: "Convertir en facture".to_string(),
                                    variant: ButtonVariant::Tonal,
                                    // Branché sur la conversion dans la tâche 23.
                                    disabled: true,
                                    onclick: move |_| {},
                                }
                            }
                            Button {
                                label: "Dupliquer".to_string(),
                                variant: ButtonVariant::Tonal,
                                // Branché sur la duplication dans la tâche 24.
                                disabled: true,
                                onclick: move |_| {},
                            }
                        }
                    }
                    if let Some(message) = notice {
                        Snackbar { message }
                    }
                }
            }
        }
    }
}

fn load_from_context(database: &DatabaseContext, id: i64) -> Result<RecordData, RecordError> {
    let database = database.as_ref().map_err(|_| RecordError::Unavailable)?;
    let connection = database.lock().map_err(|_| RecordError::Unavailable)?;
    load_record(&connection, id)
}

fn error_message(error: RecordError) -> (&'static str, &'static str) {
    match error {
        RecordError::DocumentNotFound => (
            "Document introuvable",
            "Ce document n’existe pas ou plus dans l’historique.",
        ),
        RecordError::Unavailable => (
            "Chargement impossible",
            "Impossible de charger le document.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    use crate::domain::{
        db::{issue_document, open_database},
        models::{ClientInput, ClientKind, DocumentInput, DocumentKind, LineInput},
    };

    use super::{RecordError, convert_action_visible, load_record};

    fn temp_connection() -> (NamedTempFile, Mutex<Connection>) {
        let file = NamedTempFile::new().expect("temp database file");
        let connection = open_database(file.path()).expect("open temp database");
        (file, connection)
    }

    fn sample_input(kind: DocumentKind) -> DocumentInput {
        DocumentInput {
            kind: kind.clone(),
            issue_date: "2026-07-24".to_string(),
            event_date: "2026-08-02".to_string(),
            payment_terms: match kind {
                DocumentKind::Quote => String::new(),
                DocumentKind::Invoice => "Comptant".to_string(),
            },
            client: ClientInput {
                kind: ClientKind::Individual,
                name: "Marie Dupont".to_string(),
                address: "12 rue des Lilas, 17130 Montendre".to_string(),
                email: None,
                phone: None,
                business_id: None,
                billing_address: None,
            },
            lines: vec![LineInput {
                group: None,
                description: "Pains spéciaux".to_string(),
                quantity: 10,
                unit_price_cents: 350,
            }],
        }
    }

    #[test]
    fn an_issued_quote_loads_without_a_source_reference() {
        let (_file, database) = temp_connection();
        let mut connection = database.lock().expect("lock database");
        let quote = issue_document(
            &mut connection,
            sample_input(DocumentKind::Quote),
            None,
            "2026-07-24T10:00:00Z",
        )
        .expect("issue quote");

        let record = load_record(&connection, quote.id).expect("record");
        assert_eq!(record.document.number, quote.number);
        assert_eq!(record.source_quote_number, None);
        assert!(!record.document.is_invoiced);
    }

    #[test]
    fn a_converted_invoice_references_its_source_quote_and_marks_it_invoiced() {
        let (_file, database) = temp_connection();
        let mut connection = database.lock().expect("lock database");
        let quote = issue_document(
            &mut connection,
            sample_input(DocumentKind::Quote),
            None,
            "2026-07-24T10:00:00Z",
        )
        .expect("issue quote");
        let invoice = issue_document(
            &mut connection,
            sample_input(DocumentKind::Invoice),
            Some(quote.id),
            "2026-07-25T10:00:00Z",
        )
        .expect("issue conversion invoice");

        // The invoice carries a discreet reference to its source quote (n°).
        let invoice_record = load_record(&connection, invoice.id).expect("invoice record");
        assert_eq!(invoice_record.source_quote_number, Some(quote.number));

        // …and the converted quote reads back as facturé (derived, task 08).
        let quote_record = load_record(&connection, quote.id).expect("quote record");
        assert!(quote_record.document.is_invoiced);
    }

    #[test]
    fn an_unknown_document_is_an_error() {
        let (_file, database) = temp_connection();
        let connection = database.lock().expect("lock database");
        assert_eq!(
            load_record(&connection, 999),
            Err(RecordError::DocumentNotFound)
        );
    }

    #[test]
    fn the_convert_action_is_only_visible_on_an_unconverted_quote() {
        assert!(convert_action_visible(&DocumentKind::Quote, false));
        assert!(!convert_action_visible(&DocumentKind::Quote, true));
        assert!(!convert_action_visible(&DocumentKind::Invoice, false));
        assert!(!convert_action_visible(&DocumentKind::Invoice, true));
    }
}
