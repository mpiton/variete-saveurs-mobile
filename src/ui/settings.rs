//! Settings screen (DESIGN.md §5): the one-time Brevo email configuration —
//! API key, sender address and name (ARCHI.md §3 `settings`, ADR 0002). The
//! key is write-only: once stored it shows as « configurée » and is never
//! displayed again; typing a new one replaces it. It can be revealed while
//! being typed, and only then (issue 34) — a Brevo key is long and opaque, and
//! without that the first sign of a typo is a refused send in front of a
//! client. Without configuration the app keeps working — only email sending
//! stays gated off (task 25).

use std::time::Duration;

use dioxus::prelude::*;
use tokio::time::sleep;

use crate::domain::{
    settings::{EmailSettings, load_email_settings, save_email_settings},
    validation::{plausible_email, validate_email_settings},
};

use super::{
    app::DatabaseContext,
    components::{Button, ButtonVariant, ErrorBlock, OutlinedField, Snackbar},
};

const NOTICE_DURATION: Duration = Duration::from_secs(4);

/// Raw form state owned by the screen. The key field only ever holds a
/// candidate replacement: the stored key is never read back into an input.
/// No Debug derive — `new_api_key` must never reach the logs.
#[derive(Clone, Default, PartialEq)]
struct SettingsForm {
    sender_name: String,
    sender_email: String,
    new_api_key: String,
    /// Whether the stored key (if any) is being replaced.
    editing_key: bool,
    /// Whether the candidate being typed is shown in clear text.
    reveal_key: bool,
    key_error: Option<String>,
    email_error: Option<String>,
    save_error: Option<String>,
}

impl SettingsForm {
    /// Drop the candidate and both of its affordances once it has been stored:
    /// a revealed field must not survive its own save.
    fn clear_key_entry(&mut self) {
        self.new_api_key.clear();
        self.editing_key = false;
        self.reveal_key = false;
    }
}

/// Revealing only ever exposes the candidate: `new_api_key` is never filled
/// from the store, so the saved key has no path back to the screen.
const fn key_input_type(reveal: bool) -> &'static str {
    if reveal { "text" } else { "password" }
}

#[component]
pub(super) fn Settings() -> Element {
    let database = use_context::<DatabaseContext>();
    let load_database = database.clone();
    // Loaded once on entry: the form is the working copy afterwards, so
    // re-renders from typing never clobber it with a fresh query.
    let initial = use_hook(move || load_settings(&load_database));
    let initial_settings = initial.as_ref().ok();
    let load_error = initial.as_ref().err().cloned();
    let key_saved = use_signal(|| {
        initial_settings
            .map(|settings| settings.has_api_key)
            .unwrap_or(false)
    });
    let mut form = use_signal(|| {
        initial_settings.map_or_else(SettingsForm::default, |settings| SettingsForm {
            sender_name: settings.sender_name.clone().unwrap_or_default(),
            sender_email: settings.sender_email.clone().unwrap_or_default(),
            ..SettingsForm::default()
        })
    });
    let mut notice = use_signal(|| None::<String>);

    // Transient snackbar (DESIGN.md §6): the timer only ever dismisses ITS
    // notice — a newer save survives an older timer.
    use_effect(move || {
        if let Some(expected) = notice() {
            spawn(async move {
                sleep(NOTICE_DURATION).await;
                if notice.peek().as_deref() == Some(expected.as_str()) {
                    notice.set(None);
                }
            });
        }
    });

    if let Some(error) = load_error {
        return rsx! {
            section { class: "screen settings-screen", aria_label: "Réglages",
                ErrorBlock {
                    title: "Chargement impossible".to_string(),
                    message: error,
                }
            }
        };
    }

    let state = form.read().clone();
    let key_field_visible = state.editing_key || !key_saved();

    rsx! {
        section { class: "screen settings-screen", aria_label: "Réglages",
            p {
                "L’envoi par email passe par votre compte Brevo. Sans ces informations, l’envoi est indisponible — le partage et l’export fonctionnent sans."
            }
            section { class: "form-section", aria_label: "Expéditeur",
                OutlinedField {
                    label: "Nom de l’expéditeur (optionnel)".to_string(),
                    name: "sender-name".to_string(),
                    placeholder: "Variété de Saveurs".to_string(),
                    value: state.sender_name.clone(),
                    oninput: move |event: FormEvent| {
                        form.write().sender_name = event.value();
                    },
                }
                OutlinedField {
                    label: "Adresse email de l’expéditeur".to_string(),
                    name: "sender-email".to_string(),
                    input_type: "email".to_string(),
                    input_mode: "email".to_string(),
                    placeholder: "contact@exemple.fr".to_string(),
                    value: state.sender_email.clone(),
                    error: state.email_error.clone(),
                    oninput: move |event: FormEvent| {
                        let mut form = form.write();
                        form.sender_email = event.value();
                        form.email_error = None;
                    },
                }
            }
            section { class: "form-section", aria_label: "Clé API Brevo",
                if key_field_visible {
                    OutlinedField {
                        label: if key_saved() {
                            "Nouvelle clé API Brevo".to_string()
                        } else {
                            "Clé API Brevo".to_string()
                        },
                        name: "brevo-api-key".to_string(),
                        input_type: key_input_type(state.reveal_key).to_string(),
                        // Keep the Android autofill framework away from the
                        // key: it must never leave the app-private store.
                        autocomplete: "off".to_string(),
                        // Revealed, the field is a plain `text` input, which
                        // hands the IME back its suggestion strip and its
                        // personalised learning — the same store the line
                        // above keeps the key out of. Chromium maps this to
                        // TYPE_TEXT_FLAG_NO_SUGGESTIONS.
                        spellcheck: "false".to_string(),
                        placeholder: if key_saved() {
                            "Laisser vide pour conserver la clé actuelle".to_string()
                        } else {
                            String::new()
                        },
                        value: state.new_api_key.clone(),
                        error: state.key_error.clone(),
                        oninput: move |event: FormEvent| {
                            let mut form = form.write();
                            form.new_api_key = event.value();
                            form.key_error = None;
                        },
                    }
                    div { class: "settings-key-reveal",
                        Button {
                            label: if state.reveal_key {
                                "Masquer la clé".to_string()
                            } else {
                                "Afficher la clé".to_string()
                            },
                            variant: ButtonVariant::Text,
                            onclick: move |_| {
                                let mut form = form.write();
                                form.reveal_key = !form.reveal_key;
                            },
                        }
                    }
                } else {
                    div { class: "settings-key-status",
                        p { class: "settings-key-status__label", "Clé API Brevo : configurée" }
                        Button {
                            label: "Modifier".to_string(),
                            variant: ButtonVariant::Tonal,
                            onclick: move |_| form.write().editing_key = true,
                        }
                    }
                }
            }
            if let Some(error) = state.save_error.clone() {
                ErrorBlock {
                    title: "Enregistrement impossible".to_string(),
                    message: error,
                }
            }
            Button {
                label: "Enregistrer".to_string(),
                onclick: move |_| save_settings(&database, form, key_saved, notice),
            }
            if let Some(message) = notice() {
                Snackbar { message }
            }
        }
    }
}

