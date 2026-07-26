//! Self-update over the GitHub releases API (issue 35): ask, download, hand
//! the APK to the Android installer. The verdict logic is pure and lives in
//! `domain::update`; what is here is the network and the JNI.
//!
//! Nothing runs at startup. `PRODUCT.md` keeps the réseau off the saisie path,
//! so the check is a button in Réglages and nothing else — an app that stalls
//! on a splash screen because GitHub is slow is worse than one that is a
//! version behind.
//!
//! The install goes through an Intent and the system installer, not
//! `PackageInstaller`: the session API buys split-APK support and precise
//! failure codes that a single-APK sideload has no use for, at the cost of a
//! `BroadcastReceiver` and a `PendingIntent`. The Intent route is the same
//! shape as `share.rs` — build, grant, `startActivity` — and the installer
//! shows its own progress and its own errors.
//!
//! What protects the install is Android's own rule, not a check written here:
//! an APK signed with another key simply will not replace this one (the
//! keystore lives outside the repo, CLAUDE.md §Sécurité). A normal update
//! keeps `/data/data/` — the SQLite base and `exports/` survive it. Nothing on
//! this path may ever suggest uninstalling: that is what would destroy them.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::domain::update::{AvailableUpdate, UpdateError, current_version, newer_release};

use super::paths;

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/mpiton/variete-saveurs-mobile/releases/latest";
/// GitHub answers 403 to an API request without one.
const USER_AGENT: &str = concat!("devis-mobile/", env!("CARGO_PKG_VERSION"));
const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
/// The APK is tens of megabytes over whatever connection the phone has; the
/// check's timeout would abort a download that is merely slow.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
const APK_FILE_NAME: &str = "mise-a-jour.apk";
/// Downloaded under this name and renamed once complete: a connection that
/// drops mid-transfer must not leave something the installer would open and
/// reject as corrupt.
const PARTIAL_FILE_NAME: &str = "mise-a-jour.apk.part";

#[cfg(target_os = "android")]
const AUTHORITY: &str = "fr.variete_saveurs.devis_factures.updates";
#[cfg(target_os = "android")]
const APK_MIME: &str = "application/vnd.android.package-archive";
#[cfg(target_os = "android")]
const ACTION_VIEW: &str = "android.intent.action.VIEW";
/// API 26+ — the per-app « Installer des applications inconnues » screen.
#[cfg(target_os = "android")]
const ACTION_MANAGE_UNKNOWN_APP_SOURCES: &str = "android.settings.MANAGE_UNKNOWN_APP_SOURCES";
#[cfg(target_os = "android")]
const FLAG_GRANT_READ_URI_PERMISSION: i32 = 0x0000_0001;
#[cfg(target_os = "android")]
const FLAG_ACTIVITY_NEW_TASK: i32 = 0x1000_0000;
/// `Build.VERSION_CODES.O`: below it there is no per-app install permission to
/// ask about, only the phone-wide « sources inconnues » toggle.
#[cfg(target_os = "android")]
const ANDROID_O: i32 = 26;

/// Asks GitHub whether a newer release exists. `Ok(None)` means « à jour ».
pub fn check_for_update() -> Result<Option<AvailableUpdate>, UpdateError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|_| UpdateError::Network)?;
    let response = client
        .get(LATEST_RELEASE_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|_| UpdateError::Network)?;
    if !response.status().is_success() {
        // Covers the unauthenticated rate limit as much as an outage; either
        // way the answer is « réessayez plus tard », not a status code.
        eprintln!("Release check refused: HTTP {}", response.status());
        return Err(UpdateError::Network);
    }
    let body = response.text().map_err(|_| UpdateError::Network)?;
    newer_release(&body, current_version())
}

