//! End-to-end issue flow (ARCHI §4) shared by the form's and the draft
//! preview's « Émettre » buttons: validate → issue transactionally (number +
//! insert, committed) → clear the draft → publish the fiche → export PDF/PNG.
//! Emission and export are decoupled in both directions: a failed export never
//! rolls the number back (« Partager » regenerates the missing files), and a running
//! export never holds the fiche back — she reaches her document as soon as it
//! exists, not when its files do. The blocking work runs on a worker thread
//! (Typst compile takes ~1 s) and publishes its phases on a sync signal
//! provided at the app root; `AppShell` turns them into navigation.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use dioxus::prelude::*;

use crate::domain::{
    db::{IssueError, clear_draft, issue_document},
    models::{Document, DocumentInput},
    validation::{DocumentField, FieldError, validate_document_fields},
};
use crate::platform::export::{DocumentExport, export_document};

use super::app::DatabaseContext;

/// App-wide issue flow state, provided by `app()` and consumed by the form
/// (errors + loading), the preview (loading) and the fiche (notice + export
/// retry). `SyncStorage` so the worker thread can publish its result.
#[derive(Clone, Copy)]
pub(super) struct IssueFlow(pub Signal<IssuePhase, SyncStorage>);

#[derive(Debug, Clone, PartialEq)]
pub(super) enum IssuePhase {
    Idle,
    Running,
    /// Validation failed: persistent block in the form, faulty fields flagged.
    Invalid(Vec<FieldError>),
    /// The chain broke before or during emission (no document was created).
    Failed(String),
    Issued(Box<IssuedState>),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct IssuedState {
    pub document: Document,
    pub export: ExportPhase,
    /// Transient snackbar text (« Devis n° 10 émis », re-export result).
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExportPhase {
    Running,
    Done,
    Failed,
}

/// Why the chain stopped before producing a document.
#[derive(Debug, PartialEq)]
pub(super) enum IssueFailure {
    Invalid(Vec<FieldError>),
    Failed(String),
}

/// Gate in front of the confirmation sheet: `true` when the draft is ready to
/// be issued, otherwise the validation errors are published and the caller must
/// not ask anything.
///
/// The order matters more than it looks. Issuing is the one irreversible act in
/// the app, and the sheet names the number it is about to spend. Showing that
/// sheet for a document the system already knows is invalid asks her to endorse
/// something that will not happen — and the sheet then closes on a screen that
/// looks unchanged, leaving her unable to tell whether the number was consumed.
/// It never is (`peek_next_number` only reads the counter), but the screen said
/// nothing. Validating first turns that dead end into the ordinary error path.
pub(super) fn check_before_issue(mut flow: IssueFlow, input: &DocumentInput) -> bool {
    let errors = validate_document_fields(input);
    if errors.is_empty() {
        return true;
    }
    flow.0.set(IssuePhase::Invalid(errors));
    false
}

/// Starts the whole chain from an « Émettre » tap. The phase itself guards
/// the double-tap: a second call while `Running` returns immediately (the
/// button is also disabled by its loading state). Validation runs again here:
/// this is the authoritative check, `check_before_issue` is the early one.
pub(super) fn start_issue(mut flow: IssueFlow, database: DatabaseContext, input: DocumentInput) {
    if matches!(&*flow.0.read(), IssuePhase::Running) {
        return;
    }
    flow.0.set(IssuePhase::Running);
    std::thread::spawn(move || {
        let outcome = match catch_unwind(AssertUnwindSafe(|| issue_draft(&database, &input))) {
            Err(payload) => {
                eprintln!("Issue chain panicked: {payload:?}");
                Err(IssuePhase::Failed(
                    "Échec inattendu de l'émission (détail dans les logs).".to_string(),
                ))
            }
            Ok(Err(IssueFailure::Invalid(errors))) => Err(IssuePhase::Invalid(errors)),
            Ok(Err(IssueFailure::Failed(message))) => Err(IssuePhase::Failed(message)),
            Ok(Ok(document)) => Ok(document),
        };
        let document = match outcome {
            Ok(document) => document,
            Err(phase) => {
                write_from_worker(flow.0, |current| *current = phase);
                return;
            }
        };
        // The number is spent and the draft is cleared the moment `issue_draft`
        // returns, so the fiche is published here rather than after the ~1 s
        // Typst compile. Waiting left her on a form whose draft no longer
        // existed, with a button spinner for company, at the one moment in the
        // app that cannot be undone — long enough to read as a hang, and a
        // force-close there hides an emission that already happened. It also
        // delivered « Devis n° 10 émis » and « PDF non généré » in the same
        // frame. The export now runs behind the fiche, which has rendered
        // `ExportPhase::Running` since it was written for this (`record.rs`);
        // navigation keys on the document id, so the later result mutates the
        // live phase instead of routing again (`app.rs`).
        let published = document.clone();
        let notice = Some(issued_notice(&document));
        write_from_worker(flow.0, |current| {
            *current = IssuePhase::Issued(Box::new(IssuedState {
                document: published,
                export: ExportPhase::Running,
                notice,
            }));
        });
        let (export, _) = run_export(&document);
        update_issued(flow, document.id, |state| {
            state.export = export;
        });
    });
}

/// Re-export from the fiche after a failed export: regenerates the missing
/// files (`export_document` keeps existing ones — ARCHI §4) and confirms with
/// a snackbar. Only meaningful while an `Issued` phase is live.
pub(super) fn retry_export(flow: IssueFlow) {
    let document_id = match &*flow.0.read() {
        IssuePhase::Issued(state) if state.export != ExportPhase::Running => state.document.id,
        _ => return,
    };
    update_issued(flow, document_id, |state| {
        state.export = ExportPhase::Running;
    });
    std::thread::spawn(move || {
        let document = match &*flow.0.read() {
            IssuePhase::Issued(state) if state.document.id == document_id => state.document.clone(),
            _ => return,
        };
        let (export, paths) = run_export(&document);
        let notice = paths.map(|paths| format!("Export terminé : {}", paths.files_label()));
        update_issued(flow, document_id, |state| {
            state.export = export;
            if let Some(notice) = notice {
                state.notice = Some(notice);
            }
        });
    });
}

/// Dismisses the fiche snackbar, but only if it still shows the notice the
/// caller scheduled its timer for — a newer notice (retry result, newer
/// issuance) must survive an older timer (UI thread — writes happen between
/// renders there, like any event handler, so no retry loop is needed).
pub(super) fn dismiss_notice(mut flow: IssueFlow, expected: &str) {
    if let IssuePhase::Issued(state) = &mut *flow.0.write()
        && state.notice.as_deref() == Some(expected)
    {
        state.notice = None;
    }
}

/// While the chain runs (or just completed), persisting the draft is
/// forbidden: the worker clears it right after commit, and a late autosave
/// would resurrect the just-issued draft — offered again, issued twice.
pub(super) fn blocks_draft_persistence(phase: &IssuePhase) -> bool {
    matches!(phase, IssuePhase::Running | IssuePhase::Issued(_))
}

/// Leaves the issue flow once the fiche is closed: the snackbar and the
/// re-export block belong to the post-emission moment, not to later visits
/// (a manual re-export stays available on the fiche and the aperçu).
pub(super) fn reset_issue_flow(mut flow: IssueFlow) {
    if matches!(&*flow.0.read(), IssuePhase::Issued(_)) {
        flow.0.set(IssuePhase::Idle);
    }
}

/// Validate → issue (one transaction) → clear the draft, exactly in the
/// ARCHI §4 order: the draft is cleared after the emission commits, never
/// after the export. A failed draft cleanup is only logged — the document is
/// committed and must not be reported as a failure.
pub(super) fn issue_draft(
    database: &DatabaseContext,
    input: &DocumentInput,
) -> Result<Document, IssueFailure> {
    let errors = validate_document_fields(input);
    if !errors.is_empty() {
        return Err(IssueFailure::Invalid(errors));
    }
    let database = database
        .as_ref()
        .map_err(|message| IssueFailure::Failed(message.clone()))?;
    let mut connection = database.lock().map_err(|_| {
        IssueFailure::Failed("Impossible d'accéder aux données locales.".to_string())
    })?;
    let now = chrono::Utc::now().to_rfc3339();
    let document = issue_document(&mut connection, input.clone(), &now).map_err(|error| {
        match error {
            // The input was just validated with the structured validator;
            // reaching this arm means the two drifted apart — surface the
            // messages as-is rather than dropping them.
            IssueError::Validation(messages) => IssueFailure::Failed(messages.join("\n")),
            // Stale conversion state (the quote was invoiced from another
            // path since the draft was loaded): the domain message is
            // user-ready French, show it as-is.
            IssueError::QuoteAlreadyInvoiced => {
                IssueFailure::Failed(IssueError::QuoteAlreadyInvoiced.to_string())
            }
            IssueError::Database(error) => {
                eprintln!("issue_document failed: {error}");
                IssueFailure::Failed("Impossible d'émettre le document.".to_string())
            }
        }
    })?;
    if let Err(error) = clear_draft(&connection) {
        eprintln!("Draft cleanup failed after issue #{}: {error}", document.id);
    }
    Ok(document)
}

/// Post-commit export with panic containment: the document stays issued
/// whatever happens here. Returns the phase and, on success, the written
/// paths (the retry snackbar names them).
fn run_export(document: &Document) -> (ExportPhase, Option<DocumentExport>) {
    match catch_unwind(AssertUnwindSafe(|| {
        export_document(&document.input, document.number)
    })) {
        Ok(Ok(paths)) => (ExportPhase::Done, Some(paths)),
        Ok(Err(error)) => {
            eprintln!("Export failed for document #{}: {error}", document.id);
            (ExportPhase::Failed, None)
        }
        Err(payload) => {
            eprintln!("Export panicked for document #{}: {payload:?}", document.id);
            (ExportPhase::Failed, None)
        }
    }
}

/// Snackbar confirmation right after the fiche appears (« Devis n° 10 émis »).
fn issued_notice(document: &Document) -> String {
    let participle = match document.input.kind {
        crate::domain::models::DocumentKind::Quote => "émis",
        crate::domain::models::DocumentKind::Invoice => "émise",
    };
    format!(
        "{} n° {} {participle}",
        document.input.kind.label(),
        document.number
    )
}

/// First validation message for one field, for that input's own error slot.
pub(super) fn field_error(errors: &[FieldError], field: DocumentField) -> Option<String> {
    errors
        .iter()
        .find(|error| error.field == field)
        .map(|error| error.message.clone())
}

/// True when any error targets line `index` (designation, quantity or price),
/// so the faulty rows stand out under the aggregated block.
pub(super) fn line_has_error(errors: &[FieldError], index: usize) -> bool {
    errors
        .iter()
        .any(|error| error.field.line_index() == Some(index))
}

/// DOM anchor for a validation error, so the screen can take her to the field.
///
/// The match is exhaustive on purpose: a new `DocumentField` cannot ship
/// without an anchor, and `every_validation_error_has_an_anchor_the_form_
/// actually_renders` checks that the anchors still name ids the form emits.
pub(super) fn error_anchor(field: &DocumentField) -> String {
    match field {
        // `OutlinedField` builds its id as `field-{name}` (`components/fields.rs`).
        DocumentField::ClientName => "field-client-name".to_string(),
        DocumentField::ClientAddress => "field-client-address".to_string(),
        DocumentField::IssueDate => "field-issue-date".to_string(),
        DocumentField::EventDate => "field-event-date".to_string(),
        DocumentField::PaymentTerms => "field-payment-terms".to_string(),
        DocumentField::Lines => "form-lines-title".to_string(),
        DocumentField::LineDescription(index)
        | DocumentField::LineQuantity(index)
        | DocumentField::LinePrice(index) => format!("form-line-{index}"),
        DocumentField::Total => "form-total".to_string(),
    }
}

/// Brings the first faulty field into view and gives it focus.
///
/// Publishing the errors is not the same as showing them. On a five-section
/// form she is at the bottom when she taps « Émettre » — the aggregated block
/// renders above the action bar and the faulty fields sit further up still, so
/// nothing moves and the tap reads as a broken app. She then taps again.
///
/// The scroll is instant rather than smooth: DESIGN.md §7 keeps motion for
/// state, and an error path is the last place to make her wait for a camera
/// move. Anchors that cannot take focus (the lines heading, the total) simply
/// scroll; their message carries `role="alert"` and is announced anyway.
/// « First » is the first on the page, not the first in the vector.
/// `validate_document_fields` publishes in the order the record is stored —
/// dates, then payment terms, then the client — while the form reads Client,
/// Dates, Prestations, Conditions. Taking `errors.first()` sent her to « Date
/// de l'événement » past the two client errors sitting above it, which is the
/// opposite of what this function is for. Asking the DOM keeps the two orders
/// independent: neither the validation nor the form has to know about the
/// other's layout.
pub(super) fn reveal_first_error(errors: &[FieldError]) {
    if let Some(script) = reveal_script(errors) {
        let _ = dioxus::document::eval(&script);
    }
}

/// The script itself, so the anchor list can be asserted without a DOM.
fn reveal_script(errors: &[FieldError]) -> Option<String> {
    if errors.is_empty() {
        return None;
    }
    // Built from the enum and a line index, never from typed text, so nothing
    // of hers can reach the script.
    let anchors = errors
        .iter()
        .map(|error| format!("'{}'", error_anchor(&error.field)))
        .collect::<Vec<_>>()
        .join(",");
    Some(format!(
        "const targets = [{anchors}]
             .map(id => document.getElementById(id))
             .filter(Boolean);
         if (targets.length) {{
             const target = targets.reduce((first, other) =>
                 first.compareDocumentPosition(other)
                     & Node.DOCUMENT_POSITION_PRECEDING ? other : first);
             target.scrollIntoView({{ block: 'center' }});
             target.focus({{ preventScroll: true }});
         }}"
    ))
}

fn update_issued(flow: IssueFlow, document_id: i64, update: impl FnOnce(&mut IssuedState)) {
    write_from_worker(flow.0, |current| {
        if let IssuePhase::Issued(state) = current {
            // A retry result landing after a newer emission must not stamp
            // the previous document's outcome onto the new fiche.
            if state.document.id == document_id {
                update(state);
            }
        }
    });
}

/// Writes from a worker thread, where the write can race a render holding a
/// read borrow. The write is retried until it lands — a lost terminal state
/// would strand the UI in `Running` with no recovery — unless the screen's
/// scope is gone: then the result targets nothing and is discarded, loudly.
/// Contention warnings start after 5 s so a stuck borrow shows in the logs
/// instead of silently leaking the worker.
pub(super) fn write_from_worker<T: Send + Sync + 'static>(
    mut signal: Signal<T, SyncStorage>,
    update: impl FnOnce(&mut T),
) {
    for attempt in 0u32.. {
        match signal.try_write() {
            Ok(mut guard) => {
                update(&mut guard);
                return;
            }
            Err(BorrowMutError::Dropped(error)) => {
                eprintln!("Worker result discarded, its screen is gone: {error}");
                return;
            }
            Err(_) => {
                if attempt > 0 && attempt % 100 == 0 {
                    eprintln!(
                        "Worker result still queued after {} s of UI contention",
                        attempt / 20
                    );
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    use super::{
        DatabaseContext, DocumentField, IssueFailure, IssuePhase, blocks_draft_persistence,
        error_anchor, field_error, issue_draft, issued_notice, line_has_error, reveal_script,
    };
    use crate::domain::{
        db::{get_document, list_documents, load_draft, open_database, save_draft},
        models::{ClientInput, ClientKind, DocumentInput, DocumentKind, LineInput},
        numbering::next_number,
        validation::FieldError,
    };

    fn temp_database() -> (NamedTempFile, DatabaseContext) {
        let file = NamedTempFile::new().expect("temp database file");
        let connection = open_database(file.path()).expect("open temp database");
        (file, Ok(std::sync::Arc::new(connection)))
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

    fn lock(database: &DatabaseContext) -> std::sync::MutexGuard<'_, Connection> {
        database
            .as_ref()
            .expect("database")
            .lock()
            .expect("lock database")
    }

    #[test]
    fn issue_draft_commits_the_number_and_clears_the_draft() {
        let (_file, database) = temp_database();
        let input = sample_input(DocumentKind::Quote);
        save_draft(&lock(&database), &input, "2026-07-24T09:00:00Z").expect("seed draft");

        let document = issue_draft(&database, &input).expect("issue document");

        assert_eq!(document.number, 10, "quote counter starts at 10");
        let connection = lock(&database);
        assert!(load_draft(&connection).expect("load draft").is_none());
        let stored = get_document(&connection, document.id).expect("stored document");
        assert_eq!(stored.number, 10);
        assert_eq!(stored.input, input);
    }

    #[test]
    fn issue_draft_rejects_an_invalid_input_without_touching_anything() {
        let (_file, database) = temp_database();
        let mut input = sample_input(DocumentKind::Quote);
        input.client.name.clear();
        save_draft(&lock(&database), &input, "2026-07-24T09:00:00Z").expect("seed draft");

        let IssueFailure::Invalid(errors) =
            issue_draft(&database, &input).expect_err("invalid input must be rejected")
        else {
            panic!("expected validation errors");
        };

        assert!(
            errors
                .iter()
                .any(|error| error.field == DocumentField::ClientName)
        );
        let connection = lock(&database);
        assert!(
            load_draft(&connection).expect("load draft").is_some(),
            "the draft must survive a refused emission"
        );
        assert!(
            list_documents(&connection, None, None)
                .expect("list documents")
                .is_empty(),
            "no document may be created"
        );
        assert_eq!(
            next_number(&connection, &DocumentKind::Quote).expect("next number"),
            10,
            "no number may be consumed"
        );
    }

    #[test]
    fn issued_notice_speaks_french_for_both_kinds() {
        let (_file, database) = temp_database();
        let quote =
            issue_draft(&database, &sample_input(DocumentKind::Quote)).expect("issue quote");
        assert_eq!(issued_notice(&quote), "Devis n° 10 émis");

        let invoice =
            issue_draft(&database, &sample_input(DocumentKind::Invoice)).expect("issue invoice");
        assert_eq!(issued_notice(&invoice), "Facture n° 1 émise");
    }

    #[test]
    fn issue_draft_carries_the_source_quote_id_through_to_the_emission() {
        let (_file, database) = temp_database();
        let quote =
            issue_draft(&database, &sample_input(DocumentKind::Quote)).expect("issue quote");

        let mut conversion = sample_input(DocumentKind::Invoice);
        conversion.source_quote_id = Some(quote.id);
        let invoice = issue_draft(&database, &conversion).expect("issue conversion");

        assert_eq!(invoice.source_quote_id, Some(quote.id));
        let reloaded = get_document(&lock(&database), quote.id).expect("reload quote");
        assert!(reloaded.is_invoiced, "the quote reads back as facturé");
    }

    #[test]
    fn issue_draft_refuses_a_stale_second_conversion_in_french() {
        let (_file, database) = temp_database();
        let quote =
            issue_draft(&database, &sample_input(DocumentKind::Quote)).expect("issue quote");
        let mut conversion = sample_input(DocumentKind::Invoice);
        conversion.source_quote_id = Some(quote.id);
        issue_draft(&database, &conversion).expect("issue first conversion");

        let mut stale = sample_input(DocumentKind::Invoice);
        stale.source_quote_id = Some(quote.id);
        let error = issue_draft(&database, &stale).expect_err("refuse stale conversion");

        assert_eq!(
            error,
            IssueFailure::Failed("Ce devis a déjà été converti en facture.".to_string())
        );
    }

    #[test]
    fn field_error_returns_the_first_message_of_that_field_only() {
        let errors = vec![
            FieldError {
                field: DocumentField::ClientName,
                message: "Le nom du client est obligatoire.".to_string(),
            },
            FieldError {
                field: DocumentField::EventDate,
                message: "La date de l'événement est obligatoire.".to_string(),
            },
        ];

        assert_eq!(
            field_error(&errors, DocumentField::ClientName).as_deref(),
            Some("Le nom du client est obligatoire.")
        );
        assert_eq!(field_error(&errors, DocumentField::IssueDate), None);
    }

    #[test]
    fn line_has_error_matches_any_slot_of_that_line() {
        let errors = vec![
            FieldError {
                field: DocumentField::LineQuantity(1),
                message: "Ligne 2: la quantité doit être positive.".to_string(),
            },
            FieldError {
                field: DocumentField::ClientName,
                message: "Le nom du client est obligatoire.".to_string(),
            },
        ];

        assert!(!line_has_error(&errors, 0));
        assert!(line_has_error(&errors, 1));
        assert!(!line_has_error(&errors, 2));
    }

    #[test]
    fn draft_persistence_is_blocked_only_while_the_chain_runs_or_just_committed() {
        assert!(!blocks_draft_persistence(&IssuePhase::Idle));
        assert!(blocks_draft_persistence(&IssuePhase::Running));
        assert!(!blocks_draft_persistence(&IssuePhase::Invalid(Vec::new())));
        assert!(!blocks_draft_persistence(&IssuePhase::Failed(
            "erreur".to_string()
        )));

        let (_file, database) = temp_database();
        let document =
            issue_draft(&database, &sample_input(DocumentKind::Quote)).expect("issue quote");
        let issued = IssuePhase::Issued(Box::new(super::IssuedState {
            document,
            export: super::ExportPhase::Running,
            notice: None,
        }));
        assert!(blocks_draft_persistence(&issued));
    }

    #[test]
    fn every_field_error_anchors_to_its_own_control() {
        assert_eq!(
            error_anchor(&DocumentField::ClientName),
            "field-client-name"
        );
        assert_eq!(error_anchor(&DocumentField::IssueDate), "field-issue-date");
        assert_eq!(error_anchor(&DocumentField::Lines), "form-lines-title");
        assert_eq!(error_anchor(&DocumentField::LineQuantity(2)), "form-line-2");
        assert_eq!(error_anchor(&DocumentField::Total), "form-total");
    }

    /// An anchor naming an id the form no longer emits scrolls to nothing, and
    /// says nothing about it — the same shape as a rename that misses one file.
    /// So the mapping is checked against the source that renders it.
    #[test]
    fn every_validation_error_has_an_anchor_the_form_actually_renders() {
        const FORM: &str = include_str!("form.rs");
        const FIELDS: &str = include_str!("components/fields.rs");

        // Every `field-` anchor rests on this one format string.
        assert!(
            FIELDS.contains(r#"format!("field-{name}")"#),
            "OutlinedField no longer builds its id as `field-{{name}}`"
        );

        for field in [
            DocumentField::ClientName,
            DocumentField::ClientAddress,
            DocumentField::IssueDate,
            DocumentField::EventDate,
            DocumentField::PaymentTerms,
            DocumentField::Lines,
            DocumentField::LineDescription(0),
            DocumentField::LineQuantity(0),
            DocumentField::LinePrice(0),
            DocumentField::Total,
        ] {
            let anchor = error_anchor(&field);
            let needle = if let Some(name) = anchor.strip_prefix("field-") {
                // The form only names the field; `OutlinedField` prefixes it.
                format!("\"{name}\"")
            } else if field.line_index().is_some() {
                // Interpolated in the RSX, so match the literal as written.
                "form-line-{index}".to_string()
            } else {
                anchor.clone()
            };
            assert!(
                FORM.contains(&needle),
                "{anchor}: form.rs renders no `{needle}`"
            );
        }
    }

    /// The regression this guards: the script used to carry `errors.first()`
    /// alone. `validate_document_fields` publishes in storage order — the event
    /// date before the client — while the form reads Client first, so the one
    /// anchor it carried was the wrong one and focus jumped past the two client
    /// errors above it. Every anchor has to reach the page for the DOM to pick.
    #[test]
    fn the_reveal_script_carries_every_anchor_and_lets_the_dom_order_them() {
        let errors = vec![
            FieldError {
                field: DocumentField::EventDate,
                message: "date".to_string(),
            },
            FieldError {
                field: DocumentField::ClientName,
                message: "nom".to_string(),
            },
            FieldError {
                field: DocumentField::ClientAddress,
                message: "adresse".to_string(),
            },
        ];

        let script = reveal_script(&errors).expect("errors produce a script");

        for error in &errors {
            let anchor = error_anchor(&error.field);
            assert!(script.contains(&anchor), "{anchor} never reaches the page");
        }
        assert!(
            script.contains("compareDocumentPosition"),
            "the page order decides, not the vector order"
        );
    }

    #[test]
    fn nothing_is_revealed_without_an_error() {
        assert!(reveal_script(&[]).is_none());
    }
}
