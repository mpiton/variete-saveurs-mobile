use std::{fs, path::Path};

fn project_file(path: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn css_token<'a>(css: &'a str, name: &str) -> &'a str {
    css.lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix(name)
                .and_then(|value| value.strip_prefix(':'))
                .map(|value| value.trim().trim_end_matches(';'))
        })
        .unwrap_or_else(|| panic!("missing CSS token {name}"))
}

/// Body of the `@media (prefers-color-scheme: dark)` rule, so the dark tokens
/// can be read with the same helper as the light ones.
fn dark_block(css: &str) -> &str {
    const QUERY: &str = "@media (prefers-color-scheme: dark)";
    // Every dark assertion below reads this one block. A second block further
    // down the cascade would override it unseen, so keep the scheme in one place.
    assert_eq!(
        css.matches(QUERY).count(),
        1,
        "the dark scheme must live in a single media query"
    );
    let after_query = css
        .find(QUERY)
        .map(|start| &css[start + QUERY.len()..])
        .unwrap_or_else(|| panic!("missing {QUERY} block"));
    let open = after_query
        .find('{')
        .expect("the dark media query must open a block");
    let mut depth = 0usize;
    for (offset, character) in after_query[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &after_query[open + 1..open + offset];
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces in the dark media query")
}

fn contrast_ratio(foreground: &str, background: &str) -> f64 {
    fn luminance(color: &str) -> f64 {
        let channels = [1, 3, 5].map(|start| {
            let channel = u8::from_str_radix(&color[start..start + 2], 16)
                .expect("design colors must be six-digit hex values")
                as f64
                / 255.0;
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        });
        0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2]
    }

    let foreground = luminance(foreground);
    let background = luminance(background);
    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
}

#[test]
fn light_palette_matches_the_design_and_meets_aa_contrast() {
    let css = project_file("assets/app.css");
    let expected = [
        ("--color-bg", "#F3F6F5"),
        ("--color-surface", "#FFFFFF"),
        ("--color-surface-dim", "#E9EEEC"),
        ("--color-ink", "#191C1B"),
        ("--color-muted", "#55605C"),
        ("--color-label", "#3F4A46"),
        ("--color-border", "#D5DDDA"),
        ("--color-border-soft", "#E2E8E5"),
        ("--color-chrome", "#0F3F3A"),
        ("--color-on-chrome", "#FFFFFF"),
        ("--color-on-chrome-muted", "#A8C8C2"),
        ("--color-primary", "#0F766E"),
        ("--color-on-primary", "#FFFFFF"),
        ("--color-primary-tint", "#E7F2EE"),
        ("--color-danger", "#B91C1C"),
    ];

    for (name, value) in expected {
        assert_eq!(css_token(&css, name), value);
    }

    // Lowercased on both sides: `#c0182b` is the same red to a browser.
    let folded = css.to_ascii_lowercase();
    for forbidden in [
        "#c0182b", "#c49a45", "#eba4ae", "#f6f0e2", "#4a2c1a", "georgia",
    ] {
        assert!(!folded.contains(forbidden), "app CSS contains {forbidden}");
    }

    assert!(
        contrast_ratio(
            css_token(&css, "--color-ink"),
            css_token(&css, "--color-bg")
        ) >= 4.5
    );
    assert!(
        contrast_ratio(
            css_token(&css, "--color-on-primary"),
            css_token(&css, "--color-primary")
        ) >= 4.5
    );
    assert!(
        contrast_ratio(
            css_token(&css, "--color-on-chrome"),
            css_token(&css, "--color-chrome")
        ) >= 4.5
    );
    assert!(
        contrast_ratio(
            css_token(&css, "--color-muted"),
            css_token(&css, "--color-surface")
        ) >= 4.5,
        "outlined-field placeholder must meet WCAG AA"
    );
}

