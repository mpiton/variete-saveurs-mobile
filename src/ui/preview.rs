//! Full-screen document preview: the `render` HTML shown in an iframe
//! `srcdoc` — never the export pipeline (ARCHI §5) — with pinch-zoom, pan
//! and double-tap gestures driven by an injected script. Two sources: the
//! draft (next number peeked read-only, never reserved, plus a discreet
//! « aperçu » pill) and an issued document (re-rendered exactly from the
//! stored document).

use dioxus::prelude::*;
use rusqlite::Connection;

use crate::domain::{
    db::{get_document, load_draft},
    models::{DocumentInput, DocumentKind},
    numbering::next_number,
    render::render_document_html,
};
use crate::platform::export::count_pdf_pages;

use super::{
    app::{DatabaseContext, Route},
    components::{Button, ButtonVariant, ErrorBlock, IssueConfirmSheet, ShareSheet, issue_label},
    issue::{IssueFlow, IssuePhase, check_before_issue, start_issue, write_from_worker},
    share::{share_file_names, use_share_flow},
};

const PREVIEW_GESTURES: &str = include_str!("preview_gestures.js");

/// The page count, and which preview it belongs to.
///
/// The tag is the route's own `document` — `None` is the draft. Dioxus keeps a
/// component instance alive when only route parameters change, and the
/// `Routable` derive leaves nowhere to key `Preview` by its document, so an
/// untagged count could outlive the document it was computed for: shown under
/// the wrong quote, or written by a worker that finished after the screen had
/// moved on. No path pushes a preview from a preview today, so neither happens;
/// the tag is what keeps that true when one is added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PageCount {
    document: Option<i64>,
    pages: PageResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageResult {
    /// Nothing asked for yet — the state a fresh screen starts in.
    Unstarted,
    Pending,
    Known(usize),
    Unavailable,
}

/// Compiles the document on a worker to learn how many pages it really makes.
///
/// The preview and the file she sends are two renderers (ARCHI §5), and the
/// preview's continuous strip cannot predict the PDF's pagination: measured
/// over fourteen documents, the HTML height disagreed with the real layout
/// twice and in both directions, so no page height reconciles them. Drawing
/// page marks from the strip would have been wrong exactly at the boundary,
/// which is the only place the answer matters. The number comes from the
/// compiler that makes the PDF instead.
///
/// It runs on the draft preview because « émis = figé »: this is the last
/// screen where a quote that falls badly across pages can still be changed.
fn start_page_count(
    mut count: Signal<PageCount, SyncStorage>,
    document: Option<i64>,
    input: DocumentInput,
    number: i64,
) {
    // One worker per preview: already running or answered for this document.
    let current = *count.peek();
    if current.document == document && current.pages != PageResult::Unstarted {
        return;
    }
    count.set(PageCount {
        document,
        pages: PageResult::Pending,
    });
    std::thread::spawn(move || {
        let pages = match count_pdf_pages(&input, number) {
            Ok(pages) => PageResult::Known(pages),
            // Informational only. The export path reports its own failures in
            // French; a missing count is nothing she can act on, so it stays
            // quiet on screen and loud in the logs.
            Err(error) => {
                eprintln!("Preview page count failed: {error}");
                PageResult::Unavailable
            }
        };
        write_from_worker(count, move |current| {
            // A worker that lands after the screen moved on is answering about
            // a document nobody is looking at any more.
            if current.document == document {
                current.pages = pages;
            }
        });
    });
}

/// Silent until the worker answers, silent if it fails, and silent while it is
/// still about the previous document: an absent count says nothing false, where
/// a guessed or stale one would.
fn pages_label(count: PageCount, document: Option<i64>) -> Option<String> {
    if count.document != document {
        return None;
    }
    match count.pages {
        PageResult::Known(1) => Some("1 page".to_string()),
        PageResult::Known(pages) => Some(format!("{pages} pages")),
        PageResult::Unstarted | PageResult::Pending | PageResult::Unavailable => None,
    }
}

