//! Draft form screen: stacked sections (client, dates, lines, payment terms)
//! bound to the domain `DocumentInput`, with debounced auto-save to the draft
//! store. Lines are summarized as rows, added from the catalog picker sheet
//! (or typed freely) and edited in a bottom sheet (`LineSheet`). The « Émettre »
//! button runs the end-to-end issue flow (`issue` module): validation errors
//! stay on screen as a persistent block with the faulty fields flagged.

use std::time::Duration;

use chrono::Utc;
use dioxus::prelude::*;
use tokio::time::sleep;

use crate::domain::{
    db::{
        clear_line_editor, list_active_catalog_items, load_draft, load_line_editor, save_draft,
        save_line_editor, search_clients,
    },
    models::{
        CatalogItem, ClientInput, ClientKind, DocumentInput, DocumentKind, LineDraft, LineInput,
    },
    money::{format_eur, parse_eur_to_cents},
    numbering::next_number,
    validation::{DocumentField, MAX_LINE_AMOUNT_CENTS, MAX_UNIT_PRICE_CENTS},
};

use super::{
    app::{DatabaseContext, OutsideInteraction, Route},
    components::{
        Button, ButtonVariant, CatalogPicker, ErrorBlock, IssueConfirmSheet, LineEditorState,
        LineSheet, OutlinedField, SegmentedButton, issue_label, parse_quantity, quantity_error,
    },
    issue::{
        IssueFlow, IssuePhase, blocks_draft_persistence, check_before_issue, field_error,
        line_has_error, reveal_first_error, start_issue,
    },
};

const AUTOSAVE_DEBOUNCE: Duration = Duration::from_millis(500);
const SUGGESTION_DEBOUNCE: Duration = Duration::from_millis(200);