/// Downloads the APK into the app cache and returns where it landed.
pub fn download_apk(update: &AvailableUpdate) -> Result<PathBuf, UpdateError> {
    let directory = paths::updates_dir().map_err(|error| {
        eprintln!("Update directory unavailable: {error}");
        UpdateError::Storage
    })?;
    let target = directory.join(APK_FILE_NAME);
    let partial = directory.join(PARTIAL_FILE_NAME);
    // A previous attempt's file would otherwise be what gets installed if this
    // one fails before the rename.
    let _ = std::fs::remove_file(&target);

    let client = reqwest::blocking::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|_| UpdateError::Download)?;
    // Redirects stay enabled here, unlike the Brevo client: GitHub answers an
    // asset URL with a redirect to short-lived signed storage, and no secret
    // of ours travels on this request to follow it there.
    let mut response = client
        .get(&update.apk_url)
        .send()
        .map_err(|_| UpdateError::Download)?;
    if !response.status().is_success() {
        eprintln!("APK download refused: HTTP {}", response.status());
        return Err(UpdateError::Download);
    }

    // Streamed, not buffered: the APK is tens of megabytes and this runs on a
    // phone.
    let written = (|| -> io::Result<u64> {
        let mut file = std::fs::File::create(&partial)?;
        let written = io::copy(&mut response, &mut file)?;
        file.sync_all()?;
        Ok(written)
    })();
    let written = written.map_err(|error| {
        eprintln!("APK download failed: {error}");
        let _ = std::fs::remove_file(&partial);
        UpdateError::Download
    })?;
    if written == 0 {
        let _ = std::fs::remove_file(&partial);
        return Err(UpdateError::Download);
    }

    std::fs::rename(&partial, &target).map_err(|error| {
        eprintln!("APK rename failed: {error}");
        let _ = std::fs::remove_file(&partial);
        UpdateError::Storage
    })?;
    Ok(target)
}

/// Whether Android would let the app start an install at all. Checked before
/// downloading: discovering the refusal after tens of megabytes, with no way
/// to act on it, is the parcours the issue asks to avoid.
pub fn can_install() -> bool {
    can_install_impl()
}

/// Opens the per-app « Installer des applications inconnues » screen. Android
/// reports nothing back, so the flow re-reads `can_install` on the next tap.
pub fn open_install_permission_settings() -> Result<(), UpdateError> {
    open_install_permission_settings_impl()
}

/// Hands the APK to the system installer, which owns the confirmation, the
/// progress and any failure from there on.
pub fn install_apk(path: &Path) -> Result<(), UpdateError> {
    install_apk_impl(path)
}

#[cfg(target_os = "android")]
fn can_install_impl() -> bool {
    with_jni(|env, context| {
        if android_sdk_int(env)? < ANDROID_O {
            // Nothing per-app to grant: the install either works or the system
            // installer explains why, which is the same place the user lands.
            return Ok(true);
        }
        let manager = env
            .call_method(
                context,
                "getPackageManager",
                "()Landroid/content/pm/PackageManager;",
                &[],
            )?
            .l()?;
        env.call_method(manager, "canRequestPackageInstalls", "()Z", &[])?
            .z()
    })
    .unwrap_or_else(|error| {
        eprintln!("Install permission check failed: {error}");
        // Assume it is allowed rather than block the only update path on a
        // JNI hiccup: the installer will say so itself if it is not.
        true
    })
}

#[cfg(target_os = "android")]
fn open_install_permission_settings_impl() -> Result<(), UpdateError> {
    with_jni(|env, context| {
        use jni::objects::JValue;

        let package = env
            .call_method(context, "getPackageName", "()Ljava/lang/String;", &[])?
            .l()?;
        let package: jni::objects::JString = package.into();
        let package = String::from(env.get_string(&package)?);
        let uri = parse_uri(env, &format!("package:{package}"))?;

        let action = env.new_string(ACTION_MANAGE_UNKNOWN_APP_SOURCES)?;
        let intent = env.new_object(
            "android/content/Intent",
            "(Ljava/lang/String;Landroid/net/Uri;)V",
            &[JValue::Object(&action), JValue::Object(&uri)],
        )?;
        env.call_method(
            &intent,
            "addFlags",
            "(I)Landroid/content/Intent;",
            &[JValue::Int(FLAG_ACTIVITY_NEW_TASK)],
        )?;
        env.call_method(
            context,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[JValue::Object(&intent)],
        )?;
        Ok(())
    })
    .map_err(|error| {
        eprintln!("Opening the install permission screen failed: {error}");
        UpdateError::Install
    })
}

