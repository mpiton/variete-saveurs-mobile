//! Settings screen (DESIGN.md §5): the one-time Brevo email configuration —
//! API key, sender address and name (ARCHI.md §3 `settings`, ADR 0002). The
//! key is write-only: once stored it shows as « configurée » and is never
//! displayed again; typing a new one replaces it. It can be revealed while
//! being typed, and only then (issue 34) — a Brevo key is long and opaque, and
//! without that the first sign of a typo is a refused send in front of a
//! client. Without configuration the app keeps working — only email sending
//! stays gated off (task 25).
//!
//! The screen also carries the update path (issue 35): the app ships as a
//! direct APK, so nothing else would ever tell her a fix exists.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use dioxus::prelude::*;
use tokio::time::sleep;

use crate::{
    domain::{
        settings::{EmailSettings, load_email_settings, save_email_settings},
        update::{AvailableUpdate, current_version},
        validation::{plausible_email, validate_email_settings},
    },
    platform::update::{
        can_install, check_for_update, download_apk, install_apk, open_install_permission_settings,
    },
};

use super::{
    app::DatabaseContext,
    components::{Button, ButtonVariant, ErrorBlock, OutlinedField, Snackbar},
    issue::write_from_worker,
};

const NOTICE_DURATION: Duration = Duration::from_secs(4);

/// Raw form state owned by the screen. The key field only ever holds a
/// candidate replacement: the stored key is never read back into an input.
/// No Debug derive — `new_api_key` must never reach the logs.
#[derive(Clone, PartialEq)]
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

impl Default for SettingsForm {
    /// The key starts revealed. Masked, the field is a `password` input, and
    /// OEM keyboards answer that with a « clavier sécurisé » whose clipboard is
    /// disabled — leaving a Brevo key to be typed out by hand, which nobody is
    /// going to do. The key arrives from a password manager, by paste. What
    /// `password` used to protect for free, `sensitive` on the field now asks
    /// for explicitly, so masking buys nothing here beyond a shoulder to hide
    /// from — and « Masquer la clé » is still one tap away.
    fn default() -> Self {
        Self {
            sender_name: String::new(),
            sender_email: String::new(),
            new_api_key: String::new(),
            editing_key: false,
            reveal_key: true,
            key_error: None,
            email_error: None,
            save_error: None,
        }
    }
}

