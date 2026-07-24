//! End-to-end issue flow (ARCHI §4) shared by the form's and the draft
//! preview's « Émettre » buttons: validate → issue transactionally (number +
//! insert, committed) → clear the draft → export PDF/PNG. Emission and export
//! are decoupled: a failed export never rolls the number back, the fiche
//! offers a re-export instead. The blocking work runs on a worker thread
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

/// Starts the whole chain from an « Émettre » tap. The phase itself guards
/// the double-tap: a second call while `Running` returns immediately (the
/// button is also disabled by its loading state).
pub(super) fn start_issue(mut flow: IssueFlow, database: DatabaseContext, input: DocumentInput) {
    if matches!(&*flow.0.read(), IssuePhase::Running) {
        return;
    }
    flow.0.set(IssuePhase::Running);
    std::thread::spawn(move || {
        let phase = match catch_unwind(AssertUnwindSafe(|| issue_draft(&database, &input))) {
            Err(payload) => {
                eprintln!("Issue chain panicked: {payload:?}");
                IssuePhase::Failed(
                    "Échec inattendu de l'émission (détail dans les logs).".to_string(),
                )
            }
            Ok(Err(IssueFailure::Invalid(errors))) => IssuePhase::Invalid(errors),
            Ok(Err(IssueFailure::Failed(message))) => IssuePhase::Failed(message),
            Ok(Ok(document)) => {
                let (export, _) = run_export(&document);
                let notice = Some(issued_notice(&document));
                IssuePhase::Issued(Box::new(IssuedState {
                    document,
                    export,
                    notice,
                }))
            }
        };
        write_from_worker(flow.0, |current| *current = phase);
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
/// (the aperçu's « Exporter » remains the standing re-export path).
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
    let document = issue_document(&mut connection, input.clone(), None, &now).map_err(|error| {
        match error {
            // The input was just validated with the structured validator;
            // reaching this arm means the two drifted apart — surface the
            // messages as-is rather than dropping them.
            IssueError::Validation(messages) => IssueFailure::Failed(messages.join("\n")),
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
/// read borrow: retry while the contention can be transient. The loop is
/// generous (100 × 50 ms) because dropping the result would strand the flow
/// in `Running` — the double-tap guard would then block every later attempt.
pub(super) fn write_from_worker<T: Send + Sync + 'static>(
    mut signal: Signal<T, SyncStorage>,
    update: impl FnOnce(&mut T),
) {
    for _ in 0..100 {
        if let Ok(mut guard) = signal.try_write() {
            update(&mut guard);
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    eprintln!("Worker result dropped after 5 s of UI contention");
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    use super::{
        DatabaseContext, DocumentField, IssueFailure, IssuePhase, blocks_draft_persistence,
        field_error, issue_draft, issued_notice, line_has_error,
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
            list_documents(&connection, None)
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
    fn field_error_returns_the_first_message_of_that_field_only() {
        let errors = vec![
            FieldError {
                field: DocumentField::ClientName,
                message: "Le nom du client est obligatoire.".to_string(),
            },
            FieldError {
                field: DocumentField::ClientAddress,
                message: "L'adresse du client est obligatoire.".to_string(),
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
}
