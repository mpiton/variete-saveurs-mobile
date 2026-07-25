//! DESIGN §8: the launcher icon is derived from `templates/logo.png` — the VS
//! monogram in Crème Vitrine over a flat Rouge Enseigne background. The assets
//! are generated once by `tools/gen-launcher-icon.py`; this guards the shape,
//! the palette and the wiring so a regeneration can't drift.

use std::{fs, path::Path};

const ROUGE_ENSEIGNE: [u8; 4] = [0xC0, 0x18, 0x2B, 0xFF];
const CREME_VITRINE: [u8; 3] = [0xF6, 0xF0, 0xE2];
/// Adaptive icons only guarantee the central 66% of the layer stays unmasked.
const SAFE_ZONE: f64 = 0.6667;
const DENSITIES: [(&str, u32, u32); 5] = [
    ("mdpi", 108, 48),
    ("hdpi", 162, 72),
    ("xhdpi", 216, 96),
    ("xxhdpi", 324, 144),
    ("xxxhdpi", 432, 192),
];

fn project_file(path: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn icon(path: &str) -> image::RgbaImage {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    image::open(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
        .to_rgba8()
}

#[test]
fn every_density_is_generated_at_the_expected_size() {
    for (density, foreground, legacy) in DENSITIES {
        let layer = icon(&format!(
            "android/res/drawable-{density}/ic_launcher_vs_foreground.png"
        ));
        assert_eq!(layer.dimensions(), (foreground, foreground), "{density}");

        for name in ["ic_launcher_vs", "ic_launcher_vs_round"] {
            let fallback = icon(&format!("android/res/mipmap-{density}/{name}.png"));
            assert_eq!(fallback.dimensions(), (legacy, legacy), "{density}/{name}");
        }
    }
}

#[test]
fn the_monogram_fits_the_adaptive_safe_zone() {
    for (density, size, _) in DENSITIES {
        let layer = icon(&format!(
            "android/res/drawable-{density}/ic_launcher_vs_foreground.png"
        ));
        let center = f64::from(size) / 2.0;
        let radius = center * SAFE_ZONE;
        let reach = layer
            .enumerate_pixels()
            // Below 3% opacity the anti-aliasing fringe isn't the motif anymore.
            .filter(|(_, _, pixel)| pixel[3] > 8)
            .map(|(x, y, _)| (f64::from(x) + 0.5 - center).hypot(f64::from(y) + 0.5 - center))
            .fold(f64::MIN, f64::max);

        assert!(reach > 0.0, "{density}: empty foreground layer");
        assert!(
            reach <= radius,
            "{density}: the monogram reaches {reach:.1}px from center, past the {radius:.1}px safe zone"
        );
    }
}

#[test]
fn the_icon_only_uses_the_brand_colors() {
    let layer = icon("android/res/drawable-xxxhdpi/ic_launcher_vs_foreground.png");
    for pixel in layer.pixels().filter(|pixel| pixel[3] > 0) {
        assert_eq!(
            [pixel[0], pixel[1], pixel[2]],
            CREME_VITRINE,
            "the monogram must stay Crème Vitrine"
        );
    }

    for name in ["ic_launcher_vs", "ic_launcher_vs_round"] {
        let fallback = icon(&format!("android/res/mipmap-xxxhdpi/{name}.png"));
        let (width, height) = fallback.dimensions();
        assert_eq!(
            fallback.get_pixel(width / 2, height / 12).0,
            ROUGE_ENSEIGNE,
            "{name} must sit on a Rouge Enseigne plate"
        );
    }

    let background = project_file("android/res/drawable/ic_launcher_vs_background.xml");
    assert!(background.contains(r##"android:color="#C0182B""##));
}

#[test]
fn the_manifest_points_at_the_adaptive_icon() {
    let manifest = project_file("android/AndroidManifest.xml");
    assert!(manifest.contains(r#"android:icon="@mipmap/ic_launcher_vs""#));
    assert!(manifest.contains(r#"android:roundIcon="@mipmap/ic_launcher_vs_round""#));

    for name in ["ic_launcher_vs", "ic_launcher_vs_round"] {
        let adaptive = project_file(&format!("android/res/mipmap-anydpi-v26/{name}.xml"));
        assert!(
            adaptive.contains("@drawable/ic_launcher_vs_background"),
            "{name}"
        );
        assert!(
            adaptive.contains("@drawable/ic_launcher_vs_foreground"),
            "{name}"
        );
        // Android 13+ themed icons.
        assert!(adaptive.contains("<monochrome"), "{name}");
    }
}
