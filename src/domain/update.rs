//! Self-update check (issue 35). The app ships as a direct APK, never through
//! the Play Store, so nothing tells the gérante a fix exists — a correction
//! only reached her phone if someone came by with a cable. The published
//! GitHub releases are the manifest: the APK is already attached to each one
//! (CLAUDE.md §Release, étape 8) and the repository is public, so the check
//! needs no token — one secret less to protect than any other distribution
//! point would have cost.
//!
//! Pure by design (ARCHI §2): the HTTP call and the Android install live in
//! `platform::update`. Everything decided here — is that tag newer, is that
//! asset installable — is decided without a network.

use serde::Deserialize;
use thiserror::Error;

/// Version this binary was built from, which is also what the release process
/// tags (`vX.Y.Z`, CLAUDE.md §Release étape 8). The two are compared directly,
/// so a release tagged out of step with `Cargo.toml` is simply not seen.
pub const fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Release assets are served from `github.com` and redirect to storage from
/// there. Refusing anything else keeps a malformed — or tampered — manifest
/// from pointing the download at an arbitrary host: this file ends up in front
/// of the Android installer, so its origin is not a detail.
const TRUSTED_ASSET_PREFIX: &str = "https://github.com/";

/// A published release that is newer than the running app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableUpdate {
    /// Normalized `X.Y.Z`, without the tag's `v` — this is shown on screen.
    pub version: String,
    pub apk_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UpdateError {
    #[error(
        "Impossible de contacter GitHub pour vérifier les mises à jour. Vérifiez votre connexion."
    )]
    Network,
    #[error("Réponse inattendue de GitHub, réessayez plus tard.")]
    Malformed,
    #[error("La dernière version publiée ne contient pas d'application à installer.")]
    NoApk,
    #[error("Le fichier proposé ne vient pas de GitHub : téléchargement annulé.")]
    UntrustedAsset,
    #[error("Le téléchargement de la mise à jour a échoué. Réessayez.")]
    Download,
    #[error("Impossible d'enregistrer la mise à jour sur le téléphone.")]
    Storage,
    /// The last two are each other's mirror: on the phone the install can
    /// fail, off it the same calls answer `Unsupported` before they get near
    /// an Intent. Whichever target is being built, one of them is unreachable
    /// — and the build refuses unreachable code (`clippy -D warnings`).
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    #[error("Impossible d'ouvrir l'installation de la mise à jour.")]
    Install,
    #[cfg_attr(target_os = "android", allow(dead_code))]
    #[error("La mise à jour n'est disponible que sur Android.")]
    Unsupported,
}

/// `/releases/latest` already excludes drafts and pre-releases, so what comes
/// back is the one release to compare against.
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Reads the `/releases/latest` body and answers the only question the screen
/// asks: is there something newer to install, and where is it? `Ok(None)` is
/// « à jour » — the common case, and not an error.
pub fn newer_release(body: &str, current: &str) -> Result<Option<AvailableUpdate>, UpdateError> {
    let release: Release = serde_json::from_str(body).map_err(|_| UpdateError::Malformed)?;
    let latest = parse_version(&release.tag_name).ok_or(UpdateError::Malformed)?;
    let running = parse_version(current).ok_or(UpdateError::Malformed)?;
    if latest <= running {
        return Ok(None);
    }

    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name.to_ascii_lowercase().ends_with(".apk"))
        .ok_or(UpdateError::NoApk)?;
    if !asset.browser_download_url.starts_with(TRUSTED_ASSET_PREFIX) {
        return Err(UpdateError::UntrustedAsset);
    }

    Ok(Some(AvailableUpdate {
        version: format!("{}.{}.{}", latest[0], latest[1], latest[2]),
        apk_url: asset.browser_download_url,
    }))
}

/// `v0.2.0` and `0.2.0` both give `[0, 2, 0]`; anything else gives `None`.
/// Exactly three numeric components: the release process tags `vX.Y.Z` and
/// nothing else, and a tag this function cannot read must stop the check
/// rather than be guessed at — comparing guesses is how an app talks itself
/// into installing a downgrade. Component-wise ordering also keeps 0.10.0
/// above 0.9.0, which a string comparison would invert.
fn parse_version(value: &str) -> Option<[u32; 3]> {
    let mut parts = value.strip_prefix('v').unwrap_or(value).split('.');
    let mut version = [0_u32; 3];
    for component in &mut version {
        *component = parts.next()?.parse().ok()?;
    }
    parts.next().is_none().then_some(version)
}

#[cfg(test)]
mod tests {
    use super::{AvailableUpdate, UpdateError, current_version, newer_release, parse_version};

