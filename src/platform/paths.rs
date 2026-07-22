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

pub fn exports_dir() -> Result<PathBuf, PathError> {
    Ok(app_files_dir()?.join("exports"))
}

pub fn database_path() -> Result<PathBuf, PathError> {
    database_path_from(app_files_dir()?)
}

fn database_path_from(directory: PathBuf) -> Result<PathBuf, PathError> {
    std::fs::create_dir_all(&directory)?;
    Ok(directory.join("devis-factures.sqlite3"))
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
    use super::database_path_from;

    #[test]
    fn database_path_creates_its_parent_directory() {
        let root = tempfile::tempdir().expect("create temp directory");
        let directory = root.path().join("private");

        let path = database_path_from(directory.clone()).expect("prepare database path");

        assert!(directory.is_dir());
        assert_eq!(path, directory.join("devis-factures.sqlite3"));
    }
}