impl SettingsForm {
    /// Drop the candidate and its affordances once it has been stored, so the
    /// next edit starts from the same state as the first one.
    fn clear_key_entry(&mut self) {
        self.new_api_key.clear();
        self.editing_key = false;
        self.reveal_key = Self::default().reveal_key;
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
        // The update section stays: it needs no database, and a build that
        // fixes whatever broke the settings query is exactly what she would
        // want to reach from here.
        return rsx! {
            section { class: "screen settings-screen", aria_label: "Réglages",
                ErrorBlock {
                    title: "Chargement impossible".to_string(),
                    message: error,
                }
                UpdateSection {}
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
                    enter_key_hint: "next".to_string(),
                    placeholder: "Variété de Saveurs".to_string(),
                    value: state.sender_name.clone(),
                    oninput: move |event: FormEvent| {
                        form.write().sender_name = event.value();
                    },
                }
                OutlinedField {
                    label: "Adresse email de l’expéditeur".to_string(),
                    name: "sender-email".to_string(),
                    enter_key_hint: "next".to_string(),
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
                        // hands back every text-assistance surface `password`
                        // suppressed — the same stores the line above keeps
                        // the key out of.
                        sensitive: true,
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
            // After « Enregistrer »: that button belongs to the email form
            // above it, and the update path is a separate errand.
            UpdateSection {}
            if let Some(message) = notice() {
                Snackbar { message }
            }
        }
    }
}

/// Where the update flow stands. Every transition is a worker answering, so
/// the phase lives in a `SyncStorage` signal like the send and share flows.
#[derive(Clone, PartialEq)]
enum UpdatePhase {
    Idle,
    Checking,
    UpToDate,
    /// A newer release is published; the next tap downloads and installs it.
    Available(AvailableUpdate),
    /// Android refuses installs from this app until she grants it, once.
    PermissionNeeded(AvailableUpdate),
    Installing(AvailableUpdate),
    /// The system installer has the APK and owns the screen from here.
    Handed,
    Failed(String),
}

/// The update path (issue 35). Self-contained — its own signal, no database —
/// so the screen can offer it even when the settings query failed.
#[component]
fn UpdateSection() -> Element {
    let phase = use_signal_sync(|| UpdatePhase::Idle);
    let state = phase.read().clone();

    // Available and Installing show the same button; keeping the update in
    // both phases lets it stay labelled with the version while it downloads.
    let pending = match &state {
        UpdatePhase::Available(update) | UpdatePhase::Installing(update) => Some(update.clone()),
        _ => None,
    };
    let status = match &state {
        UpdatePhase::Idle | UpdatePhase::Failed(_) => None,
        UpdatePhase::Checking => Some("Vérification en cours…".to_string()),
        UpdatePhase::UpToDate => Some("Vous avez la dernière version.".to_string()),
        UpdatePhase::Available(update) => {
            Some(format!("Version {} disponible.", update.version))
        }
        UpdatePhase::PermissionNeeded(_) => Some(
            "Android demande votre autorisation avant d’installer une application hors Play Store. Accordez-la, revenez ici, puis relancez l’installation."
                .to_string(),
        ),
        UpdatePhase::Installing(_) => Some("Téléchargement en cours, restez sur cet écran…".to_string()),
        UpdatePhase::Handed => Some(
            "L’installation prend la suite. Suivez ce qu’affiche Android, puis rouvrez l’application."
                .to_string(),
        ),
    };

    // One action per phase, built here rather than in the markup: three
    // mutually exclusive buttons nest badly inside RSX.
    let action = if let UpdatePhase::PermissionNeeded(update) = state.clone() {
        rsx! {
            Button {
                label: "Ouvrir l’autorisation Android".to_string(),
                variant: ButtonVariant::Tonal,
                onclick: move |_| grant_install_permission(phase, update.clone()),
            }
        }
    } else if let Some(update) = pending {
        let version = update.version.clone();
        rsx! {
            Button {
                label: "Télécharger et installer la version {version}",
                loading: matches!(state, UpdatePhase::Installing(_)),
                onclick: move |_| start_install(phase, update.clone()),
            }
        }
    } else {
        // `Handed` lands here too, on purpose: if she dismissed the installer,
        // this is the way back to the offer. Nothing else on the screen would
        // give her one short of leaving Réglages and coming back.
        rsx! {
            Button {
                label: "Vérifier les mises à jour".to_string(),
                variant: ButtonVariant::Tonal,
                loading: matches!(state, UpdatePhase::Checking),
                onclick: move |_| start_check(phase),
            }
        }
    };

    rsx! {
        section { class: "form-section settings-update", aria_label: "Mise à jour de l’application",
            p { class: "settings-update__version", "Version installée : {current_version()}" }
            // The line that has to survive every redesign of this screen: a
            // mise à jour keeps her accounting, uninstalling destroys it.
            p { class: "settings-update__safety",
                "Une mise à jour conserve vos devis, vos factures et vos exports. Ne désinstallez jamais l’application : c’est la seule chose qui les effacerait."
            }
            if let Some(message) = status {
                p { class: "settings-update__status", role: "status", "{message}" }
            }
            if let UpdatePhase::Failed(message) = state {
                ErrorBlock {
                    title: "Mise à jour impossible".to_string(),
                    message,
                }
            }
            {action}
        }
    }
}

/// Runs a network or JNI job off the UI thread and publishes the phase it
/// returns — the send worker's shape (`compose.rs`), fallible spawn included:
/// a resource-starved OS must not panic the screen.
fn run_update_job(
    phase: Signal<UpdatePhase, SyncStorage>,
    label: &'static str,
    job: impl FnOnce() -> UpdatePhase + Send + 'static,
) {
    let worker = std::thread::Builder::new().spawn(move || {
        let next = match catch_unwind(AssertUnwindSafe(job)) {
            Ok(next) => next,
            Err(payload) => {
                eprintln!("{label} panicked: {payload:?}");
                UpdatePhase::Failed(
                    "Échec inattendu de la mise à jour (détail dans les logs).".to_string(),
                )
            }
        };
        write_from_worker(phase, |current| *current = next);
    });
    if let Err(error) = worker {
        eprintln!("{label} worker could not start: {error}");
        write_from_worker(phase, |current| {
            *current = UpdatePhase::Failed("Impossible de démarrer la mise à jour.".to_string());
        });
    }
}

/// A live job is left alone: the button is inert while loading, and a second
/// worker would race the first one's phase.
fn busy(phase: &UpdatePhase) -> bool {
    matches!(phase, UpdatePhase::Checking | UpdatePhase::Installing(_))
}

fn start_check(mut phase: Signal<UpdatePhase, SyncStorage>) {
    if busy(&phase.read()) {
        return;
    }
    *phase.write() = UpdatePhase::Checking;
    run_update_job(phase, "Update check", || match check_for_update() {
        Ok(Some(update)) => UpdatePhase::Available(update),
        Ok(None) => UpdatePhase::UpToDate,
        Err(error) => UpdatePhase::Failed(error.to_string()),
    });
}

fn start_install(mut phase: Signal<UpdatePhase, SyncStorage>, update: AvailableUpdate) {
    if busy(&phase.read()) {
        return;
    }
    *phase.write() = UpdatePhase::Installing(update.clone());
    run_update_job(phase, "Update install", move || {
        // Asked before the download, not after: discovering Android's refusal
        // once tens of megabytes are spent is the parcours issue 35 rules out.
        if !can_install() {
            return UpdatePhase::PermissionNeeded(update);
        }
        match download_apk(&update).and_then(|path| install_apk(&path)) {
            Ok(()) => UpdatePhase::Handed,
            Err(error) => UpdatePhase::Failed(error.to_string()),
        }
    });
}

/// Android reports nothing back when she leaves that screen, so the flow just
/// re-offers the install: the next tap re-reads the permission.
fn grant_install_permission(phase: Signal<UpdatePhase, SyncStorage>, update: AvailableUpdate) {
    run_update_job(phase, "Install permission screen", move || {
        match open_install_permission_settings() {
            Ok(()) => UpdatePhase::Available(update),
            Err(error) => UpdatePhase::Failed(error.to_string()),
        }
    });
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
    use crate::domain::update::AvailableUpdate;

    use super::{SettingsForm, UpdatePhase, busy, key_input_type};

    #[test]
    fn the_key_field_shows_its_content_until_masking_is_asked() {
        assert_eq!(key_input_type(true), "text");
        assert_eq!(key_input_type(false), "password");
    }

    /// A `password` input makes OEM keyboards refuse the clipboard, and the key
    /// is pasted from a password manager. Nothing may quietly put it back.
    #[test]
    fn the_key_can_be_pasted_without_touching_anything_first() {
        assert_eq!(
            key_input_type(SettingsForm::default().reveal_key),
            "text",
            "the key field must not open as a password input"
        );
    }

    #[test]
    fn a_stored_key_leaves_no_candidate_on_screen() {
        let mut form = SettingsForm {
            new_api_key: "xkeysib-candidate".to_string(),
            editing_key: true,
            ..SettingsForm::default()
        };

        form.clear_key_entry();

        assert!(form.new_api_key.is_empty());
        assert!(!form.editing_key);
        assert_eq!(form.reveal_key, SettingsForm::default().reveal_key);
    }

    /// The double-tap guard: only a running job blocks a new one. A phase left
    /// out of `busy` would let a second worker spawn behind the first and race
    /// it to the terminal state — two stacked downloads, or two installers.
    #[test]
    fn only_a_running_job_blocks_the_next_one() {
        let update = AvailableUpdate {
            version: "0.2.0".to_string(),
            apk_url: "https://github.com/x/y/releases/download/v0.2.0/app.apk".to_string(),
        };

        assert!(busy(&UpdatePhase::Checking));
        assert!(busy(&UpdatePhase::Installing(update.clone())));

        for idle in [
            UpdatePhase::Idle,
            UpdatePhase::UpToDate,
            UpdatePhase::Available(update.clone()),
            UpdatePhase::PermissionNeeded(update),
            UpdatePhase::Handed,
            UpdatePhase::Failed("réseau".to_string()),
        ] {
            assert!(!busy(&idle));
        }
    }
}