#[cfg(target_os = "android")]
fn install_apk_impl(path: &Path) -> Result<(), UpdateError> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(UpdateError::Install)?
        .to_string();

    with_jni(|env, context| {
        use jni::objects::JValue;

        let uri = parse_uri(env, &format!("content://{AUTHORITY}/{name}"))?;
        let action = env.new_string(ACTION_VIEW)?;
        let intent = env.new_object(
            "android/content/Intent",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&action)],
        )?;
        let mime = env.new_string(APK_MIME)?;
        // setDataAndType, not setData then setType: each of those clears the
        // other, and the installer needs both.
        env.call_method(
            &intent,
            "setDataAndType",
            "(Landroid/net/Uri;Ljava/lang/String;)Landroid/content/Intent;",
            &[JValue::Object(&uri), JValue::Object(&mime)],
        )?;
        env.call_method(
            &intent,
            "addFlags",
            "(I)Landroid/content/Intent;",
            &[JValue::Int(
                FLAG_GRANT_READ_URI_PERMISSION | FLAG_ACTIVITY_NEW_TASK,
            )],
        )?;
        env.call_method(
            context,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[JValue::Object(&intent)],
        )?;
        Ok(())
    })
    .map_err(|error| {
        eprintln!("Starting the APK install failed: {error}");
        UpdateError::Install
    })
}

/// Attach, run, and never leave a pending Java exception behind — it turns
/// fatal when the native thread detaches, which would take the app down
/// instead of showing the French message. Same guard as `share.rs`.
#[cfg(target_os = "android")]
fn with_jni<T>(
    body: impl FnOnce(&mut jni::AttachGuard, &jni::objects::JObject) -> jni::errors::Result<T>,
) -> jni::errors::Result<T> {
    use jni::JavaVM;
    use jni::objects::JObject;

    let android = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(android.vm().cast()) }?;
    let mut env = vm.attach_current_thread()?;
    let raw_context = unsafe { JObject::from_raw(android.context().cast()) };
    let context = env.new_global_ref(&raw_context)?;
    let result = body(&mut env, context.as_obj());
    if result.is_err() {
        let _ = env.exception_clear();
    }
    result
}

#[cfg(target_os = "android")]
fn android_sdk_int(env: &mut jni::AttachGuard) -> jni::errors::Result<i32> {
    env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
        .i()
}

#[cfg(target_os = "android")]
fn parse_uri<'a>(
    env: &mut jni::AttachGuard<'a>,
    value: &str,
) -> jni::errors::Result<jni::objects::JObject<'a>> {
    use jni::objects::JValue;

    let value = env.new_string(value)?;
    env.call_static_method(
        "android/net/Uri",
        "parse",
        "(Ljava/lang/String;)Landroid/net/Uri;",
        &[JValue::Object(&value)],
    )?
    .l()
}

#[cfg(not(target_os = "android"))]
const fn can_install_impl() -> bool {
    false
}

#[cfg(not(target_os = "android"))]
const fn open_install_permission_settings_impl() -> Result<(), UpdateError> {
    Err(UpdateError::Unsupported)
}

#[cfg(not(target_os = "android"))]
const fn install_apk_impl(_path: &Path) -> Result<(), UpdateError> {
    Err(UpdateError::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The provider authority the install URI is built from has to be the one
    /// the manifest declares, or the installer gets a URI nothing answers.
    #[cfg(target_os = "android")]
    #[test]
    fn the_update_authority_matches_the_manifest() {
        const MANIFEST: &str = include_str!("../../android/AndroidManifest.xml");
        assert!(MANIFEST.contains(&format!(r#"android:authorities="{AUTHORITY}""#)));
    }

    /// Installing needs `REQUEST_INSTALL_PACKAGES` declared: without it
    /// `canRequestPackageInstalls` throws instead of answering.
    #[test]
    fn the_manifest_declares_the_install_permission() {
        const MANIFEST: &str = include_str!("../../android/AndroidManifest.xml");
        assert!(
            MANIFEST.contains("android.permission.REQUEST_INSTALL_PACKAGES"),
            "AndroidManifest.xml must declare REQUEST_INSTALL_PACKAGES"
        );
    }

    #[cfg(not(target_os = "android"))]
    #[test]
    fn installing_is_unsupported_off_android() {
        assert!(!can_install());
        assert_eq!(
            open_install_permission_settings(),
            Err(UpdateError::Unsupported)
        );
        assert_eq!(
            install_apk(Path::new("mise-a-jour.apk")),
            Err(UpdateError::Unsupported)
        );
    }
}