/// What the preview renders, resolved from the optional document id in the
/// route: no id = the draft, an id = the issued document stored under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PreviewSource {
    Draft,
    Issued,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct PreviewData {
    pub html: String,
    pub source: PreviewSource,
    pub kind: DocumentKind,
    pub number: i64,
    pub input: DocumentInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PreviewError {
    NoDraft,
    DocumentNotFound,
    Unavailable,
}

/// Load-and-render for the preview. The draft path peeks the next number
/// read-only: the reservation only happens at issuance (tasks 07/09), so
/// opening the preview never consumes a number.
pub(super) fn load_preview(
    connection: &Connection,
    document_id: Option<i64>,
) -> Result<PreviewData, PreviewError> {
    match document_id {
        Some(id) => {
            let document = get_document(connection, id).map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => PreviewError::DocumentNotFound,
                error => {
                    eprintln!("Preview document query failed: {error}");
                    PreviewError::Unavailable
                }
            })?;
            let html = render_document_html(&document.input, document.number);
            Ok(PreviewData {
                html,
                source: PreviewSource::Issued,
                kind: document.input.kind.clone(),
                number: document.number,
                input: document.input,
            })
        }
        None => {
            let input = load_draft(connection)
                .map_err(|error| {
                    eprintln!("Preview draft load failed: {error}");
                    PreviewError::Unavailable
                })?
                .ok_or(PreviewError::NoDraft)?;
            let number = next_number(connection, &input.kind).map_err(|error| {
                eprintln!("Preview number peek failed: {error}");
                PreviewError::Unavailable
            })?;
            let html = render_document_html(&input, number);
            Ok(PreviewData {
                html,
                source: PreviewSource::Draft,
                kind: input.kind.clone(),
                number,
                input,
            })
        }
    }
}

