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

#[cfg(target_os = "android")]
fn app_files_dir() -> Result<PathBuf, PathError> {
    use manganis::jni::objects::JString;

    manganis::android::with_activity(|env, activity| {
        let path = env
            .call_method(activity, "getFilesDir", "()Ljava/io/File;", &[])
            .and_then(|value| value.l())
            .and_then(|directory| {
                env.call_method(directory, "getAbsolutePath", "()Ljava/lang/String;", &[])
            })
            .and_then(|value| value.l())
            .and_then(|value| {
                let value = JString::from(value);
                env.get_string(&value).map(String::from)
            });

        match path {
            Ok(path) => Some(PathBuf::from(path)),
            Err(error) => {
                eprintln!("Android filesDir lookup failed: {error}");
                None
            }
        }
    })
    .ok_or(PathError::PrivateStorage)
}

#[cfg(not(target_os = "android"))]
fn app_files_dir() -> Result<PathBuf, PathError> {
    Ok(std::env::temp_dir().join("devis-mobile"))
}
