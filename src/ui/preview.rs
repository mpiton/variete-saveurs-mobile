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
    models::DocumentKind,
    numbering::next_number,
    render::render_document_html,
};

use super::{
    app::DatabaseContext,
    components::{Button, ButtonVariant, ErrorBlock, issue_label},
};

const PREVIEW_GESTURES: &str = include_str!("preview_gestures.js");

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
                kind: document.input.kind,
                number: document.number,
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
                kind: input.kind,
                number,
            })
        }
    }
}

#[component]
pub(super) fn Preview(document: Option<i64>) -> Element {
    let database = use_context::<DatabaseContext>();
    let navigator = use_navigator();

    // Loaded synchronously in the body: this screen subscribes to no signal,
    // so the query + render run once per mount or route-param change.
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
                    footer { class: "chrome-action-bar preview-action-bar",
                        if draft {
                            Button {
                                label: issue_label(&data.kind).to_string(),
                                // Branché sur le flux d’émission dans la tâche 20.
                                disabled: true,
                                onclick: move |_| {},
                            }
                        } else {
                            Button {
                                label: "Exporter".to_string(),
                                variant: ButtonVariant::Tonal,
                                // Branché sur l’export PDF/PNG dans la tâche 19.
                                disabled: true,
                                onclick: move |_| {},
                            }
                            Button {
                                label: "Partager".to_string(),
                                variant: ButtonVariant::Tonal,
                                // Branché sur le partage dans la tâche 22.
                                disabled: true,
                                onclick: move |_| {},
                            }
                            Button {
                                label: "Envoyer".to_string(),
                                variant: ButtonVariant::Tonal,
                                // Branché sur l’envoi email dans les tâches 26/27.
                                disabled: true,
                                onclick: move |_| {},
                            }
                        }
                    }
                }
            }
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
            None,
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
            None,
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
}