/// Field-level checks mirror the domain gate so mistakes land next to the
/// faulty input, like the catalog sheet; the domain gate runs again before
/// writing.
fn save_settings(
    database: &DatabaseContext,
    mut form: Signal<SettingsForm>,
    mut key_saved: Signal<bool>,
    mut notice: Signal<Option<String>>,
) {
    let mut state = form.read().clone();
    let has_saved_key = key_saved();
    state.key_error = if has_saved_key || !state.new_api_key.trim().is_empty() {
        None
    } else {
        Some("La clé API Brevo est obligatoire.".to_string())
    };
    state.email_error = if plausible_email(&state.sender_email) {
        None
    } else {
        Some("L'adresse email de l'expéditeur semble invalide.".to_string())
    };
    state.save_error = None;
    if state.key_error.is_some() || state.email_error.is_some() {
        form.set(state);
        return;
    }

    let new_key = match state.new_api_key.trim() {
        "" => None,
        key => Some(key.to_string()),
    };
    if let Err(errors) =
        validate_email_settings(new_key.as_deref(), has_saved_key, &state.sender_email)
    {
        state.save_error = Some(errors.join("\n"));
        form.set(state);
        return;
    }
    match persist_settings(
        database,
        new_key.as_deref(),
        &state.sender_email,
        &state.sender_name,
    ) {
        Ok(()) => {
            if new_key.is_some() {
                key_saved.set(true);
            }
            state.clear_key_entry();
            form.set(state);
            notice.set(Some("Réglages enregistrés.".to_string()));
        }
        Err(error) => {
            state.save_error = Some(error);
            form.set(state);
        }
    }
}

fn load_settings(database: &DatabaseContext) -> Result<EmailSettings, String> {
    let database = database.as_ref().map_err(Clone::clone)?;
    let connection = database
        .lock()
        .map_err(|_| "Impossible d’accéder aux données locales.".to_string())?;
    load_email_settings(&connection).map_err(|error| {
        eprintln!("Settings query failed: {error}");
        "Impossible de charger les réglages.".to_string()
    })
}

fn persist_settings(
    database: &DatabaseContext,
    new_api_key: Option<&str>,
    sender_email: &str,
    sender_name: &str,
) -> Result<(), String> {
    let database = database.as_ref().map_err(Clone::clone)?;
    let connection = database
        .lock()
        .map_err(|_| "Impossible d’accéder aux données locales.".to_string())?;
    save_email_settings(&connection, new_api_key, sender_email, sender_name).map_err(|error| {
        // The rusqlite error carries the SQL message and code, never bound
        // values — the API key must stay out of the logs (CLAUDE.md NEVER).
        eprintln!("Settings save failed: {error}");
        "Impossible d’enregistrer les réglages.".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::{SettingsForm, key_input_type};

    #[test]
    fn the_key_field_hides_its_content_until_asked() {
        assert_eq!(key_input_type(false), "password");
        assert_eq!(key_input_type(true), "text");
    }

    #[test]
    fn a_stored_key_leaves_no_revealed_candidate_on_screen() {
        let mut form = SettingsForm {
            new_api_key: "xkeysib-candidate".to_string(),
            editing_key: true,
            reveal_key: true,
            ..SettingsForm::default()
        };

        form.clear_key_entry();

        assert!(form.new_api_key.is_empty());
        assert!(!form.editing_key);
        assert!(!form.reveal_key);
    }
}
