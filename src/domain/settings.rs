//! The `settings` table (ARCHI.md §3): the Brevo email configuration,
//! entered once from the settings screen — never hardcoded, never logged
//! (CLAUDE.md NEVER) — plus the first-launch prompt dismissal.

use rusqlite::{Connection, OptionalExtension as _, params};

use super::email::MailConfig;

/// Settings keys (ARCHI.md §3).
pub const SETTING_BREVO_API_KEY: &str = "brevo_api_key";
pub const SETTING_SENDER_EMAIL: &str = "sender_email";
pub const SETTING_SENDER_NAME: &str = "sender_name";
const SETTING_EMAIL_SETUP_DISMISSED: &str = "email_setup_dismissed";

/// Email sending configuration as stored in `settings` (ADR 0002). Every
/// field stays optional until the gérante completes the settings screen;
/// sending is gated on `is_configured`. The key VALUE never leaves this
/// module outside the send path (CLAUDE.md NEVER): the struct only carries
/// its presence, so no Debug formatting or UI hook can leak it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EmailSettings {
    pub has_api_key: bool,
    pub sender_email: Option<String>,
    pub sender_name: Option<String>,
}

impl EmailSettings {
    /// Sending needs a key and a sender address (ARCHI.md §4 envoi email);
    /// the display name falls back to the address when unset.
    pub fn is_configured(&self) -> bool {
        self.has_api_key
            && self
                .sender_email
                .as_deref()
                .is_some_and(|email| !email.is_empty())
    }
}

pub fn load_email_settings(connection: &Connection) -> rusqlite::Result<EmailSettings> {
    let (api_key, sender_email, sender_name) = read_email_rows(connection)?;
    Ok(EmailSettings {
        has_api_key: api_key.is_some_and(|value| !value.is_empty()),
        sender_email,
        sender_name,
    })
}

/// Loads the raw sending configuration for the mailer, `None` until the
/// settings screen has stored both a key and a sender address (the same
/// rule as `EmailSettings::is_configured`). This is the ONE path where the
/// key value leaves storage — the compose screen loads it right before
/// handing it to the mailer, and `MailConfig`'s redacted `Debug` keeps it
/// out of the logs.
pub fn load_email_credentials(connection: &Connection) -> rusqlite::Result<Option<MailConfig>> {
    let (api_key, sender_email, sender_name) = read_email_rows(connection)?;
    let non_empty = |value: Option<String>| value.filter(|value| !value.is_empty());
    Ok(match (non_empty(api_key), non_empty(sender_email)) {
        (Some(api_key), Some(sender_email)) => Some(MailConfig {
            api_key,
            sender_email,
            sender_name: non_empty(sender_name),
        }),
        _ => None,
    })
}