#[component]
pub(super) fn Form() -> Element {
    let database = use_context::<DatabaseContext>();
    let navigator = use_navigator();
    let issue_flow = use_context::<IssueFlow>();
    let initial_database = database.clone();
    let catalog_database = database.clone();
    let suggestions_database = database.clone();
    let preview_database = database.clone();
    let issue_database = database.clone();
    let number_database = database.clone();
    let editor_initial_database = database.clone();
    let editor_database = database.clone();
    let flush_database = database.clone();
    // The number the emission is about to spend, held while she confirms. It is
    // peeked, never reserved: cancelling costs nothing.
    let mut confirm_number = use_signal(|| None::<i64>);
    let draft = use_signal(move || load_initial_draft(&initial_database));
    let edit_generation = use_signal(|| 0_u64);
    let mut save_error = use_signal(|| None::<String>);
    // The line she was typing when the app went away. It lives in a sheet, so
    // it used to exist only in memory: a WebView the system recycled took it
    // with it while the rest of the draft survived.
    let line_editor = use_signal(move || {
        load_line_editor_state(&editor_initial_database).map(LineEditorState::from_draft)
    });
    let mut catalog_picker = use_signal(|| None::<Vec<CatalogItem>>);
    let picker_error = use_signal(|| None::<String>);
    let mut client_suggestions = use_signal(Vec::<ClientInput>::new);
    // Pending autocomplete lookup: `Some(name)` asks for a debounced search,
    // `None` keeps the list dismissed (outside tap, scroll gesture, pick).
    let mut suggestion_search = use_signal(|| None::<String>);

    // Debounced auto-save: every edit bumps `edit_generation`; the save only
    // runs once the generation has been stable for AUTOSAVE_DEBOUNCE — and
    // never while an issue is in flight, since the worker clears the draft
    // right after commit and a late save would resurrect it.
    let autosave_flow = issue_flow;
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
            if blocks_draft_persistence(&autosave_flow.0.peek()) {
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

    // Same debounce for the line being typed, self-timed rather than
    // generation-counted: the snapshot taken here is compared to the signal
    // after the wait, so only the last keystroke of a burst reaches SQLite.
    // Closing the sheet writes `None`, which clears the row — the editor never
    // outlives the line it was editing.
    use_effect(move || {
        let snapshot = line_editor.read().as_ref().map(LineEditorState::to_draft);
        let database = editor_database.clone();
        spawn(async move {
            sleep(AUTOSAVE_DEBOUNCE).await;
            if line_editor.peek().as_ref().map(LineEditorState::to_draft) != snapshot {
                return;
            }
            persist_line_editor(&database, snapshot.as_ref());
        });
    });

    // Both debounced saves above live on this screen's scope, and Dioxus drops a
    // scope's spawned tasks when it unmounts (`runtime.rs`, `remove_scope`), so
    // leaving inside the debounce window took the last keystrokes of a burst
    // with it — Back, the top bar's menu, either of them. « Aperçu » was the one
    // path that flushed, and only because it needed the draft on disk for the
    // next screen. CONTEXT.md calls the brouillon « auto-sauvegardé en continu »
    // and DESIGN.md §1 puts « la reprise sans perte » above the rest, so it is
    // written on the way out too.
    let flush_flow = issue_flow;
    use_drop(move || {
        let Some(current) = draft_to_flush(&flush_flow.0.peek(), draft.peek().clone()) else {
            return;
        };
        if let Err(error) = persist_draft(&flush_database, &current) {
            eprintln!("Draft flush on leave failed: {error}");
        }
        // The half-typed line goes with it: same window, same loss, and the
        // sheet is exactly where Back is most likely to be pressed.
        let editor = line_editor.peek().as_ref().map(LineEditorState::to_draft);
        persist_line_editor(&flush_database, editor.as_ref());
    });

    // Debounced client lookup: the SQLite query stays out of the input event
    // path and runs at most once per typing pause; a lookup is dropped as
    // soon as a newer keystroke or a dismissal supersedes it.
    use_effect(move || {
        let Some(name) = suggestion_search() else {
            return;
        };
        let database = suggestions_database.clone();
        spawn(async move {
            sleep(SUGGESTION_DEBOUNCE).await;
            if suggestion_search.peek().as_deref() != Some(name.as_str()) {
                return;
            }
            let matches = match suggestion_query(&name) {
                Some(query) => load_client_suggestions(&database, query),
                None => Vec::new(),
            };
            client_suggestions.set(matches);
        });
    });

    // The shell bumps this on any tap or scroll gesture that bubbles up to
    // it (top bar and scroll gutter included); the autocomplete wrapper
    // stops its own taps from reaching the shell.
    let outside_interaction = use_context::<OutsideInteraction>().0;
    use_effect(move || {
        let _ = outside_interaction();
        close_client_suggestions(client_suggestions, suggestion_search);
    });

    // Validation errors are persistent but never stale: any draft edit drops
    // the block and the field flags (re-tapping « Émettre » re-lists what
    // still needs fixing). The flow signal is peeked, not read — subscribing
    // here would clear freshly published errors on arrival — and generation
    // zero is the mount run, before any edit.
    let mut issue_flow_on_edit = issue_flow;
    use_effect(move || {
        if edit_generation() == 0 {
            return;
        }
        if matches!(
            &*issue_flow_on_edit.0.peek(),
            IssuePhase::Invalid(_) | IssuePhase::Failed(_)
        ) {
            issue_flow_on_edit.0.set(IssuePhase::Idle);
        }
    });

    // The counterpart of the effect above: that one clears the errors on the
    // next edit, this one shows them when they arrive. It subscribes rather
    // than peeks — it has to run *on* the transition into `Invalid`.
    let issue_flow_on_invalid = issue_flow;
    use_effect(move || {
        if let IssuePhase::Invalid(errors) = &*issue_flow_on_invalid.0.read() {
            reveal_first_error(errors);
        }
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
    let picker_error_message = picker_error();
    let suggestions = client_suggestions();
    let (issuing, issue_errors, issue_failure) = match &*issue_flow.0.read() {
        IssuePhase::Running => (true, Vec::new(), None),
        IssuePhase::Invalid(errors) => (false, errors.clone(), None),
        IssuePhase::Failed(message) => (false, Vec::new(), Some(message.clone())),
        _ => (false, Vec::new(), None),
    };
    let line_error_flags: Vec<bool> = (0..current.lines.len())
        .map(|index| line_has_error(&issue_errors, index))
        .collect();
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
                // h3, not h2: the bar's title is the screen's h1 and the gold-
                // ruled title above is its h2, so five sibling h2s left the
                // longest screen of the app with a flat heading tree — nothing
                // for TalkBack's heading navigation to descend into.
                h3 { id: "form-client-title", "Client" }
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
                // The suggestion list flows inside the scroll view, so it
                // never overlays the keyboard; the wrapper keeps its own
                // taps from counting as "outside" for the shell broadcast.
                div {
                    class: "client-autocomplete",
                    onclick: move |event| event.stop_propagation(),
                    OutlinedField {
                        label: "Nom".to_string(),
                        name: "client-name".to_string(),
                        enter_key_hint: "next".to_string(),
                        value: current.client.name.clone(),
                        error: field_error(&issue_errors, DocumentField::ClientName),
                        announce_error: false,
                        oninput: move |event: FormEvent| {
                            let value = event.value();
                            apply_edit(draft, edit_generation, |draft| {
                                draft.client.name = value.clone();
                            });
                            suggestion_search.set(Some(value));
                        },
                        onfocus: move |_| {
                            let name = draft
                                .read()
                                .as_ref()
                                .map(|draft| draft.client.name.clone())
                                .unwrap_or_default();
                            suggestion_search.set(Some(name));
                        },
                    }
                    if !suggestions.is_empty() {
                        ul { class: "client-suggestions", aria_label: "Clients suggérés",
                            // Index keys are acceptable here: rows are
                            // stateless buttons, and two history clients can
                            // share name and address (DISTINCT spans every
                            // client field), so a content key could collide.
                            for (index, client) in suggestions.into_iter().enumerate() {
                                li { key: "{index}",
                                    button {
                                        class: "client-suggestion",
                                        r#type: "button",
                                        aria_label: suggestion_label(&client),
                                        onclick: {
                                            let client = client.clone();
                                            move |_| {
                                                apply_edit(draft, edit_generation, |draft| {
                                                    fill_client_from_suggestion(draft, &client);
                                                });
                                                close_client_suggestions(
                                                    client_suggestions,
                                                    suggestion_search,
                                                );
                                            }
                                        },
                                        span { class: "client-suggestion__name", "{client.name}" }
                                        if let Some(detail) = suggestion_detail(&client) {
                                            span { class: "client-suggestion__detail", "{detail}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                OutlinedField {
                    label: "Adresse".to_string(),
                    name: "client-address".to_string(),
                    enter_key_hint: "next".to_string(),
                    value: current.client.address.clone(),
                    error: field_error(&issue_errors, DocumentField::ClientAddress),
                    announce_error: false,
                    oninput: move |event: FormEvent| {
                        apply_edit(draft, edit_generation, |draft| {
                            draft.client.address = event.value();
                        });
                    },
                }
                OutlinedField {
                    label: "Email".to_string(),
                    name: "client-email".to_string(),
                    enter_key_hint: "next".to_string(),
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
                    enter_key_hint: "next".to_string(),
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
                        enter_key_hint: "next".to_string(),
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
                        enter_key_hint: "next".to_string(),
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
                h3 { id: "form-dates-title", "Dates" }
                OutlinedField {
                    label: "Date d’émission".to_string(),
                    name: "issue-date".to_string(),
                    input_type: "date".to_string(),
                    value: current.issue_date.clone(),
                    error: field_error(&issue_errors, DocumentField::IssueDate),
                    announce_error: false,
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
                    error: field_error(&issue_errors, DocumentField::EventDate),
                    announce_error: false,
                    oninput: move |event: FormEvent| {
                        apply_edit(draft, edit_generation, |draft| {
                            draft.event_date = event.value();
                        });
                    },
                }
            }

            section { class: "form-section", aria_labelledby: "form-lines-title",
                h3 { id: "form-lines-title", "Prestations" }
                if current.lines.is_empty() {
                    // « Ajoutez au moins une prestation » used to appear only in
                    // the block at the very bottom, with nothing marking the
                    // section it was about — several screens of scroll away.
                    if let Some(message) = field_error(&issue_errors, DocumentField::Lines) {
                        // No `role="alert"`: the aggregated block below is the
                        // announcement, and it already lists this sentence. Two
                        // live regions firing on the same tap read it twice —
                        // and `reveal_first_error` scrolls this heading into
                        // view, so it is seen as well as heard.
                        p { class: "outlined-field__error", "{message}" }
                    } else {
                        p { "Aucune prestation pour l’instant." }
                    }
                } else {
                    ul { class: "line-list",
                        // Index keys are acceptable here: rows are stateless
                        // (content is derived from the line, no local state or
                        // focus to preserve across reorders), and `LineInput`
                        // has no stable identifier to key on.
                        for (index, line) in current.lines.iter().enumerate() {
                            li { key: "{index}",
                                button {
                                    // Anchor for `reveal_first_error`.
                                    id: "form-line-{index}",
                                    class: if line_error_flags[index] {
                                        "line-row__main is-error"
                                    } else {
                                        "line-row__main"
                                    },
                                    r#type: "button",
                                    aria_label: line_row_label(index, line),
                                    aria_invalid: line_error_flags[index],
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
                if let Some(error) = picker_error_message {
                    ErrorBlock {
                        title: "Catalogue indisponible".to_string(),
                        message: error,
                    }
                }
                Button {
                    label: "Ajouter une prestation".to_string(),
                    variant: ButtonVariant::Tonal,
                    onclick: move |_| open_catalog_picker(&catalog_database, catalog_picker, picker_error),
                }
            }

            section { class: "form-section", aria_labelledby: "form-terms-title",
                h3 { id: "form-terms-title", "Conditions" }
                OutlinedField {
                    label: "Conditions de paiement".to_string(),
                    name: "payment-terms".to_string(),
                    placeholder: "À réception".to_string(),
                    value: current.payment_terms.clone(),
                    error: field_error(&issue_errors, DocumentField::PaymentTerms),
                    announce_error: false,
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

            // Validation failures stay on screen (DESIGN.md §6 : les erreurs
            // sont des blocs persistants, jamais des snackbars) until the next
            // edit; the faulty fields above carry the same message.
            //
            // This block is also the live region for them: it lists everything
            // that needs fixing, and `reveal_first_error` puts the focus on the
            // first faulty control, which reads its own message from the
            // `aria-describedby` it already has. That is why the fields above
            // pass `announce_error: false` — they are the only ones in the app
            // with somewhere better to be announced from.
            if !issue_errors.is_empty() {
                ErrorBlock {
                    title: "Impossible d’émettre le document".to_string(),
                    items: issue_errors
                        .iter()
                        .map(|error| error.message.clone())
                        .collect(),
                }
            }

            if let Some(message) = issue_failure {
                ErrorBlock {
                    title: "Émission impossible".to_string(),
                    message,
                }
            }

            div { class: "form-sticky",
                p { id: "form-total", class: "total-pill", aria_live: "polite",
                    span { class: "total-pill__label", "Total" }
                    span { class: "total-pill__amount", "{format_eur(current.total_cents())}" }
                }
                footer { class: "chrome-action-bar form-action-bar",
                    Button {
                        label: "Aperçu".to_string(),
                        variant: ButtonVariant::Tonal,
                        onclick: move |_| {
                            // An issue in flight owns the draft: flushing now
                            // would resurrect it after the worker clears it.
                            if blocks_draft_persistence(&issue_flow.0.peek()) {
                                return;
                            }
                            // Flush the pending auto-save first so the preview
                            // renders the draft as just edited.
                            let Some(current) = draft.read().clone() else {
                                return;
                            };
                            match persist_draft(&preview_database, &current) {
                                Ok(()) => {
                                    save_error.set(None);
                                    navigator.push(Route::Preview { document: None });
                                }
                                Err(error) => save_error.set(Some(error)),
                            }
                        },
                    }
                    Button {
                        label: issue_label,
                        loading: issuing,
                        // Émettre is the one irreversible act in the app; it
                        // asks before spending the number, not after.
                        onclick: move |_| {
                            let Some(input) = draft.read().clone() else {
                                return;
                            };
                            // Nothing is asked of her for a document that
                            // cannot be issued: the errors surface directly.
                            if !check_before_issue(issue_flow, &input) {
                                return;
                            }
                            match peek_next_number(&number_database, &input.kind) {
                                Ok(number) => confirm_number.set(Some(number)),
                                Err(error) => save_error.set(Some(error)),
                            }
                        },
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

            if let Some(number) = confirm_number() {
                IssueConfirmSheet {
                    kind: current.kind.clone(),
                    number,
                    open: true,
                    on_cancel: move |_| confirm_number.set(None),
                    on_confirm: move |_| {
                        confirm_number.set(None);
                        if let Some(input) = draft.read().clone() {
                            start_issue(issue_flow, issue_database.clone(), input);
                        }
                    },
                }
            }

            CatalogPicker {
                state: catalog_picker,
                // The sheet stays open: the picker owns its own closing now,
                // so five items cost one visit instead of five.
                // The picker builds the whole line now — it is where the
                // quantity is said, so it is where the line is made.
                on_pick: move |line: LineInput| {
                    apply_edit(draft, edit_generation, move |draft| {
                        draft.lines.push(line);
                    });
                },
                on_free_form: move |_| {
                    catalog_picker.set(None);
                    open_new_line_editor(line_editor);
                },
            }
        }
    }
}

/// What the screen owes the disk on its way out, or `None` when it owes nothing.
///
/// An issue in flight owns the draft: the worker clears it right after the
/// commit, and a late write would resurrect a document that has already been
/// issued — offered again on the home screen, issued twice. Same guard the
/// debounced save and « Aperçu » already carry, in one place they can share.
fn draft_to_flush(phase: &IssuePhase, draft: Option<DocumentInput>) -> Option<DocumentInput> {
    if blocks_draft_persistence(phase) {
        return None;
    }
    draft
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

/// Suggestions pop in from two typed characters (task 17) — counted in
/// chars, not bytes, so an accented letter counts once.
fn suggestion_query(name_value: &str) -> Option<&str> {
    let trimmed = name_value.trim();
    (trimmed.chars().count() >= 2).then_some(trimmed)
}

/// Read-only assist on the issued documents history (task 08 query): a
/// failing lookup only means no suggestions, never a broken form.
fn load_client_suggestions(database: &DatabaseContext, query: &str) -> Vec<ClientInput> {
    let Ok(database) = database.as_ref() else {
        return Vec::new();
    };
    let Ok(connection) = database.lock() else {
        return Vec::new();
    };
    search_clients(&connection, query).unwrap_or_else(|error| {
        eprintln!("Client suggestion query failed: {error}");
        Vec::new()
    })
}

/// Tap on a suggestion pre-fills every client field (CONTEXT.md: no client
/// book — name, address, email, phone, SIRET and billing address all come
/// from issued documents). The fields stay editable afterwards: only this
/// explicit pick rewrites them, later keystrokes are never overwritten.
fn fill_client_from_suggestion(draft: &mut DocumentInput, suggestion: &ClientInput) {
    draft.client = suggestion.clone();
}

/// Dismisses the list: clearing the pending lookup also aborts any
/// debounced search still in flight, so a dismissed list never reopens.
fn close_client_suggestions(
    mut suggestions: Signal<Vec<ClientInput>>,
    mut search: Signal<Option<String>>,
) {
    if search.peek().is_some() {
        search.set(None);
    }
    if !suggestions.peek().is_empty() {
        suggestions.set(Vec::new());
    }
}

/// Disambiguation line under the client name: street address first
/// (homonyms), then email, then phone.
fn suggestion_detail(client: &ClientInput) -> Option<String> {
    if !client.address.is_empty() {
        return Some(client.address.clone());
    }
    client.email.clone().or_else(|| client.phone.clone())
}

fn suggestion_label(client: &ClientInput) -> String {
    format!("Pré-remplir le client avec {}", client.name)
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

fn open_catalog_picker(
    database: &DatabaseContext,
    mut catalog_picker: Signal<Option<Vec<CatalogItem>>>,
    mut picker_error: Signal<Option<String>>,
) {
    match load_active_items(database) {
        Ok(items) => {
            picker_error.set(None);
            catalog_picker.set(Some(items));
        }
        Err(error) => picker_error.set(Some(error)),
    }
}

/// Active items only: an inactive item never appears in the picker, while
/// the lines it produced stay untouched (copies, not references).
fn load_active_items(database: &DatabaseContext) -> Result<Vec<CatalogItem>, String> {
    let database = database.as_ref().map_err(Clone::clone)?;
    let connection = database
        .lock()
        .map_err(|_| "Impossible d’accéder aux données locales.".to_string())?;
    list_active_catalog_items(&connection).map_err(|error| {
        eprintln!("Catalog picker query failed: {error}");
        "Impossible de charger le catalogue.".to_string()
    })
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

fn load_line_editor_state(database: &DatabaseContext) -> Option<LineDraft> {
    let database = database.as_ref().ok()?;
    let connection = database.lock().ok()?;
    load_line_editor(&connection).ok().flatten()
}

/// Failures here are swallowed: losing a half-typed line is a small harm, and
/// an error block about it would sit on top of the form she is typing in.
fn persist_line_editor(database: &DatabaseContext, editor: Option<&LineDraft>) {
    let Ok(database) = database.as_ref() else {
        return;
    };
    let Ok(connection) = database.lock() else {
        return;
    };
    let result = match editor {
        Some(editor) => save_line_editor(&connection, editor),
        None => clear_line_editor(&connection),
    };
    if let Err(error) = result {
        eprintln!("Line editor persistence failed: {error}");
    }
}

/// Reads the counter without touching it — the confirmation sheet needs to name
/// the number, and a cancelled emission must leave the sequence untouched.
fn peek_next_number(database: &DatabaseContext, kind: &DocumentKind) -> Result<i64, String> {
    let database = database.as_ref().map_err(Clone::clone)?;
    let connection = database
        .lock()
        .map_err(|_| "Impossible d’accéder aux données locales.".to_string())?;
    next_number(&connection, kind).map_err(|error| {
        eprintln!("Number peek failed: {error}");
        "Impossible de lire le prochain numéro.".to_string()
    })
}

fn draft_title(kind: &DocumentKind) -> &'static str {
    match kind {
        DocumentKind::Quote => "Brouillon de devis",
        DocumentKind::Invoice => "Brouillon de facture",
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
    use std::sync::Arc;

    use super::{
        DatabaseContext, IssuePhase, LineEditorState, cents_to_euro_input, client_kind_for_index,
        client_kind_index, draft_title, draft_to_flush, fill_client_from_suggestion, issue_label,
        line_from_editor, line_row_label, load_client_suggestions, move_line, optional_text,
        parse_quantity, price_error, quantity_error, suggestion_detail, suggestion_query,
    };
    use crate::domain::{
        db::{issue_document, open_database},
        models::{ClientInput, ClientKind, DocumentInput, DocumentKind, LineInput},
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

    /// The debounced save dies with the screen, so leaving flushes what it had
    /// not written yet — except while the issue chain owns the draft, where a
    /// late write would resurrect a document that has already been issued.
    #[test]
    fn the_flush_on_leave_writes_the_draft_unless_an_issue_owns_it() {
        let draft = valid_quote_input();

        for phase in [
            IssuePhase::Idle,
            IssuePhase::Invalid(Vec::new()),
            IssuePhase::Failed("erreur".to_string()),
        ] {
            assert_eq!(
                draft_to_flush(&phase, Some(draft.clone())),
                Some(draft.clone()),
                "{phase:?} does not own the draft"
            );
        }

        assert_eq!(
            draft_to_flush(&IssuePhase::Running, Some(draft.clone())),
            None,
            "the worker clears the draft right after the commit"
        );
        assert_eq!(draft_to_flush(&IssuePhase::Idle, None), None);
    }

    #[test]
    fn suggestion_query_requires_two_characters_after_trimming() {
        for value in ["", "  ", "m", " m ", "é"] {
            assert_eq!(
                suggestion_query(value),
                None,
                "{value:?} should not trigger a search"
            );
        }
        assert_eq!(suggestion_query("ma"), Some("ma"));
        assert_eq!(suggestion_query("  mai  "), Some("mai"));
        assert_eq!(suggestion_query("ém"), Some("ém"));
    }

    #[test]
    fn suggestion_detail_prefers_address_then_email_then_phone() {
        let mut client = mairie_client();
        assert_eq!(
            suggestion_detail(&client).as_deref(),
            Some("12 rue Émile Zola, Lyon")
        );

        client.address = String::new();
        assert_eq!(
            suggestion_detail(&client).as_deref(),
            Some("contact@example.com")
        );

        client.email = None;
        assert_eq!(suggestion_detail(&client).as_deref(), Some("0601020304"));

        client.phone = None;
        assert_eq!(suggestion_detail(&client), None);
    }

    #[test]
    fn fill_client_from_suggestion_replaces_every_field_then_stays_editable() {
        let mut draft = empty_draft();
        let suggestion = mairie_client();

        fill_client_from_suggestion(&mut draft, &suggestion);
        assert_eq!(draft.client, suggestion);

        // The form never re-applies a picked suggestion: later manual edits
        // only touch their own field (acceptance: no re-overwriting).
        draft.client.address = "3 place Bellecour, Lyon".to_string();
        draft.client.name = "Mairie de Lyon — protocole".to_string();
        assert_eq!(draft.client.address, "3 place Bellecour, Lyon");
        assert_eq!(draft.client.name, "Mairie de Lyon — protocole");
        assert_eq!(draft.client.email, suggestion.email);
        assert_eq!(draft.client.phone, suggestion.phone);
        assert_eq!(draft.client.business_id, suggestion.business_id);
        assert_eq!(draft.client.billing_address, suggestion.billing_address);
    }

    #[test]
    fn load_client_suggestions_matches_issued_document_history() {
        let file = tempfile::NamedTempFile::new().expect("create temp db");
        let mut database = open_database(file.path()).expect("open initialized db");
        let input = DocumentInput {
            client: mairie_client(),
            ..valid_quote_input()
        };
        issue_document(
            database.get_mut().expect("lock db"),
            input,
            "2026-07-22T10:00:00Z",
        )
        .expect("issue document");
        let context: DatabaseContext = Ok(Arc::new(database));

        let matches = load_client_suggestions(&context, "mai");
        assert_eq!(matches, vec![mairie_client()]);
    }

    #[test]
    fn load_client_suggestions_stays_empty_without_history_or_database() {
        let file = tempfile::NamedTempFile::new().expect("create temp db");
        let database = open_database(file.path()).expect("open initialized db");
        let context: DatabaseContext = Ok(Arc::new(database));
        assert!(load_client_suggestions(&context, "mai").is_empty());

        let broken: DatabaseContext = Err("base indisponible".to_string());
        assert!(load_client_suggestions(&broken, "mai").is_empty());
    }

    fn mairie_client() -> ClientInput {
        ClientInput {
            kind: ClientKind::Professional,
            name: "Mairie de Lyon".to_string(),
            address: "12 rue Émile Zola, Lyon".to_string(),
            email: Some("contact@example.com".to_string()),
            phone: Some("0601020304".to_string()),
            business_id: Some("123 456 789 00010".to_string()),
            billing_address: Some("Étage 1".to_string()),
        }
    }

    fn valid_quote_input() -> DocumentInput {
        DocumentInput {
            kind: DocumentKind::Quote,
            issue_date: "2026-07-22".to_string(),
            event_date: "2026-08-15".to_string(),
            payment_terms: "à réception".to_string(),
            client: mairie_client(),
            lines: vec![LineInput {
                group: Some("Salé".to_string()),
                description: "Mini burgers".to_string(),
                quantity: 50,
                unit_price_cents: 85,
            }],
            source_quote_id: None,
        }
    }

    fn empty_draft() -> DocumentInput {
        DocumentInput {
            kind: DocumentKind::Quote,
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