#[test]
fn dark_palette_matches_the_design_and_meets_aa_contrast() {
    let css = project_file("assets/app.css");
    let dark = dark_block(&css);

    // DESIGN.md frontmatter (dark-*) is normative for these eight.
    // The rest are derived here; see the CSS comments for the rationale.
    let expected = [
        ("--color-bg", "#101413"),
        ("--color-surface", "#1A201E"),
        ("--color-surface-dim", "#242B29"),
        ("--color-ink", "#E2E8E6"),
        ("--color-muted", "#A3AFAB"),
        ("--color-label", "#B9C4C0"),
        ("--color-border", "#33403C"),
        ("--color-border-soft", "#283330"),
        ("--color-chrome", "#0C2B27"),
        ("--color-primary", "#58B5A9"),
        ("--color-on-primary", "#06302B"),
        ("--color-primary-tint", "#16332E"),
        ("--color-danger", "#F2938C"),
    ];
    for (name, value) in expected {
        assert_eq!(css_token(dark, name), value, "dark token {name}");
    }

    assert!(
        dark.contains("color-scheme: dark;"),
        "the dark scheme must opt in so native controls and scrollbars follow"
    );

    // Every foreground/background pair the components actually paint.
    let pair = |foreground: &str, background: &str| {
        contrast_ratio(css_token(dark, foreground), css_token(dark, background))
    };
    for (foreground, background) in [
        ("--color-ink", "--color-bg"),
        ("--color-ink", "--color-surface"),
        ("--color-ink", "--color-surface-dim"),
        ("--color-muted", "--color-bg"),
        ("--color-muted", "--color-surface"),
        ("--color-label", "--color-bg"),
        ("--color-label", "--color-surface"),
        ("--color-label", "--color-surface-dim"),
        ("--color-primary", "--color-bg"),
        ("--color-primary", "--color-surface"),
        // Badge « envoyé » and tonal buttons: the pale light tint would only
        // reach 2.1:1 under the lightened primary, hence a dark container.
        ("--color-primary", "--color-primary-tint"),
        ("--color-on-primary", "--color-primary"),
        ("--color-danger", "--color-bg"),
        ("--color-danger", "--color-surface"),
    ] {
        let ratio = pair(foreground, background);
        assert!(
            ratio >= 4.5,
            "{foreground} on {background} is {ratio:.2}:1 in the dark scheme"
        );
    }

    // The chrome keeps the light on-colors; check they still clear AA on it.
    for foreground in ["--color-on-chrome", "--color-on-chrome-muted"] {
        let ratio = contrast_ratio(
            css_token(&css, foreground),
            css_token(dark, "--color-chrome"),
        );
        assert!(
            ratio >= 4.5,
            "{foreground} on the dark chrome is {ratio:.2}:1"
        );
    }
}

#[test]
fn dark_scheme_overrides_every_light_color_token() {
    let css = project_file("assets/app.css");
    let dark = dark_block(&css);

    // Both on-chrome colors sit on the chrome, which is dark in either
    // scheme, and they clear AA on the darker one — no override needed.
    const SHARED: [&str; 2] = ["--color-on-chrome", "--color-on-chrome-muted"];

    let declared = |block: &str| -> Vec<String> {
        block
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                line.starts_with("--color-")
                    .then(|| line.split(':').next().unwrap_or_default().to_string())
            })
            .collect()
    };

    let light = css.replace(dark, "");
    for token in declared(&light) {
        if SHARED.contains(&token.as_str()) {
            continue;
        }
        assert!(
            declared(dark).contains(&token),
            "{token} keeps its light value in the dark scheme"
        );
    }
}

#[test]
fn the_a4_preview_stays_white_in_both_schemes() {
    let css = project_file("assets/app.css");
    // The preview is paper, not a surface: it must not follow the scheme.
    // A single rule, so a later one cannot repaint the sheet behind this test.
    assert_eq!(css.matches(".preview-frame").count(), 1);
    assert!(!dark_block(&css).contains(".preview-frame"));

    let frame = css
        .split(".preview-frame")
        .nth(1)
        .and_then(|rule| rule.split('}').next())
        .expect("the .preview-frame rule must exist");
    assert!(
        frame.contains("background: #FFFFFF;"),
        "the A4 sheet must stay white"
    );
}

#[test]
fn startup_background_matches_the_chrome_token_in_both_schemes() {
    let css = project_file("assets/app.css");
    let app = project_file("src/ui/app.rs");
    let activity = project_file("android/MainActivity.kt");

    for chrome in [
        css_token(&css, "--color-chrome"),
        css_token(dark_block(&css), "--color-chrome"),
    ] {
        assert!(
            app.contains(&format!("background:{chrome}")),
            "the pre-render style must cover {chrome}"
        );

        let channels = [1, 3, 5].map(|start| {
            u8::from_str_radix(&chrome[start..start + 2], 16)
                .expect("chrome must be a six-digit hex value")
        });
        assert!(
            activity.contains(&format!(
                "Color.rgb({}, {}, {})",
                channels[0], channels[1], channels[2]
            )),
            "the activity window background must cover {chrome}"
        );
    }
}

