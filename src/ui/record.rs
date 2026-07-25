//! Issued document record (DESIGN.md §5): full summary (kind + number,
//! client, dates, total, status badges) with read-only collapsible lines, and
//! the action stack in the bottom third (Règle du Pouce). An issued document
//! is frozen (émis = figé, CONTEXT.md): this screen has no edit entry point —
//! no button, no hidden long-press. Export, share (task 22), conversion
//! (task 23) and duplication (task 24) are live; sending is wired in tasks
//! 26/27 and renders as a disabled placeholder until then.

use std::time::Duration;

use chrono::Utc;
use dioxus::prelude::*;
use rusqlite::Connection;
use tokio::time::sleep;

use crate::domain::{
    convert::invoice_draft_from_quote,
    db::{get_document, load_draft, save_draft},
    duplicate::duplicate_draft_from_document,
    models::{Document, DocumentInput, DocumentKind},
    money::format_eur,
    render::format_date,
};

use super::{
    app::{DatabaseContext, Route},
    components::{
        BadgeKind, BottomSheet, Button, ButtonVariant, ErrorBlock, ShareSheet, Snackbar,
        StatusBadge,
    },
    issue::{
        ExportJobState, ExportPhase, IssueFlow, IssuePhase, dismiss_notice, reset_issue_flow,
        retry_export, start_export, use_export_notice_dismiss,
    },
    share::{share_file_names, use_share_flow},
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
    let share = use_share_flow();
    let export_state = use_signal_sync(|| ExportJobState::Ready);
    use_export_notice_dismiss(export_state);
    // Conversion (task 23): `Some`-like flags for the replace-draft
    // confirmation sheet and the last conversion failure, mirroring the
    // home's new-draft flow (task 13).
    let mut convert_confirmation = use_signal(|| false);
    let mut convert_error = use_signal(|| None::<String>);
    // Duplication (task 24): same guard, but only a draft with real content
    // is confirmed away — a blank one is replaced silently.
    let mut duplicate_confirmation = use_signal(|| false);
    let mut duplicate_error = use_signal(|| None::<String>);

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
    // retry block on later visits (a manual re-export stays available on the
    // fiche and the aperçu).
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
            let (pdf_name, png_name) = share_file_names(&input.kind, document.number);
            let share_input = input.clone();
            let share_number = document.number;
            let export_input = input.clone();
            let export_number = document.number;
            let convert_database = database.clone();
            let convert_quote = document.clone();
            let confirm_database = database.clone();
            let confirm_quote = document.clone();
            let duplicate_database = database.clone();
            let duplicate_document = document.clone();
            let confirm_duplicate_database = database.clone();
            let confirm_duplicate_document = document.clone();
            let manual_export_running = matches!(&*export_state.read(), ExportJobState::Running);
            let (manual_export_notice, manual_export_error) = match &*export_state.read() {
                ExportJobState::Done(message) => (Some(message.clone()), None),
                ExportJobState::Failed(message) => (None, Some(message.clone())),
                _ => (None, None),
            };

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
                    if let Some(message) = manual_export_error {
                        ErrorBlock {
                            title: "Export impossible".to_string(),
                            message,
                        }
                    }
                    if let Some(message) = share.error() {
                        ErrorBlock {
                            title: "Partage impossible".to_string(),
                            message,
                        }
                    }
                    if !convert_confirmation() {
                        if let Some(message) = convert_error() {
                            ErrorBlock {
                                title: "Conversion impossible".to_string(),
                                message,
                            }
                        }
                    }
                    if !duplicate_confirmation() {
                        if let Some(message) = duplicate_error() {
                            ErrorBlock {
                                title: "Duplication impossible".to_string(),
                                message,
                            }
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
                                loading: manual_export_running,
                                onclick: move |_| {
                                    start_export(export_state, export_input.clone(), export_number);
                                },
                            }
                            Button {
                                label: "Partager".to_string(),
                                variant: ButtonVariant::Tonal,
                                onclick: move |_| share.open_sheet(),
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
                                    onclick: move |_| {
                                        request_conversion(
                                            &convert_database,
                                            &convert_quote,
                                            navigator,
                                            convert_confirmation,
                                            convert_error,
                                        );
                                    },
                                }
                            }
                            Button {
                                label: "Dupliquer".to_string(),
                                variant: ButtonVariant::Tonal,
                                onclick: move |_| {
                                    request_duplication(
                                        &duplicate_database,
                                        &duplicate_document,
                                        navigator,
                                        duplicate_confirmation,
                                        duplicate_error,
                                    );
                                },
                            }
                        }
                    }
                    if let Some(message) = manual_export_notice {
                        Snackbar { message }
                    }
                    if let Some(message) = notice {
                        Snackbar { message }
                    }
                    ShareSheet {
                        state: share.state(),
                        pdf_name,
                        png_name,
                        on_pick: move |format| share.start(share_input.clone(), share_number, format),
                    }
                    BottomSheet {
                        id: "convert-replace-draft-sheet".to_string(),
                        title: "Remplacer le brouillon ?".to_string(),
                        open: convert_confirmation(),
                        error: convert_error().is_some(),
                        on_dismiss: move |_| {
                            convert_confirmation.set(false);
                            convert_error.set(None);
                        },
                        p { "Le brouillon actuel sera remplacé par la facture pré-remplie depuis ce devis." }
                        if let Some(message) = convert_error() {
                            ErrorBlock {
                                title: "Conversion impossible".to_string(),
                                message,
                            }
                        }
                        div { class: "home-confirmation-actions",
                            Button {
                                label: "Annuler".to_string(),
                                variant: ButtonVariant::Text,
                                onclick: move |_| {
                                    convert_confirmation.set(false);
                                    convert_error.set(None);
                                },
                            }
                            Button {
                                label: "Remplacer".to_string(),
                                onclick: move |_| {
                                    convert_error.set(None);
                                    match persist_conversion(&confirm_database, &confirm_quote) {
                                        Ok(()) => {
                                            convert_confirmation.set(false);
                                            navigator.push(Route::Form {});
                                        }
                                        Err(message) => convert_error.set(Some(message)),
                                    }
                                },
                            }
                        }
                    }
                    BottomSheet {
                        id: "duplicate-replace-draft-sheet".to_string(),
                        title: "Remplacer le brouillon ?".to_string(),
                        open: duplicate_confirmation(),
                        error: duplicate_error().is_some(),
                        on_dismiss: move |_| {
                            duplicate_confirmation.set(false);
                            duplicate_error.set(None);
                        },
                        p { "Le brouillon actuel sera remplacé par une copie de ce document." }
                        if let Some(message) = duplicate_error() {
                            ErrorBlock {
                                title: "Duplication impossible".to_string(),
                                message,
                            }
                        }
                        div { class: "home-confirmation-actions",
                            Button {
                                label: "Annuler".to_string(),
                                variant: ButtonVariant::Text,
                                onclick: move |_| {
                                    duplicate_confirmation.set(false);
                                    duplicate_error.set(None);
                                },
                            }
                            Button {
                                label: "Remplacer".to_string(),
                                onclick: move |_| {
                                    duplicate_error.set(None);
                                    match persist_duplication(
                                        &confirm_duplicate_database,
                                        &confirm_duplicate_document,
                                    ) {
                                        Ok(()) => {
                                            duplicate_confirmation.set(false);
                                            navigator.push(Route::Form {});
                                        }
                                        Err(message) => duplicate_error.set(Some(message)),
                                    }
                                },
                            }
                        }
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

/// « Convertir en facture » entry point: an existing draft must be confirmed
/// away before the pre-filled invoice replaces it — the same guard as the
/// home's new-document flow (task 13), since both overwrite the single draft.
fn request_conversion(
    database: &DatabaseContext,
    quote: &Document,
    navigator: dioxus_router::Navigator,
    mut confirmation: Signal<bool>,
    mut error: Signal<Option<String>>,
) {
    error.set(None);
    match draft_exists(database) {
        Ok(true) => confirmation.set(true),
        Ok(false) => match persist_conversion(database, quote) {
            Ok(()) => {
                navigator.push(Route::Form {});
            }
            Err(message) => error.set(Some(message)),
        },
        Err(message) => error.set(Some(message)),
    }
}

/// « Dupliquer » entry point (CONTEXT.md « dupliquer » — correcting an
/// issued document means duplicating it): the copy replaces the single draft
/// slot, so a draft with real content must be confirmed away first; a blank
/// one (an untouched home draft) is replaced silently.
fn request_duplication(
    database: &DatabaseContext,
    document: &Document,
    navigator: dioxus_router::Navigator,
    mut confirmation: Signal<bool>,
    mut error: Signal<Option<String>>,
) {
    error.set(None);
    match filled_draft_exists(database) {
        Ok(true) => confirmation.set(true),
        Ok(false) => match persist_duplication(database, document) {
            Ok(()) => {
                navigator.push(Route::Form {});
            }
            Err(message) => error.set(Some(message)),
        },
        Err(message) => error.set(Some(message)),
    }
}

fn load_current_draft(database: &DatabaseContext) -> Result<Option<DocumentInput>, String> {
    let database = database.as_ref().map_err(Clone::clone)?;
    let connection = database
        .lock()
        .map_err(|_| "Impossible d’accéder aux données locales.".to_string())?;
    load_draft(&connection).map_err(|error| {
        eprintln!("Draft query failed: {error}");
        "Impossible de vérifier le brouillon.".to_string()
    })
}

fn draft_exists(database: &DatabaseContext) -> Result<bool, String> {
    load_current_draft(database).map(|draft| draft.is_some())
}

fn filled_draft_exists(database: &DatabaseContext) -> Result<bool, String> {
    load_current_draft(database).map(|draft| draft.is_some_and(|input| !input.is_blank()))
}

/// Writes a pre-filled `input` into the single draft slot; the form loads it
/// from there, `Route::Form` has no parameter (home pattern). Shared by the
/// conversion (task 23) and the duplication (task 24).
fn persist_prefilled_draft(
    database: &DatabaseContext,
    input: &DocumentInput,
    log_context: &'static str,
    error_message: &'static str,
) -> Result<(), String> {
    let database = database.as_ref().map_err(Clone::clone)?;
    let connection = database
        .lock()
        .map_err(|_| "Impossible d’accéder aux données locales.".to_string())?;
    save_draft(&connection, input, &Utc::now().to_rfc3339()).map_err(|error| {
        eprintln!("{log_context} draft save failed: {error}");
        error_message.to_string()
    })
}

/// Writes the pre-filled invoice (deep copy of the quote, dated today — the
/// gérante adjusts it in the form) as the draft.
fn persist_conversion(database: &DatabaseContext, quote: &Document) -> Result<(), String> {
    let input = invoice_draft_from_quote(quote, &Utc::now().format("%Y-%m-%d").to_string());
    persist_prefilled_draft(
        database,
        &input,
        "Conversion",
        "Impossible de préparer la facture.",
    )
}

/// Writes the duplicate (deep copy of the document re-dated today, without
/// number or `source_quote_id` — no link kept with the original) as the draft.
fn persist_duplication(database: &DatabaseContext, document: &Document) -> Result<(), String> {
    let input = duplicate_draft_from_document(document, &Utc::now().format("%Y-%m-%d").to_string());
    persist_prefilled_draft(
        database,
        &input,
        "Duplication",
        "Impossible de préparer la copie.",
    )
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
        db::{get_document, issue_document, load_draft, open_database, save_draft},
        models::{ClientInput, ClientKind, DocumentInput, DocumentKind, LineInput},
    };

    use super::{
        DatabaseContext, RecordError, convert_action_visible, draft_exists, filled_draft_exists,
        load_record, persist_conversion, persist_duplication,
    };

    fn temp_connection() -> (NamedTempFile, Mutex<Connection>) {
        let file = NamedTempFile::new().expect("temp database file");
        let connection = open_database(file.path()).expect("open temp database");
        (file, connection)
    }

    fn temp_context() -> (NamedTempFile, DatabaseContext) {
        let (file, connection) = temp_connection();
        (file, Ok(std::sync::Arc::new(connection)))
    }

    fn lock(database: &DatabaseContext) -> std::sync::MutexGuard<'_, Connection> {
        database
            .as_ref()
            .expect("database")
            .lock()
            .expect("lock database")
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
            source_quote_id: None,
        }
    }

    #[test]
    fn an_issued_quote_loads_without_a_source_reference() {
        let (_file, database) = temp_connection();
        let mut connection = database.lock().expect("lock database");
        let quote = issue_document(
            &mut connection,
            sample_input(DocumentKind::Quote),
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
            "2026-07-24T10:00:00Z",
        )
        .expect("issue quote");
        let mut invoice_input = sample_input(DocumentKind::Invoice);
        invoice_input.source_quote_id = Some(quote.id);
        let invoice = issue_document(&mut connection, invoice_input, "2026-07-25T10:00:00Z")
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

    #[test]
    fn a_conversion_writes_a_prefilled_invoice_draft() {
        let (_file, database) = temp_context();
        let quote = issue_document(
            &mut lock(&database),
            sample_input(DocumentKind::Quote),
            "2026-07-24T10:00:00Z",
        )
        .expect("issue quote");

        persist_conversion(&database, &quote).expect("persist conversion");

        let draft = load_draft(&lock(&database))
            .expect("load draft")
            .expect("conversion draft present");
        assert_eq!(draft.kind, DocumentKind::Invoice);
        assert_eq!(draft.source_quote_id, Some(quote.id));
        assert_eq!(draft.client, quote.input.client);
        assert_eq!(draft.lines, quote.input.lines);
        assert_eq!(draft.event_date, quote.input.event_date);
        assert!(
            chrono::NaiveDate::parse_from_str(&draft.issue_date, "%Y-%m-%d").is_ok(),
            "the invoice draft carries a valid ISO issue date: {}",
            draft.issue_date
        );
    }

    #[test]
    fn draft_exists_tracks_the_draft_slot() {
        let (_file, database) = temp_context();
        assert_eq!(draft_exists(&database), Ok(false));

        save_draft(
            &lock(&database),
            &sample_input(DocumentKind::Quote),
            "2026-07-24T09:00:00Z",
        )
        .expect("seed draft");
        assert_eq!(draft_exists(&database), Ok(true));

        let broken: DatabaseContext = Err("base indisponible".to_string());
        assert!(draft_exists(&broken).is_err());
    }

    #[test]
    fn a_confirmed_conversion_replaces_the_existing_draft() {
        let (_file, database) = temp_context();
        save_draft(
            &lock(&database),
            &sample_input(DocumentKind::Invoice),
            "2026-07-24T09:00:00Z",
        )
        .expect("seed unrelated draft");
        let quote = issue_document(
            &mut lock(&database),
            sample_input(DocumentKind::Quote),
            "2026-07-24T10:00:00Z",
        )
        .expect("issue quote");

        persist_conversion(&database, &quote).expect("replace draft");

        let draft = load_draft(&lock(&database))
            .expect("load draft")
            .expect("conversion draft present");
        assert_eq!(draft.kind, DocumentKind::Invoice);
        assert_eq!(draft.source_quote_id, Some(quote.id));
    }

    #[test]
    fn a_duplication_writes_a_copy_draft_without_a_source_link() {
        let (_file, database) = temp_context();
        let quote = issue_document(
            &mut lock(&database),
            sample_input(DocumentKind::Quote),
            "2026-07-24T10:00:00Z",
        )
        .expect("issue quote");

        persist_duplication(&database, &quote).expect("persist duplication");

        let draft = load_draft(&lock(&database))
            .expect("load draft")
            .expect("duplication draft present");
        assert_eq!(draft.kind, DocumentKind::Quote);
        assert_eq!(draft.source_quote_id, None);
        assert_eq!(draft.client, quote.input.client);
        assert_eq!(draft.lines, quote.input.lines);
        assert_eq!(draft.payment_terms, quote.input.payment_terms);
        // Both dates are reset to the duplication day (the original's
        // 2026-07-24 / 2026-08-02 differ, so equal valid dates prove it).
        assert_eq!(draft.issue_date, draft.event_date);
        assert!(
            chrono::NaiveDate::parse_from_str(&draft.issue_date, "%Y-%m-%d").is_ok(),
            "the copy carries a valid ISO issue date: {}",
            draft.issue_date
        );
    }

    #[test]
    fn duplicating_quote_10_issues_quote_11_and_leaves_quote_10_unchanged() {
        let (_file, database) = temp_context();
        let quote = issue_document(
            &mut lock(&database),
            sample_input(DocumentKind::Quote),
            "2026-07-24T10:00:00Z",
        )
        .expect("issue quote");
        assert_eq!(quote.number, 10, "quote numbering starts at n° 10");

        persist_duplication(&database, &quote).expect("persist duplication");
        let draft = load_draft(&lock(&database))
            .expect("load draft")
            .expect("duplication draft present");
        let copy = issue_document(&mut lock(&database), draft, "2026-07-25T10:00:00Z")
            .expect("issue the duplicated draft");

        assert_eq!(copy.number, quote.number + 1);
        let original = get_document(&lock(&database), quote.id).expect("reload original");
        assert_eq!(original.number, 10);
        assert_eq!(original.input, quote.input);
        assert_eq!(original.created_at, quote.created_at);
    }

    #[test]
    fn a_duplicated_invoice_marks_nothing_invoiced() {
        let (_file, database) = temp_context();
        let quote = issue_document(
            &mut lock(&database),
            sample_input(DocumentKind::Quote),
            "2026-07-24T10:00:00Z",
        )
        .expect("issue quote");
        let mut invoice_input = sample_input(DocumentKind::Invoice);
        invoice_input.source_quote_id = Some(quote.id);
        let invoice = issue_document(&mut lock(&database), invoice_input, "2026-07-25T10:00:00Z")
            .expect("issue conversion invoice");

        persist_duplication(&database, &invoice).expect("persist duplication");

        let draft = load_draft(&lock(&database))
            .expect("load draft")
            .expect("duplication draft present");
        assert_eq!(draft.source_quote_id, None);
        // A copy that had kept the source link would be refused here (« Ce
        // devis a déjà été converti en facture. ») since the quote is
        // already invoiced — issuing it proves the link is gone.
        let copy = issue_document(&mut lock(&database), draft, "2026-07-26T10:00:00Z")
            .expect("issue the duplicated invoice");
        assert_eq!(copy.source_quote_id, None);
        let copy_record = load_record(&lock(&database), copy.id).expect("copy record");
        assert_eq!(copy_record.source_quote_number, None);
    }

    #[test]
    fn filled_draft_exists_ignores_a_blank_draft() {
        let (_file, database) = temp_context();
        assert_eq!(filled_draft_exists(&database), Ok(false));

        let mut blank = sample_input(DocumentKind::Quote);
        blank.issue_date = String::new();
        blank.event_date = String::new();
        blank.client.name = String::new();
        blank.client.address = String::new();
        blank.lines.clear();
        save_draft(&lock(&database), &blank, "2026-07-24T09:00:00Z").expect("seed blank draft");
        assert_eq!(filled_draft_exists(&database), Ok(false));

        save_draft(
            &lock(&database),
            &sample_input(DocumentKind::Quote),
            "2026-07-24T09:05:00Z",
        )
        .expect("seed filled draft");
        assert_eq!(filled_draft_exists(&database), Ok(true));

        let broken: DatabaseContext = Err("base indisponible".to_string());
        assert!(filled_draft_exists(&broken).is_err());
    }

    #[test]
    fn filled_draft_exists_treats_an_entered_date_as_content() {
        let (_file, database) = temp_context();
        let mut dates_only = sample_input(DocumentKind::Quote);
        dates_only.client.name = String::new();
        dates_only.client.address = String::new();
        dates_only.lines.clear();
        save_draft(&lock(&database), &dates_only, "2026-07-24T09:00:00Z")
            .expect("seed dates-only draft");

        assert_eq!(filled_draft_exists(&database), Ok(true));
    }

    #[test]
    fn a_confirmed_duplication_replaces_the_existing_draft() {
        let (_file, database) = temp_context();
        save_draft(
            &lock(&database),
            &sample_input(DocumentKind::Invoice),
            "2026-07-24T09:00:00Z",
        )
        .expect("seed unrelated draft");
        let quote = issue_document(
            &mut lock(&database),
            sample_input(DocumentKind::Quote),
            "2026-07-24T10:00:00Z",
        )
        .expect("issue quote");

        persist_duplication(&database, &quote).expect("replace draft");

        let draft = load_draft(&lock(&database))
            .expect("load draft")
            .expect("duplication draft present");
        assert_eq!(draft.kind, DocumentKind::Quote);
        assert_eq!(draft.source_quote_id, None);
        assert_eq!(draft.client, quote.input.client);
    }
}
