use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PathError {
    #[error("Impossible de préparer le stockage privé de l'application.")]
    CreateDirectory(#[from] std::io::Error),
    #[cfg(target_os = "android")]
    #[error("Impossible d'accéder au stockage privé de l'application.")]
    PrivateStorage,
}

/// Both live directly under `getFilesDir()`, which is what the Android Auto
/// Backup rules in `android/res/xml/` enumerate (ARCHI §6). Renaming either
/// one without editing those rules would silently drop it from the backup,
/// so `backup_rules_cover_stored_data` ties the two together.
const DATABASE_FILE_NAME: &str = "devis-factures.sqlite3";
const EXPORTS_DIR_NAME: &str = "exports";

pub fn exports_dir() -> Result<PathBuf, PathError> {
    Ok(app_files_dir()?.join(EXPORTS_DIR_NAME))
}

pub fn database_path() -> Result<PathBuf, PathError> {
    database_path_from(app_files_dir()?)
}

fn database_path_from(directory: PathBuf) -> Result<PathBuf, PathError> {
    std::fs::create_dir_all(&directory)?;
    Ok(directory.join(DATABASE_FILE_NAME))
}

#[cfg(target_os = "android")]
fn app_files_dir() -> Result<PathBuf, PathError> {
    use jni::JavaVM;
    use jni::objects::{JObject, JString};

    let path = (|| -> jni::errors::Result<PathBuf> {
        let android = ndk_context::android_context();
        let vm = unsafe { JavaVM::from_raw(android.vm().cast()) }?;
        let mut env = vm.attach_current_thread()?;
        let raw_context = unsafe { JObject::from_raw(android.context().cast()) };
        let context = env.new_global_ref(&raw_context)?;
        let directory = env
            .call_method(context.as_obj(), "getFilesDir", "()Ljava/io/File;", &[])?
            .l()?;
        let value = env
            .call_method(directory, "getAbsolutePath", "()Ljava/lang/String;", &[])?
            .l()?;
        let value = JString::from(value);
        Ok(PathBuf::from(String::from(env.get_string(&value)?)))
    })();

    path.map_err(|error| {
        eprintln!("Android filesDir lookup failed: {error}");
        PathError::PrivateStorage
    })
}

#[cfg(not(target_os = "android"))]
fn app_files_dir() -> Result<PathBuf, PathError> {
    Ok(std::env::temp_dir().join("devis-mobile"))
}

#[cfg(test)]
mod tests {
    use super::{DATABASE_FILE_NAME, EXPORTS_DIR_NAME, database_path_from};

    #[test]
    fn database_path_creates_its_parent_directory() {
        let root = tempfile::tempdir().expect("create temp directory");
        let directory = root.path().join("private");

        let path = database_path_from(directory.clone()).expect("prepare database path");

        assert!(directory.is_dir());
        assert_eq!(path, directory.join(DATABASE_FILE_NAME));
    }

    /// A backup that silently stops covering the accounting data is worse than
    /// no backup, so the rules for both Android generations are checked against
    /// the names actually used on disk. Each section is checked on its own:
    /// losing a path from `device-transfer` only would go unnoticed otherwise,
    /// and the data would vanish when she moves to a new phone.
    #[test]
    fn backup_rules_cover_stored_data() {
        const BACKUP_RULES: &str = include_str!("../../android/res/xml/backup_rules.xml");
        const EXTRACTION_RULES: &str =
            include_str!("../../android/res/xml/data_extraction_rules.xml");
        const SECTIONS: [(&str, &str, &str); 3] = [
            ("backup_rules.xml", "full-backup-content", BACKUP_RULES),
            (
                "data_extraction_rules.xml",
                "cloud-backup",
                EXTRACTION_RULES,
            ),
            (
                "data_extraction_rules.xml",
                "device-transfer",
                EXTRACTION_RULES,
            ),
        ];

        for (file, section, rules) in SECTIONS {
            let body = rules
                .split_once(&format!("<{section}>"))
                .and_then(|(_, rest)| rest.split_once(&format!("</{section}>")))
                .map(|(body, _)| body)
                .unwrap_or_else(|| panic!("{file} must declare a <{section}> section"));

            for name in [DATABASE_FILE_NAME, EXPORTS_DIR_NAME] {
                let entry = format!(r#"domain="file" path="{name}""#);
                assert!(
                    body.contains(&entry),
                    "<{section}> in {file} must back up {name} (missing `{entry}`)"
                );
            }
        }
    }
}