#[test]
fn android_follows_the_system_theme_without_recreating_the_activity() {
    let manifest = project_file("android/AndroidManifest.xml");
    // Without uiMode the activity is torn down and rebuilt on every theme
    // switch, which the user sees as a restart.
    assert!(manifest.contains("uiMode"));

    let activity = project_file("android/MainActivity.kt");
    assert!(activity.contains("UI_MODE_NIGHT_MASK"));
    // The surviving activity has to repaint its own chrome.
    let on_config_change = activity
        .split("override fun onConfigurationChanged")
        .nth(1)
        .expect("onConfigurationChanged must exist");
    assert!(on_config_change.contains("setBackgroundColor"));
}

#[test]
fn android_updates_font_scale_without_recreating_the_activity() {
    let manifest = project_file("android/AndroidManifest.xml");
    assert!(manifest.contains("keyboardHidden|fontScale"));

    let activity = project_file("android/MainActivity.kt");
    assert!(activity.contains("override fun onConfigurationChanged"));
    assert!(activity.contains("newConfig.fontScale"));
}

#[test]
fn android_replays_initial_insets_after_the_webview_attaches() {
    let activity = project_file("android/MainActivity.kt");
    // Insets are cached from the decor view (reliable dispatch on every
    // device) and replayed into the WebView once it attaches.
    assert!(activity.contains("setOnApplyWindowInsetsListener(window.decorView)"));
    assert!(activity.contains("webView.post"));
}

#[test]
fn interactive_controls_disable_double_tap_zoom() {
    let css = project_file("assets/app.css");
    assert!(css.contains("touch-action: manipulation;"));
}

#[test]
fn every_hardcoded_color_outside_the_tokens_is_scheme_agnostic() {
    let css = project_file("assets/app.css");
    let dark = dark_block(&css);
    // The two `:root` blocks are where literals belong; everywhere else a
    // literal is frozen against the scheme, so each one needs a reason.
    let light_root = css
        .split(":root {")
        .nth(1)
        .and_then(|rule| rule.split("\n}").next())
        .expect("the light :root block must exist");
    let painted = css.replace(light_root, "").replace(dark, "");

    let allowed = [
        // The A4 is paper. DESIGN.md §2 keeps it white in both schemes.
        "#FFFFFF",
        // Scrim value pinned by DESIGN.md §4.
        "rgb(10 20 18 / 0.45)",
        // `.icon-button` only ever sits on the chrome, which is dark in both
        // schemes: white at 12% stays a visible press state either way.
        "rgb(255 255 255 / 0.12)",
    ];
    // `#main` is a selector, not a colour: only count `#` runs that are hex.
    let is_hex = |value: &str| {
        matches!(value.len(), 4 | 5 | 7 | 9) && value[1..].chars().all(|c| c.is_ascii_hexdigit())
    };
    let mut found = Vec::new();
    for chunk in painted.split([';', '{', '\n']) {
        for literal in [chunk.find('#'), chunk.find("rgb(")].into_iter().flatten() {
            let value = chunk[literal..].trim().trim_end_matches([';', ',']);
            if (value.starts_with('#') && !is_hex(value)) || allowed.contains(&value) {
                continue;
            }
            found.push(value.to_owned());
        }
    }
    assert!(
        found.is_empty(),
        "hardcoded colors outside the scheme tokens: {found:?}"
    );
}

#[test]
fn the_android_theme_follows_the_system_scheme() {
    // The WebView answers `prefers-color-scheme` from the app theme's
    // `android:isLightTheme`, so a `.Light` parent pins the page to the light
    // stylesheet whatever the phone is set to.
    let styles = project_file("android/res/values/styles.xml");
    assert!(styles.contains("Theme.AppCompat.DayNight.NoActionBar"));
    // The starting window is painted from the theme before the process exists,
    // which is out of reach of `window.setBackgroundDrawable`.
    assert!(styles.contains("android:windowBackground\">@color/vs_chrome"));

    let css = project_file("assets/app.css");
    for (resource, scheme) in [
        ("android/res/values/vs_colors.xml", &css as &str),
        ("android/res/values-night/vs_colors.xml", dark_block(&css)),
    ] {
        let chrome = css_token(scheme, "--color-chrome");
        assert!(
            project_file(resource).contains(&format!("\"vs_chrome\">{chrome}<")),
            "{resource} must carry {chrome}"
        );
    }
}
