use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

use chrono::{Local, Utc};
use dioxus::prelude::*;

use crate::domain::{
    db::{count_documents, list_documents, load_draft, save_draft},
    models::{ClientInput, ClientKind, Document, DocumentInput, DocumentKind},
    money::format_eur,
    render::format_date,
    settings::{dismiss_email_setup, email_setup_dismissed, load_email_settings},
};

use super::{
    app::{DatabaseContext, OutsideInteraction, Route},
    components::{
        BottomSheet, Button, ButtonVariant, DocumentCard, EmptyState, ErrorBlock, FabMenu,
        OutlinedField, SegmentedButton,
    },
};

/// The filter outlives the screen. She narrows to « Factures », opens one,
/// comes back — and used to land on « Tous » again, every time. Session-scoped
/// on purpose: a cold start opens on everything.
static LAST_FILTER: AtomicUsize = AtomicUsize::new(0);

/// Below this many documents the list is short enough to read, and a permanent
/// search field would be a fifth thing competing for the home screen. Above it,
/// « le devis de la mairie » starts to be a scroll. At a few documents a month,
/// it lands near the end of the first year.
const SEARCH_THRESHOLD: usize = 15;

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
    /// Every document, ignoring the filter and the search: the search field
    /// appears on the size of the history, not on what is currently shown.
    total_documents: usize,
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
    let mut filter = use_signal(|| {
        HomeFilter::ALL
            .get(LAST_FILTER.load(AtomicOrdering::Relaxed))
            .copied()
            .unwrap_or_default()
    });
    let mut fab_open = use_signal(|| false);
    // Same dismissal contract as the app menu: a tap or a scroll anywhere else
    // closes it, so the thumb is never trapped by a transient affordance.
    let outside_interaction = use_context::<OutsideInteraction>().0;
    use_effect(move || {
        let _ = outside_interaction();
        fab_open.set(false);
    });
    // Deliberately not persisted, unlike the filter: one looks something up,
    // consults it, and comes back to the whole list.
    let mut search = use_signal(String::new);
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
                let term = search();
                Some(load_home_data(&database, filter(), Some(term.as_str())))
            } else {
                None
            }
        })
    };
    let home_data = home_data.read();
    let (documents, total_documents, draft, show_email_setup, load_error) = match home_data.as_ref()
    {
        Some(Ok(data)) => (
            data.documents.as_slice(),
            data.total_documents,
            data.draft.as_ref(),
            data.show_email_setup,
            None,
        ),
        Some(Err(error)) => (&[][..], 0, None, false, Some(error.as_str())),
        None => (&[][..], 0, None, false, None),
    };
    let searching = !search().trim().is_empty();
    let offers_search = total_documents > SEARCH_THRESHOLD;
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
                                LAST_FILTER.store(index, AtomicOrdering::Relaxed);
                                filter.set(selected);
                            }
                        },
                    }

                    if offers_search {
                        div { class: "home-search",
                            OutlinedField {
                                label: "Rechercher une cliente".to_string(),
                                name: "home-search".to_string(),
                                value: search(),
                                autocomplete: "off".to_string(),
                                oninput: move |event: FormEvent| search.set(event.value()),
                            }
                            if searching {
                                Button {
                                    label: "Effacer".to_string(),
                                    variant: ButtonVariant::Text,
                                    onclick: move |_| search.set(String::new()),
                                }
                            }
                        }
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
                        // Three different nothings. « Aucun document » on a
                        // history full of quotes, because the filter says
                        // Factures, sent her looking for a bug that was not one.
                        if searching {
                            EmptyState {
                                message: "Aucune cliente de ce nom",
                                action_label: "Effacer la recherche",
                                onclick: move |_| search.set(String::new()),
                            }
                        } else if filter() != HomeFilter::All {
                            EmptyState {
                                message: "Rien dans ce filtre",
                                action_label: "Voir tous les documents",
                                onclick: move |_| {
                                    LAST_FILTER.store(0, AtomicOrdering::Relaxed);
                                    filter.set(HomeFilter::All);
                                },
                            }
                        } else {
                            EmptyState {
                                message: "Aucun document",
                                action_label: "Créer",
                                onclick: move |event: MouseEvent| {
                                    event.stop_propagation();
                                    fab_open.set(true);
                                },
                            }
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
                                    issue_date: format_date(&document.input.issue_date),
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
                            // Stops the shell from reading its own trigger as an
                            // outside tap and closing the menu on open.
                            on_toggle: move |event: MouseEvent| {
                                event.stop_propagation();
                                fab_open.toggle();
                            },
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
                        div { class: "confirmation-actions",
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

fn load_home_data(
    database: &DatabaseContext,
    filter: HomeFilter,
    search: Option<&str>,
) -> Result<HomeData, String> {
    let database = database.as_ref().map_err(Clone::clone)?;
    let connection = database
        .lock()
        .map_err(|_| "Impossible d’accéder aux données locales.".to_string())?;
    let kind = filter.kind();
    let documents = list_documents(&connection, kind.as_ref(), search).map_err(|error| {
        eprintln!("Home document query failed: {error}");
        "Impossible de charger les documents.".to_string()
    })?;
    let total_documents = count_documents(&connection).map_err(|error| {
        eprintln!("Home document count failed: {error}");
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
        total_documents,
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
    // The issue date is the day *she* is living, not the day in UTC: she works
    // in the evening, and between midnight and 02:00 in summer UTC is still
    // yesterday — the document would carry the wrong date, frozen.
    let now = Utc::now();
    let draft = blank_draft(kind, &Local::now().format("%Y-%m-%d").to_string());
    save_draft(&connection, &draft, &now.to_rfc3339()).map_err(|error| {
        eprintln!("Draft creation failed: {error}");
        "Impossible de créer le brouillon.".to_string()
    })
}

/// Dated by the caller, like a conversion or a duplication already are: the
/// issue date is the day she writes it, not a mandatory field to retype for
/// every document of the evening. The event date stays empty — that one is a
/// real choice, and a plausible wrong default would ship inside a document she
/// can no longer amend.
fn blank_draft(kind: DocumentKind, issue_date: &str) -> DocumentInput {
    DocumentInput {
        kind,
        issue_date: issue_date.to_string(),
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

        let data = load_home_data(&database, HomeFilter::All, None).expect("load home data");
        assert!(data.show_email_setup);

        {
            let connection = database.as_ref().expect("database").lock().expect("lock");
            dismiss_email_setup(&connection).expect("dismiss");
        }
        let data = load_home_data(&database, HomeFilter::All, None).expect("load home data");
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
        let data = load_home_data(&database, HomeFilter::All, None).expect("load home data");
        assert!(!data.show_email_setup);
    }

    #[test]
    fn filters_map_to_the_document_query_kinds() {
        assert_eq!(HomeFilter::All.kind(), None);
        assert_eq!(HomeFilter::Quotes.kind(), Some(DocumentKind::Quote));
        assert_eq!(HomeFilter::Invoices.kind(), Some(DocumentKind::Invoice));
    }

    #[test]
    fn a_new_draft_is_dated_today_and_keeps_the_selected_kind() {
        for kind in [DocumentKind::Quote, DocumentKind::Invoice] {
            let draft = blank_draft(kind.clone(), "2026-07-26");

            assert_eq!(draft.kind, kind);
            // The issue date is the one field she never has to retype…
            assert_eq!(draft.issue_date, "2026-07-26");
            // …and the event date is the one she must always choose: a
            // plausible default here would ship inside a frozen document.
            assert!(draft.event_date.is_empty());
            assert!(draft.payment_terms.is_empty());
            assert!(draft.client.name.is_empty());
            assert!(draft.lines.is_empty());
        }
    }
}
