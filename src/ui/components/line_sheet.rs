//! Line editor bottom sheet: a line is summarized as a row in the form list
//! and edited in this sheet — the pattern chosen for mobile line editing
//! (left open by DESIGN §5), consistent with the catalog sheet and the Règle
//! du Carnet. Deletion is a lightweight two-tap inline confirmation, never a
//! dialog; a minimum delay between the two taps filters accidental
//! double-taps.

use std::time::{Duration, Instant};

use dioxus::prelude::*;

use super::{
    actions::{Button, ButtonVariant},
    feedback::BottomSheet,
    fields::OutlinedField,
};

/// Minimum delay between arming the deletion and accepting the confirming
/// tap, so an accidental double-tap cannot delete a line.
const DELETE_CONFIRM_MIN_DELAY: Duration = Duration::from_millis(400);

/// Editor draft for one line, owned by the form screen: raw field strings.
/// The price is typed in euros and converted to cents by the domain
/// (`money::parse_eur_to_cents`) when the sheet is saved — never a float.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineEditorState {
    /// `None` while adding a line, `Some(index)` while editing one.
    pub index: Option<usize>,
    pub description: String,
    pub quantity: String,
    pub price: String,
    pub group: String,
    pub confirm_delete: bool,
    /// When the deletion was armed — the confirming tap is only accepted
    /// after `DELETE_CONFIRM_MIN_DELAY`.
    pub confirm_armed_at: Option<Instant>,
    pub quantity_error: Option<String>,
    pub price_error: Option<String>,
}

#[component]
pub fn LineSheet(
    editor: Signal<Option<LineEditorState>>,
    can_move_up: bool,
    can_move_down: bool,
    on_save: EventHandler<MouseEvent>,
    on_delete: EventHandler<MouseEvent>,
    on_move_up: EventHandler<MouseEvent>,
    on_move_down: EventHandler<MouseEvent>,
) -> Element {
    let Some(state) = editor.read().clone() else {
        return rsx! {};
    };
    let editing = state.index.is_some();
    let title = match state.index {
        Some(index) => format!("Ligne {}", index + 1),
        None => "Nouvelle prestation".to_string(),
    };
    let delete_class = if state.confirm_delete {
        "m3-button m3-button--text line-sheet__delete is-confirm"
    } else {
        "m3-button m3-button--text line-sheet__delete"
    };

    rsx! {
        BottomSheet {
            id: "line-sheet".to_string(),
            title,
            open: true,
            on_dismiss: move |_| editor.set(None),
            OutlinedField {
                label: "Désignation".to_string(),
                name: "line-description".to_string(),
                value: state.description,
                oninput: move |event: FormEvent| update_editor(editor, |state| state.description = event.value()),
            }
            div { class: "line-sheet__row",
                OutlinedField {
                    label: "Quantité".to_string(),
                    name: "line-quantity".to_string(),
                    input_mode: "numeric".to_string(),
                    value: state.quantity,
                    error: state.quantity_error,
                    oninput: move |event: FormEvent| update_editor(editor, |state| {
                        state.quantity = event.value();
                        state.quantity_error = None;
                    }),
                }
                OutlinedField {
                    label: "Prix unitaire".to_string(),
                    name: "line-price".to_string(),
                    input_mode: "decimal".to_string(),
                    placeholder: "0,00".to_string(),
                    value: state.price,
                    error: state.price_error,
                    oninput: move |event: FormEvent| update_editor(editor, |state| {
                        state.price = event.value();
                        state.price_error = None;
                    }),
                }
            }
            OutlinedField {
                label: "Groupe (optionnel)".to_string(),
                name: "line-group".to_string(),
                placeholder: "Salé, Sucré…".to_string(),
                value: state.group,
                oninput: move |event: FormEvent| update_editor(editor, |state| state.group = event.value()),
            }
            Button {
                label: "Enregistrer".to_string(),
                onclick: move |event| {
                    disarm_delete(editor);
                    on_save.call(event);
                },
            }
            if editing {
                div { class: "line-sheet__row",
                    Button {
                        label: "Monter".to_string(),
                        variant: ButtonVariant::Outlined,
                        disabled: !can_move_up,
                        onclick: move |event| {
                            disarm_delete(editor);
                            on_move_up.call(event);
                        },
                    }
                    Button {
                        label: "Descendre".to_string(),
                        variant: ButtonVariant::Outlined,
                        disabled: !can_move_down,
                        onclick: move |event| {
                            disarm_delete(editor);
                            on_move_down.call(event);
                        },
                    }
                }
                // Raw button mirroring `ButtonVariant::Text`: `Button` has no
                // class/danger override, and deletion needs the danger color.
                button {
                    class: delete_class,
                    r#type: "button",
                    onclick: move |event| {
                        let armed_at = editor
                            .read()
                            .as_ref()
                            .filter(|state| state.confirm_delete)
                            .and_then(|state| state.confirm_armed_at);
                        if confirm_delete_ready(armed_at) {
                            on_delete.call(event);
                        } else if let Some(state) = editor.write().as_mut() {
                            state.confirm_delete = true;
                            state.confirm_armed_at = Some(Instant::now());
                        }
                    },
                    if state.confirm_delete {
                        "Confirmer la suppression"
                    } else {
                        "Supprimer la ligne"
                    }
                }
                if state.confirm_delete {
                    span { class: "visually-hidden", role: "status",
                        "Appuyez à nouveau pour confirmer la suppression."
                    }
                }
            }
        }
    }
}

/// Any field edit also resets the pending deletion confirmation.
fn update_editor(
    mut editor: Signal<Option<LineEditorState>>,
    mutate: impl FnOnce(&mut LineEditorState),
) {
    if let Some(state) = editor.write().as_mut() {
        mutate(state);
    }
    disarm_delete(editor);
}

/// Non-delete actions (save, move) reset the armed deletion, so the next
/// delete tap always starts a fresh two-tap sequence.
fn disarm_delete(mut editor: Signal<Option<LineEditorState>>) {
    if let Some(state) = editor.write().as_mut() {
        state.confirm_delete = false;
        state.confirm_armed_at = None;
    }
}

/// The confirming tap is only accepted once the arming tap is at least
/// `DELETE_CONFIRM_MIN_DELAY` old.
fn confirm_delete_ready(armed_at: Option<Instant>) -> bool {
    armed_at.is_some_and(|armed_at| armed_at.elapsed() >= DELETE_CONFIRM_MIN_DELAY)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{DELETE_CONFIRM_MIN_DELAY, confirm_delete_ready};

    #[test]
    fn confirm_delete_requires_an_armed_state() {
        assert!(!confirm_delete_ready(None));
    }

    #[test]
    fn confirm_delete_rejects_taps_inside_the_double_tap_window() {
        assert!(!confirm_delete_ready(Some(Instant::now())));
        let just_armed = Instant::now() - (DELETE_CONFIRM_MIN_DELAY / 2);
        assert!(!confirm_delete_ready(Some(just_armed)));
    }

    #[test]
    fn confirm_delete_accepts_taps_after_the_delay() {
        let armed_earlier = Instant::now() - DELETE_CONFIRM_MIN_DELAY - Duration::from_millis(1);
        assert!(confirm_delete_ready(Some(armed_earlier)));
    }
}
