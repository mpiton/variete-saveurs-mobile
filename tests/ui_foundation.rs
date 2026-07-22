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

    for forbidden in [
        "#C0182B", "#C49A45", "#EBA4AE", "#F6F0E2", "#4A2C1A", "Georgia",
    ] {
        assert!(!css.contains(forbidden), "app CSS contains {forbidden}");
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
}