/// Raw values of the three email settings rows, as stored.
fn read_email_rows(
    connection: &Connection,
) -> rusqlite::Result<(Option<String>, Option<String>, Option<String>)> {
    let mut statement =
        connection.prepare("SELECT key, value FROM settings WHERE key IN (?1, ?2, ?3)")?;
    let rows = statement.query_map(
        params![
            SETTING_BREVO_API_KEY,
            SETTING_SENDER_EMAIL,
            SETTING_SENDER_NAME
        ],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    let mut api_key = None;
    let mut sender_email = None;
    let mut sender_name = None;
    for row in rows {
        let (key, value) = row?;
        if key == SETTING_BREVO_API_KEY {
            api_key = Some(value);
        } else if key == SETTING_SENDER_EMAIL {
            sender_email = Some(value);
        } else if key == SETTING_SENDER_NAME {
            sender_name = Some(value);
        }
    }
    Ok((api_key, sender_email, sender_name))
}

/// Persists the email configuration. `new_api_key` is `Some` only when the
/// gérante typed a replacement: `None` keeps the stored key, which the
/// settings screen never re-displays (« configurée • modifier »). An empty
/// sender name drops the row so the sender falls back to the address.
pub fn save_email_settings(
    connection: &Connection,
    new_api_key: Option<&str>,
    sender_email: &str,
    sender_name: &str,
) -> rusqlite::Result<()> {
    // One transaction: a mid-save failure never leaves a half-written
    // configuration behind a « Enregistrement impossible » message.
    let transaction = connection.unchecked_transaction()?;
    if let Some(key) = new_api_key.map(str::trim).filter(|key| !key.is_empty()) {
        upsert_setting(&transaction, SETTING_BREVO_API_KEY, key)?;
    }
    upsert_setting(&transaction, SETTING_SENDER_EMAIL, sender_email.trim())?;
    let name = sender_name.trim();
    if name.is_empty() {
        transaction.execute(
            "DELETE FROM settings WHERE key = ?1",
            params![SETTING_SENDER_NAME],
        )?;
    } else {
        upsert_setting(&transaction, SETTING_SENDER_NAME, name)?;
    }
    transaction.commit()
}

fn upsert_setting(connection: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn email_setup_dismissed(connection: &Connection) -> rusqlite::Result<bool> {
    let value: Option<String> = connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![SETTING_EMAIL_SETUP_DISMISSED],
            |row| row.get(0),
        )
        .optional()?;
    Ok(value.as_deref() == Some("1"))
}

pub fn dismiss_email_setup(connection: &Connection) -> rusqlite::Result<()> {
    upsert_setting(connection, SETTING_EMAIL_SETUP_DISMISSED, "1")
}

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, params};

    use super::{
        EmailSettings, SETTING_BREVO_API_KEY, dismiss_email_setup, email_setup_dismissed,
        load_email_credentials, load_email_settings, save_email_settings,
    };
    use crate::domain::db::migrate;

    fn initialized_connection() -> (tempfile::NamedTempFile, Connection) {
        let file = tempfile::NamedTempFile::new().expect("temp db file");
        let connection = Connection::open(file.path()).expect("open temp db");
        migrate(&connection).expect("migrate");
        (file, connection)
    }

    #[test]
    fn email_settings_roundtrip_and_key_update() {
        let (_file, connection) = initialized_connection();

        let empty = load_email_settings(&connection).expect("load empty settings");
        assert_eq!(empty, EmailSettings::default());
        assert!(!empty.is_configured());

        save_email_settings(
            &connection,
            Some("key-1"),
            "  contact@variete-saveurs.fr ",
            " Variété de Saveurs ",
        )
        .expect("save settings");
        let saved = load_email_settings(&connection).expect("load saved settings");
        assert!(saved.has_api_key);
        assert_eq!(
            saved.sender_email.as_deref(),
            Some("contact@variete-saveurs.fr")
        );
        assert_eq!(saved.sender_name.as_deref(), Some("Variété de Saveurs"));
        assert!(saved.is_configured());

        // Updating without a new key keeps the stored one (« configurée •
        // modifier »): the UI never re-displays it.
        save_email_settings(&connection, None, "pro@example.fr", "").expect("update without key");
        let updated = load_email_settings(&connection).expect("load updated settings");
        assert!(updated.has_api_key);
        assert_eq!(updated.sender_email.as_deref(), Some("pro@example.fr"));
        assert_eq!(updated.sender_name, None);
        assert!(updated.is_configured());

        // A whitespace-only replacement must not clobber the stored key.
        save_email_settings(&connection, Some("   "), "pro@example.fr", "")
            .expect("whitespace key is ignored");
        let stored_key: String = connection
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![SETTING_BREVO_API_KEY],
                |row| row.get(0),
            )
            .expect("stored key");
        assert_eq!(stored_key, "key-1");
    }

    #[test]
    fn email_credentials_only_surface_when_the_configuration_is_complete() {
        let (_file, connection) = initialized_connection();

        assert!(
            load_email_credentials(&connection)
                .expect("load empty credentials")
                .is_none()
        );

        // Sender without a key: still incomplete, nothing to send with.
        save_email_settings(&connection, None, "pro@example.fr", "").expect("save sender only");
        assert!(
            load_email_credentials(&connection)
                .expect("load sender-only credentials")
                .is_none()
        );

        save_email_settings(
            &connection,
            Some(" secret-key "),
            "pro@example.fr",
            "Variété de Saveurs",
        )
        .expect("save complete settings");
        let credentials = load_email_credentials(&connection)
            .expect("load credentials")
            .expect("complete configuration yields credentials");
        assert_eq!(credentials.api_key, "secret-key");
        assert_eq!(credentials.sender_email, "pro@example.fr");
        assert_eq!(
            credentials.sender_name.as_deref(),
            Some("Variété de Saveurs")
        );
    }

    #[test]
    fn email_setup_dismissal_persists() {
        let (_file, connection) = initialized_connection();

        assert!(!email_setup_dismissed(&connection).expect("default not dismissed"));
        dismiss_email_setup(&connection).expect("dismiss");
        dismiss_email_setup(&connection).expect("dismiss is idempotent");
        assert!(email_setup_dismissed(&connection).expect("dismissed"));
    }
}
