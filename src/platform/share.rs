use std::path::Path;

use thiserror::Error;

use super::paths;

#[cfg(target_os = "android")]
const AUTHORITY: &str = "fr.variete_saveurs.devis_factures.exports";
#[cfg(target_os = "android")]
const ACTION_SEND: &str = "android.intent.action.SEND";
#[cfg(target_os = "android")]
const EXTRA_STREAM: &str = "android.intent.extra.STREAM";
#[cfg(target_os = "android")]
const FLAG_GRANT_READ_URI_PERMISSION: i32 = 0x00000001;
#[cfg(target_os = "android")]
const FLAG_ACTIVITY_NEW_TASK: i32 = 0x10000000;

#[derive(Debug, Error)]
pub enum ShareError {
    #[error("Impossible de localiser le fichier à partager.")]
    Path(#[from] paths::PathError),
    #[cfg(target_os = "android")]
    #[error("Impossible d'ouvrir le partage du fichier.")]
    Android(#[source] jni::errors::Error),
    #[error("Le partage n'est disponible que sur Android.")]
    Unsupported,
}

/// Opens the system share sheet for a file stored under `exports/`.
pub fn share_file(path: &Path) -> Result<(), ShareError> {
    share_file_impl(path)
}

#[cfg(target_os = "android")]
fn share_file_impl(path: &Path) -> Result<(), ShareError> {
    use jni::JavaVM;

    let exports = paths::exports_dir()?;
    let relative = path
        .strip_prefix(&exports)
        .map_err(|_| ShareError::Unsupported)?
        .to_string_lossy()
        .replace('\\', "/");
    let uri = format!("content://{AUTHORITY}/{relative}");
    let mime = match path.extension().and_then(|ext| ext.to_str()) {
        Some("pdf") => "application/pdf",
        Some("html") => "text/html",
        _ => "application/octet-stream",
    };

    let android = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(android.vm().cast()) }.map_err(ShareError::Android)?;
    let mut env = vm.attach_current_thread().map_err(ShareError::Android)?;
    let result = share_via_intent(&mut env, &uri, mime);
    if result.is_err() {
        // A pending Java exception becomes fatal when the native thread
        // detaches; clear it so the error surfaces as a French message only.
        let _ = env.exception_clear();
    }
    result.map_err(ShareError::Android)
}

#[cfg(target_os = "android")]
fn share_via_intent(env: &mut jni::AttachGuard, uri: &str, mime: &str) -> jni::errors::Result<()> {
    use jni::objects::{JObject, JValue};

    let android = ndk_context::android_context();
    let raw_context = unsafe { JObject::from_raw(android.context().cast()) };
    let context = env.new_global_ref(&raw_context)?;

    let uri_string = env.new_string(uri)?;
    let content_uri = env
        .call_static_method(
            "android/net/Uri",
            "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&uri_string)],
        )?
        .l()?;

    let action = env.new_string(ACTION_SEND)?;
    let intent = env.new_object(
        "android/content/Intent",
        "(Ljava/lang/String;)V",
        &[JValue::Object(&action)],
    )?;
    let mime_type = env.new_string(mime)?;
    env.call_method(
        &intent,
        "setType",
        "(Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&mime_type)],
    )?;
    let extra = env.new_string(EXTRA_STREAM)?;
    env.call_method(
        &intent,
        "putExtra",
        "(Ljava/lang/String;Landroid/os/Parcelable;)Landroid/content/Intent;",
        &[JValue::Object(&extra), JValue::Object(&content_uri)],
    )?;
    env.call_method(
        &intent,
        "addFlags",
        "(I)Landroid/content/Intent;",
        &[JValue::Int(FLAG_GRANT_READ_URI_PERMISSION)],
    )?;

    let title = env.new_string("Partager le fichier")?;
    let chooser = env
        .call_static_method(
            "android/content/Intent",
            "createChooser",
            "(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;",
            &[JValue::Object(&intent), JValue::Object(&title)],
        )?
        .l()?;
    env.call_method(
        &chooser,
        "addFlags",
        "(I)Landroid/content/Intent;",
        &[JValue::Int(FLAG_ACTIVITY_NEW_TASK)],
    )?;
    env.call_method(
        context.as_obj(),
        "startActivity",
        "(Landroid/content/Intent;)V",
        &[JValue::Object(&chooser)],
    )?;
    Ok(())
}

#[cfg(not(target_os = "android"))]
fn share_file_impl(_path: &Path) -> Result<(), ShareError> {
    Err(ShareError::Unsupported)
}