#[component]
pub(super) fn Preview(document: Option<i64>) -> Element {
    let database = use_context::<DatabaseContext>();
    let navigator = use_navigator();
    let share = use_share_flow();
    let page_count = use_signal_sync(|| PageCount {
        document,
        pages: PageResult::Unstarted,
    });

    // Loaded synchronously in the body; the phase signals only re-run the
    // query + render on their own transitions (identical output for a frozen
    // document, so the iframe and its zoom state are undisturbed).
    match load_from_context(&database, document) {
        Err(error) => {
            let (title, message) = error_message(error);
            rsx! {
                section { class: "preview-screen preview-screen--message",
                    div { class: "preview-message",
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
            let draft = data.source == PreviewSource::Draft;
            let (pdf_name, png_name) = share_file_names(&data.kind, data.number);
            let share_input = data.input.clone();
            let share_number = data.number;
            // Started from the body rather than from `onmounted`: a retained
            // instance keeps its elements mounted, so the mount handler would
            // never fire again for a document that just changed.
            start_page_count(page_count, document, data.input.clone(), data.number);
            let pages_label = pages_label(page_count(), document);
            rsx! {
                section { class: "preview-screen",
                    div {
                        class: "preview-viewport",
                        id: "preview-viewport",
                        onmounted: move |_| {
                            let _ = dioxus::document::eval(PREVIEW_GESTURES);
                        },
                        if draft {
                            p { class: "preview-pill", "Aperçu" }
                        }
                        // The exported PDF's real page count, from the compiler
                        // that produces it. The strip above has no pages of its
                        // own, and it cannot be made to predict these.
                        if let Some(label) = pages_label {
                            p {
                                class: "preview-pages",
                                role: "status",
                                aria_live: "polite",
                                "{label}"
                            }
                        }
                        div { class: "preview-stage", id: "preview-stage",
                            iframe {
                                class: "preview-frame",
                                id: "preview-frame",
                                title: "Document prévisualisé",
                                // No script ever runs inside the frame; the
                                // gesture script only reads its measurements
                                // from the parent (same-origin kept).
                                "sandbox": "allow-same-origin",
                                srcdoc: data.html.clone(),
                            }
                        }
                    }
                    if let Some(message) = share.error() {
                        ErrorBlock {
                            title: "Partage impossible".to_string(),
                            message,
                        }
                    }
                    footer { class: "chrome-action-bar preview-action-bar",
                        if draft {
                            IssueDraftButton {
                                kind: data.kind.clone(),
                                number: data.number,
                                input: data.input.clone(),
                            }
                        } else {
                            // « Exporter » used to sit here. It ran the first
                            // half of « Partager » — same files, same call —
                            // then left them in the app's private storage,
                            // where no file manager reaches them, and named
                            // them in a snackbar. Sharing to « Fichiers » is
                            // the path that actually lands a file on the phone.
                            Button {
                                label: "Partager".to_string(),
                                variant: ButtonVariant::Tonal,
                                onclick: move |_| share.open_sheet(),
                            }
                            // No « Envoyer » here: sending is the fiche's, and a
                            // permanently disabled button taught nothing.
                        }
                    }
                    ShareSheet {
                        state: share.state(),
                        pdf_name,
                        png_name,
                        on_pick: move |format| share.start(share_input.clone(), share_number, format),
                    }
                }
            }
        }
    }
}

/// The draft's « Émettre » button, isolated in its own component so the
/// preview body keeps its load-once property: an issue-phase change never
/// re-triggers the SQLite reload + HTML render above (the draft disappears
/// from under the screen on success — navigation moves to the fiche first).
#[component]
fn IssueDraftButton(kind: DocumentKind, number: i64, input: DocumentInput) -> Element {
    let database = use_context::<DatabaseContext>();
    let issue_flow = use_context::<IssueFlow>();
    let navigator = use_navigator();
    let issuing = matches!(&*issue_flow.0.read(), IssuePhase::Running);
    // The preview already shows the number on the sheet; the confirmation is
    // here for the same reason as on the form — the act is irreversible.
    let mut confirming = use_signal(|| false);
    rsx! {
        Button {
            label: issue_label(&kind).to_string(),
            loading: issuing,
            // Same gate as the form: an invalid draft never reaches the sheet.
            onclick: {
                let input = input.clone();
                move |_| {
                    if check_before_issue(issue_flow, &input) {
                        confirming.set(true);
                        return;
                    }
                    // The errors are published, but this screen has neither a
                    // block to show them in nor a field to fix — the tap did
                    // nothing at all, which is worse than the form's own
                    // version of this bug. Both live one screen back, and the
                    // form reveals the first faulty field on arrival.
                    if navigator.can_go_back() {
                        navigator.go_back();
                    } else {
                        navigator.push(Route::Form {});
                    }
                }
            },
        }
        IssueConfirmSheet {
            kind: kind.clone(),
            number,
            open: confirming(),
            on_cancel: move |_| confirming.set(false),
            on_confirm: move |_| {
                confirming.set(false);
                start_issue(issue_flow, database.clone(), input.clone());
            },
        }
    }
}

fn load_from_context(
    database: &DatabaseContext,
    document_id: Option<i64>,
) -> Result<PreviewData, PreviewError> {
    let database = database.as_ref().map_err(|_| PreviewError::Unavailable)?;
    let connection = database.lock().map_err(|_| PreviewError::Unavailable)?;
    load_preview(&connection, document_id)
}

fn error_message(error: PreviewError) -> (&'static str, &'static str) {
    match error {
        PreviewError::NoDraft => (
            "Aucun brouillon",
            "Le brouillon est vide : rédigez d’abord le document dans le formulaire.",
        ),
        PreviewError::DocumentNotFound => (
            "Document introuvable",
            "Ce document n’existe pas ou plus dans l’historique.",
        ),
        PreviewError::Unavailable => (
            "Aperçu impossible",
            "Impossible de préparer l’aperçu du document.",
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rusqlite::Connection;
    use tempfile::NamedTempFile;

    use crate::domain::{
        db::{issue_document, open_database, save_draft},
        models::{ClientInput, ClientKind, DocumentInput, DocumentKind, LineInput},
        render::render_document_html,
    };

    use super::{PreviewError, PreviewSource, load_preview};

    fn temp_connection() -> (NamedTempFile, Mutex<Connection>) {
        let file = NamedTempFile::new().expect("temp database file");
        let connection = open_database(file.path()).expect("open temp database");
        (file, connection)
    }

    fn sample_input(kind: DocumentKind) -> DocumentInput {
        DocumentInput {
            kind: kind.clone(),
            issue_date: "2026-07-23".to_string(),
            event_date: "2026-07-30".to_string(),
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
    fn draft_preview_peeks_the_next_number_without_reserving_it() {
        let (_file, database) = temp_connection();
        let mut connection = database.lock().expect("lock database");
        save_draft(
            &connection,
            &sample_input(DocumentKind::Quote),
            "2026-07-23T09:00:00Z",
        )
        .expect("save draft");

        // The quote counter starts at 10; peeking twice must not move it.
        let first = load_preview(&connection, None).expect("first preview");
        assert_eq!(first.source, PreviewSource::Draft);
        assert_eq!(first.number, 10);
        assert!(first.html.contains("<!DOCTYPE"));
        assert!(first.html.contains("Marie Dupont"));
        let second = load_preview(&connection, None).expect("second preview");
        assert_eq!(second.number, 10);

        // The peeked number stays available for the real issuance.
        let issued = issue_document(
            &mut connection,
            sample_input(DocumentKind::Quote),
            "2026-07-23T10:00:00Z",
        )
        .expect("issue document");
        assert_eq!(issued.number, 10);
    }

    #[test]
    fn invoice_draft_preview_uses_the_invoice_sequence() {
        let (_file, database) = temp_connection();
        let connection = database.lock().expect("lock database");
        save_draft(
            &connection,
            &sample_input(DocumentKind::Invoice),
            "2026-07-23T09:00:00Z",
        )
        .expect("save draft");

        let preview = load_preview(&connection, None).expect("preview");
        assert_eq!(preview.kind, DocumentKind::Invoice);
        assert_eq!(preview.number, 1);
    }

    #[test]
    fn issued_preview_renders_the_stored_document_exactly() {
        let (_file, database) = temp_connection();
        let mut connection = database.lock().expect("lock database");
        let issued = issue_document(
            &mut connection,
            sample_input(DocumentKind::Quote),
            "2026-07-23T10:00:00Z",
        )
        .expect("issue document");

        let preview = load_preview(&connection, Some(issued.id)).expect("preview");
        assert_eq!(preview.source, PreviewSource::Issued);
        assert_eq!(preview.number, issued.number);
        assert_eq!(
            preview.html,
            render_document_html(&issued.input, issued.number)
        );
    }

    #[test]
    fn missing_draft_is_an_error() {
        let (_file, database) = temp_connection();
        let connection = database.lock().expect("lock database");
        assert_eq!(load_preview(&connection, None), Err(PreviewError::NoDraft));
    }

    #[test]
    fn unknown_document_is_an_error() {
        let (_file, database) = temp_connection();
        let connection = database.lock().expect("lock database");
        assert_eq!(
            load_preview(&connection, Some(999)),
            Err(PreviewError::DocumentNotFound)
        );
    }

    /// The count is exact or absent — never a guess dressed as a number. And
    /// « 1 pages » on a one-page quote would undo the credit the exactness buys.
    #[test]
    fn the_page_count_is_shown_only_when_it_is_known() {
        use super::{PageCount, PageResult, pages_label};

        let about = |document, pages| PageCount { document, pages };

        assert_eq!(
            pages_label(about(Some(7), PageResult::Known(1)), Some(7)).as_deref(),
            Some("1 page")
        );
        assert_eq!(
            pages_label(about(Some(7), PageResult::Known(3)), Some(7)).as_deref(),
            Some("3 pages")
        );
        assert_eq!(pages_label(about(None, PageResult::Pending), None), None);
        assert_eq!(pages_label(about(None, PageResult::Unstarted), None), None);
        assert_eq!(
            pages_label(about(None, PageResult::Unavailable), None),
            None
        );
    }

    /// The count belongs to one preview. Shown under another document it would
    /// be a confident wrong number on the screen that exists to be trusted.
    #[test]
    fn a_count_from_another_document_is_never_shown() {
        use super::{PageCount, PageResult, pages_label};

        let counted = PageCount {
            document: Some(7),
            pages: PageResult::Known(3),
        };
        assert_eq!(pages_label(counted, Some(8)), None, "another document");
        assert_eq!(pages_label(counted, None), None, "the draft");
        assert_eq!(pages_label(counted, Some(7)).as_deref(), Some("3 pages"));
    }
}