    fn release(tag: &str, assets: &str) -> String {
        format!(r#"{{"tag_name":"{tag}","assets":[{assets}]}}"#)
    }

    fn apk(name: &str) -> String {
        format!(
            r#"{{"name":"{name}","browser_download_url":"https://github.com/mpiton/variete-saveurs-mobile/releases/download/v0.2.0/{name}"}}"#
        )
    }

    #[test]
    fn a_newer_tag_offers_its_apk() {
        let body = release("v0.2.0", &apk("devis-factures.apk"));

        let update = newer_release(&body, "0.1.0")
            .expect("well-formed release")
            .expect("0.2.0 is newer than 0.1.0");

        assert_eq!(
            update,
            AvailableUpdate {
                version: "0.2.0".to_string(),
                apk_url:
                    "https://github.com/mpiton/variete-saveurs-mobile/releases/download/v0.2.0/devis-factures.apk"
                        .to_string(),
            }
        );
    }

    #[test]
    fn the_running_version_and_older_ones_are_up_to_date() {
        let body = release("v0.2.0", &apk("app.apk"));

        assert_eq!(newer_release(&body, "0.2.0"), Ok(None));
        assert_eq!(newer_release(&body, "0.3.0"), Ok(None));
        // A patch below still counts as behind.
        assert!(
            newer_release(&release("v0.2.1", &apk("app.apk")), "0.2.0")
                .expect("well-formed release")
                .is_some()
        );
    }

    /// The tag that would break a string comparison: 0.10.0 must beat 0.9.0.
    #[test]
    fn versions_compare_by_number_not_by_text() {
        assert!(
            newer_release(&release("v0.10.0", &apk("app.apk")), "0.9.0")
                .expect("well-formed release")
                .is_some()
        );
        assert_eq!(
            newer_release(&release("v0.9.0", &apk("app.apk")), "0.10.0"),
            Ok(None)
        );
    }

    #[test]
    fn a_tag_without_the_v_prefix_is_read_the_same_way() {
        assert!(
            newer_release(&release("0.2.0", &apk("app.apk")), "0.1.0")
                .expect("well-formed release")
                .is_some()
        );
    }

    #[test]
    fn a_release_without_an_apk_cannot_be_installed() {
        let sources = r#"{"name":"sources.zip","browser_download_url":"https://github.com/mpiton/variete-saveurs-mobile/x.zip"}"#;

        assert_eq!(
            newer_release(&release("v0.2.0", sources), "0.1.0"),
            Err(UpdateError::NoApk)
        );
        assert_eq!(
            newer_release(&release("v0.2.0", ""), "0.1.0"),
            Err(UpdateError::NoApk)
        );
    }

    /// Case is not a promise the API makes; `.APK` is still an APK.
    #[test]
    fn the_apk_asset_is_found_whatever_its_case() {
        assert!(
            newer_release(&release("v0.2.0", &apk("Devis-Factures.APK")), "0.1.0")
                .expect("well-formed release")
                .is_some()
        );
    }

    #[test]
    fn an_asset_hosted_elsewhere_is_refused() {
        let elsewhere =
            r#"{"name":"app.apk","browser_download_url":"https://exemple.invalid/app.apk"}"#;

        assert_eq!(
            newer_release(&release("v0.2.0", elsewhere), "0.1.0"),
            Err(UpdateError::UntrustedAsset)
        );
    }

    #[test]
    fn an_unreadable_body_or_tag_stops_the_check() {
        assert_eq!(
            newer_release("<html>502</html>", "0.1.0"),
            Err(UpdateError::Malformed)
        );
        assert_eq!(newer_release("{}", "0.1.0"), Err(UpdateError::Malformed));
        assert_eq!(
            newer_release(&release("nightly", &apk("app.apk")), "0.1.0"),
            Err(UpdateError::Malformed)
        );
        // An unreadable *running* version is just as disqualifying.
        assert_eq!(
            newer_release(&release("v0.2.0", &apk("app.apk")), "inconnue"),
            Err(UpdateError::Malformed)
        );
    }

    #[test]
    fn version_parsing_wants_exactly_three_numbers() {
        assert_eq!(parse_version("1.2.3"), Some([1, 2, 3]));
        assert_eq!(parse_version("v10.0.42"), Some([10, 0, 42]));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("1.2.3-beta"), None);
        assert_eq!(parse_version("v"), None);
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("-1.0.0"), None);
    }

    /// The check compares this against the release tag, so it has to be a
    /// version the comparison can read at all.
    #[test]
    fn the_running_version_is_readable_by_the_check() {
        assert!(parse_version(current_version()).is_some());
    }
}
