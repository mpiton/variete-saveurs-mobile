use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PathError {
    #[cfg(target_os = "android")]
    #[error("Impossible d'accéder au stockage privé de l'application.")]
    PrivateStorage,
}

pub fn exports_dir() -> Result<PathBuf, PathError> {
    Ok(app_files_dir()?.join("exports"))
}

#[expect(
    dead_code,
    reason = "database initialization lands before app state in this sprint"
)]
pub fn database_path() -> Result<PathBuf, PathError> {
    Ok(app_files_dir()?.join("devis-factures.sqlite3"))
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
