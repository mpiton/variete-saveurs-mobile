use chrono::Utc;
use dioxus::prelude::*;

use crate::domain::{
    db::{list_documents, load_draft, save_draft},
    models::{ClientInput, ClientKind, Document, DocumentInput, DocumentKind},
    money::format_eur,
    settings::{dismiss_email_setup, email_setup_dismissed, load_email_settings},
};

use super::{
    app::{DatabaseContext, Route},
    components::{
        BottomSheet, Button, ButtonVariant, DocumentCard, EmptyState, ErrorBlock, FabMenu,
        SegmentedButton,
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum HomeFilter {
    #[default]
    All,
    Quotes,
    Invoices,
}

impl HomeFilter {
    const ALL: [Self; 3] = [Self::All, Self::Quotes, Self::Invoices];

    const fn index(self) -> usize {
        match self {
            Self::All => 0,
            Self::Quotes => 1,
            Self::Invoices => 2,
        }
    }

    const fn kind(self) -> Option<DocumentKind> {
        match self {
            Self::All => None,
            Self::Quotes => Some(DocumentKind::Quote),
            Self::Invoices => Some(DocumentKind::Invoice),
        }
    }
}

#[derive(Default, PartialEq, Eq)]
struct HomeData {
    documents: Vec<Document>,
    draft: Option<DocumentInput>,
    /// First-launch prompt (task 25): shown while email sending is
    /// unconfigured and the gérante hasn't dismissed it. Non-blocking — the
    /// app works without, only sending stays gated off on the fiche.
    show_email_setup: bool,
}

#[component]
pub(super) fn Home() -> Element {
    let database = use_context::<DatabaseContext>();
    let navigator = use_navigator();
    let mut filter = use_signal(HomeFilter::default);
    let mut fab_open = use_signal(|| false);
    let mut pending_kind = use_signal(|| None::<DocumentKind>);
    let mut action_error = use_signal(|| None::<String>);
    let reload = use_signal(|| 0_u64);
    let database_ready = database.is_ok();
    let home_data = {
        let database = database.clone();
        use_memo(move || {
            // Subscribing to `reload` refreshes after « Plus tard ».
            reload();
            if database.is_ok() {
                Some(load_home_data(&database, filter()))
            } else {
                None
            }
        })
    };
    let home_data = home_data.read();
    let (documents, draft, show_email_setup, load_error) = match home_data.as_ref() {
        Some(Ok(data)) => (
            data.documents.as_slice(),
            data.draft.as_ref(),
            data.show_email_setup,
            None,
        ),
        Some(Err(error)) => (&[][..], None, false, Some(error.as_str())),
        None => (&[][..], None, false, None),
    };
    let has_draft = draft.is_some();
    let draft_kind = draft.map(|draft| document_kind_label(&draft.kind));
    let documents_empty = documents.is_empty();
    let action_error_message = action_error();
    let replacement_open = pending_kind().is_some();

    rsx! {
        section { class: "screen home-screen", aria_label: "Documents",
            if database_ready {
                if let Some(error) = load_error {
                    ErrorBlock { title: "Chargement impossible", message: error.to_string() }
                } else {
                    SegmentedButton {
                        label: "Filtrer les documents",
                        options: vec!["Tous".to_string(), "Devis".to_string(), "Factures".to_string()],
                        selected: filter().index(),
                        on_select: move |index| {
                            if let Some(selected) = HomeFilter::ALL.get(index).copied() {
                                filter.set(selected);
                            }
                        },
                    }

                    if show_email_setup {
                        div { class: "setup-prompt",
                            p { class: "setup-prompt__text",
                                "Pour envoyer vos documents par email, configurez la clé Brevo et l’adresse d’expédition. Le reste de l’app fonctionne sans."
                            }
                            div { class: "setup-prompt__actions",
                                Button {
                                    label: "Configurer".to_string(),
                                    variant: ButtonVariant::Tonal,
                                    onclick: move |_| {
                                        navigator.push(Route::Settings {});
                                    },
                                }
                                Button {
                                    label: "Plus tard".to_string(),
                                    variant: ButtonVariant::Text,
                                    onclick: {
                                        let database = database.clone();
                                        move |_| {
                                            dismiss_setup_prompt(&database, reload);
                                        }
                                    },
                                }
                            }
                        }
                    }

                    if let Some(kind) = draft_kind {
                        button {
                            class: "draft-resume-card",
                            r#type: "button",
                            onclick: move |_| {
                                navigator.push(Route::Form {});
                            },
                            strong { "Reprendre le brouillon" }
                            span { "{kind}" }
                        }
                    }

                    if !replacement_open {
                        if let Some(error) = action_error_message.clone() {
                            ErrorBlock { title: "Création impossible", message: error }
                        }
                    }

                    if documents_empty {
                        EmptyState {
                            message: "Aucun document",
                            action_label: "Créer",
                            onclick: move |_| fab_open.set(true),
                        }
                    } else {
                        div { class: "home-document-list",
                            for document in documents {
                                DocumentCard {
                                    key: "{document.id}",
                                    document_type: document_kind_label(&document.input.kind),
                                    number: document.number,
                                    client: document.input.client.name.clone(),
                                    total: format_eur(document.total_cents),
                                    sent: document.is_sent(),
                                    invoiced: document.is_invoiced,
                                    onclick: {
                                        let id = document.id;
                                        move |_| {
                                            navigator.push(Route::Record { id });
                                        }
                                    },
                                }
                            }
        }
                    }

                    div { class: "home-fab",
                        FabMenu {
                            id: "home-create-menu",
                            open: fab_open(),
                            on_toggle: move |_| fab_open.toggle(),
                            on_quote: {
                                let database = database.clone();
                                move |_| {
                                    fab_open.set(false);
                                    request_new_draft(
                                        &database,
                                        has_draft,
                                        DocumentKind::Quote,
                                        navigator,
                                        pending_kind,
                                        action_error,
                                    );
                                }
                            },
                            on_invoice: {
                                let database = database.clone();
                                move |_| {
                                    fab_open.set(false);
                                    request_new_draft(
                                        &database,
                                        has_draft,
                                        DocumentKind::Invoice,
                                        navigator,
                                        pending_kind,
                                        action_error,
                                    );
                                }
                            },
                        }
                    }

                    BottomSheet {
                        id: "replace-draft-sheet",
                        title: "Remplacer le brouillon ?",
                        open: replacement_open,
                        error: action_error_message.is_some(),
                        on_dismiss: move |_| {
                            pending_kind.set(None);
                            action_error.set(None);
                        },
                        p { "Le brouillon actuel sera remplacé par un document vide." }
                        if let Some(error) = action_error_message.clone() {
                            ErrorBlock { title: "Remplacement impossible", message: error }
                        }
                        div { class: "home-confirmation-actions",
                            Button {
                                label: "Annuler",
                                variant: ButtonVariant::Text,
                                onclick: move |_| {
                                    pending_kind.set(None);
                                    action_error.set(None);
                                },
                            }
                            Button {
                                label: "Remplacer",
                                onclick: {
                                    let database = database.clone();
                                    move |_| {
                                        let Some(kind) = pending_kind() else {
                                            return;
                                        };
                                        action_error.set(None);
                                        match persist_new_draft(&database, kind) {
                                            Ok(()) => {
                                                pending_kind.set(None);
                                                navigator.push(Route::Form {});
                                            }
                                            Err(error) => action_error.set(Some(error)),
                                        }
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

fn load_home_data(database: &DatabaseContext, filter: HomeFilter) -> Result<HomeData, String> {
    let database = database.as_ref().map_err(Clone::clone)?;
    let connection = database
        .lock()
        .map_err(|_| "Impossible d’accéder aux données locales.".to_string())?;
    let kind = filter.kind();
    let documents = list_documents(&connection, kind.as_ref()).map_err(|error| {
        eprintln!("Home document query failed: {error}");
        "Impossible de charger les documents.".to_string()
    })?;
    let draft = load_draft(&connection).map_err(|error| {
        eprintln!("Home draft query failed: {error}");
        "Impossible de charger le brouillon.".to_string()
    })?;
    // A settings read failure must not break the home screen: the prompt
    // simply stays hidden, and the fiche re-checks before sending anyway.
    let show_email_setup = match load_email_settings(&connection) {
        Ok(settings) if settings.is_configured() => false,
        Ok(_) => match email_setup_dismissed(&connection) {
            Ok(dismissed) => !dismissed,
            Err(error) => {
                eprintln!("Home settings query failed: {error}");
                false
            }
        },
        Err(error) => {
            eprintln!("Home settings query failed: {error}");
            false
        }
    };
    Ok(HomeData {
        documents,
        draft,
        show_email_setup,
    })
}

/// « Plus tard » persists the dismissal: the prompt never nags again, the
/// fiche keeps its disabled-send explanation as the remaining cue. If the
/// write fails the prompt simply stays — nothing changed for her.
fn dismiss_setup_prompt(database: &DatabaseContext, mut reload: Signal<u64>) {
    let persisted = (|| {
        let database = database.as_ref().ok()?;
        let connection = database.lock().ok()?;
        dismiss_email_setup(&connection)
            .map_err(|error| {
                eprintln!("Settings dismissal failed: {error}");
            })
            .ok()
    })();
    if persisted.is_some() {
        *reload.write() += 1;
    }
}

fn request_new_draft(
    database: &DatabaseContext,
    has_draft: bool,
    kind: DocumentKind,
    navigator: dioxus_router::Navigator,
    mut pending_kind: Signal<Option<DocumentKind>>,
    mut action_error: Signal<Option<String>>,
) {
    action_error.set(None);
    if has_draft {
        pending_kind.set(Some(kind));
    } else {
        match persist_new_draft(database, kind) {
            Ok(()) => {
                navigator.push(Route::Form {});
            }
            Err(error) => action_error.set(Some(error)),
        }
    }
}

fn persist_new_draft(database: &DatabaseContext, kind: DocumentKind) -> Result<(), String> {
    let database = database.as_ref().map_err(Clone::clone)?;
    let connection = database
        .lock()
        .map_err(|_| "Impossible d’accéder aux données locales.".to_string())?;
    save_draft(&connection, &blank_draft(kind), &Utc::now().to_rfc3339()).map_err(|error| {
        eprintln!("Draft creation failed: {error}");
        "Impossible de créer le brouillon.".to_string()
    })
}

fn blank_draft(kind: DocumentKind) -> DocumentInput {
    DocumentInput {
        kind,
        issue_date: String::new(),
        event_date: String::new(),
        payment_terms: String::new(),
        client: ClientInput {
            kind: ClientKind::Individual,
            name: String::new(),
            address: String::new(),
            email: None,
            phone: None,
            business_id: None,
            billing_address: None,
        },
        lines: Vec::new(),
        source_quote_id: None,
    }
}

fn document_kind_label(kind: &DocumentKind) -> String {
    kind.label().to_string()
}

#[cfg(test)]
mod tests {
    use tempfile::NamedTempFile;

    use super::{DatabaseContext, HomeFilter, blank_draft, load_home_data};
    use crate::domain::{
        db::open_database,
        models::DocumentKind,
        settings::{dismiss_email_setup, save_email_settings},
    };

    fn temp_context() -> (NamedTempFile, DatabaseContext) {
        let file = NamedTempFile::new().expect("temp database file");
        let connection = open_database(file.path()).expect("open temp database");
        (file, Ok(std::sync::Arc::new(connection)))
    }

    #[test]
    fn email_setup_prompt_shows_until_dismissed_or_configured() {
        let (_file, database) = temp_context();

        let data = load_home_data(&database, HomeFilter::All).expect("load home data");
        assert!(data.show_email_setup);

        {
            let connection = database.as_ref().expect("database").lock().expect("lock");
            dismiss_email_setup(&connection).expect("dismiss");
        }
        let data = load_home_data(&database, HomeFilter::All).expect("load home data");
        assert!(!data.show_email_setup);
    }

    #[test]
    fn email_setup_prompt_hidden_once_configured() {
        let (_file, database) = temp_context();

        {
            let connection = database.as_ref().expect("database").lock().expect("lock");
            save_email_settings(&connection, Some("key"), "contact@variete-saveurs.fr", "")
                .expect("save settings");
        }
        let data = load_home_data(&database, HomeFilter::All).expect("load home data");
        assert!(!data.show_email_setup);
    }

    #[test]
    fn filters_map_to_the_document_query_kinds() {
        assert_eq!(HomeFilter::All.kind(), None);
        assert_eq!(HomeFilter::Quotes.kind(), Some(DocumentKind::Quote));
        assert_eq!(HomeFilter::Invoices.kind(), Some(DocumentKind::Invoice));
    }

    #[test]
    fn a_new_draft_is_empty_and_keeps_the_selected_kind() {
        for kind in [DocumentKind::Quote, DocumentKind::Invoice] {
            let draft = blank_draft(kind.clone());

            assert_eq!(draft.kind, kind);
            assert!(draft.issue_date.is_empty());
            assert!(draft.event_date.is_empty());
            assert!(draft.payment_terms.is_empty());
            assert!(draft.client.name.is_empty());
            assert!(draft.lines.is_empty());
        }
    }
}
