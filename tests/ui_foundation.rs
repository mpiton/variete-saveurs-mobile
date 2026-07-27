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
        ("--color-bg", "#F6F0E2"),
        ("--color-surface", "#FFFFFF"),
        ("--color-surface-dim", "#ECE3CF"),
        ("--color-ink", "#4A2C1A"),
        ("--color-muted", "#6B5A4E"),
        ("--color-label", "#5A4535"),
        ("--color-border", "#DCD0B8"),
        ("--color-border-soft", "#E9E1CE"),
        ("--color-chrome", "#6B1220"),
        ("--color-on-chrome", "#FFFFFF"),
        ("--color-on-chrome-muted", "#EBA4AE"),
        ("--color-primary", "#C0182B"),
        ("--color-on-primary", "#FFFFFF"),
        ("--color-primary-tint", "#F7E3E5"),
        ("--color-danger", "#B3261E"),
    ];

    for (name, value) in expected {
        assert_eq!(css_token(&css, name), value);
    }

    // The Vitrine hues are now the app's own (DESIGN.md §2), so only the
    // typography half of the old rule survives: the serif stays on the
    // letterhead. Lowercased, `Georgia` and `georgia` are the same font.
    let folded = css.to_ascii_lowercase();
    assert!(
        !folded.contains("georgia"),
        "the serif belongs to the document"
    );

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

/// The teal « Le Comptoir » scheme was dropped for the document's own palette
/// (DESIGN.md §2). Its hexes survived in four places at once — the stylesheet,
/// the inline pre-render style, the Android theme colour and the activity's
/// window background — so the guard sweeps the whole app rather than the CSS.
#[test]
fn no_teal_survives_in_the_app_chrome() {
    fn walk(directory: &Path, files: &mut Vec<std::path::PathBuf>) {
        let entries = fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, files);
            } else {
                files.push(path);
            }
        }
    }

    // Hex forms plus the decimal triples MainActivity.kt paints the window
    // with — a plain hex sweep would walk straight past `Color.rgb(15, 63, 58)`.
    const TEAL: [&str; 10] = [
        "#0f3f3a",
        "#0c2b27",
        "#0f766e",
        "#58b5a9",
        "#e7f2ee",
        "#16332e",
        "#a8c8c2",
        "#06302b",
        "15, 63, 58",
        "12, 43, 39",
    ];

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    // `templates/` is the A4 document and keeps its own palette; it has no teal
    // either, but it is deliberately out of this guard's reach.
    for directory in ["src", "assets", "android"] {
        walk(&root.join(directory), &mut files);
    }

    let mut offenders = Vec::new();
    for path in files {
        // The splash loop is a video, not text: reading it as UTF-8 fails.
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        let folded = contents.to_ascii_lowercase();
        for teal in TEAL {
            if folded.contains(teal) {
                offenders.push(format!("{} contains {teal}", path.display()));
            }
        }
    }
    assert!(offenders.is_empty(), "teal survives: {offenders:#?}");
}

#[test]
fn dark_palette_matches_the_design_and_meets_aa_contrast() {
    let css = project_file("assets/app.css");
    let dark = dark_block(&css);

    // DESIGN.md frontmatter (dark-*) is normative for these eight.
    // The rest are derived here; see the CSS comments for the rationale.
    let expected = [
        ("--color-bg", "#17110D"),
        ("--color-surface", "#221A14"),
        ("--color-surface-dim", "#2E241C"),
        ("--color-ink", "#EFE5D6"),
        ("--color-muted", "#B3A493"),
        ("--color-label", "#C8BAA7"),
        ("--color-border", "#443729"),
        ("--color-border-soft", "#33291F"),
        ("--color-chrome", "#4A0C16"),
        ("--color-primary", "#EBA4AE"),
        ("--color-on-primary", "#4A0511"),
        ("--color-primary-tint", "#3A181C"),
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

/// Body of a rule, found by its own selector on its own line — so
/// `.m3-button--tonal` never matches `.chrome-action-bar .m3-button--tonal`.
fn rule_block<'a>(css: &'a str, selector: &str) -> Option<&'a str> {
    let needle = format!("\n{selector} {{\n");
    let start = css.find(&needle)? + needle.len();
    let end = css[start..].find('}')?;
    Some(&css[start..start + end])
}

fn declaration<'a>(block: &'a str, property: &str) -> Option<&'a str> {
    block.lines().find_map(|line| {
        line.trim()
            .strip_prefix(property)?
            .strip_prefix(':')
            .map(|value| value.trim().trim_end_matches(';'))
    })
}

/// A declared value as a colour in one scheme. `transparent`, or no declaration
/// at all, means the signal is deliberately absent and cannot carry the shape.
fn signal<'a>(scheme: &'a str, value: Option<&str>) -> Option<&'a str> {
    let name = value?.trim().strip_prefix("var(")?.strip_suffix(')')?;
    let mut resolved = css_token(scheme, name.trim());
    // Tokens alias tokens: `--color-on-chrome-primary-edge` is
    // `var(--color-on-chrome)` in the light scheme and `transparent` in the dark.
    for _ in 0..8 {
        let Some(alias) = resolved
            .strip_prefix("var(")
            .and_then(|value| value.strip_suffix(')'))
        else {
            break;
        };
        resolved = css_token(scheme, alias.trim());
    }
    resolved.starts_with('#').then_some(resolved)
}

/// Every `.m3-button--<variant>` the sheet declares, so a variant added later
/// is covered by the guard below without anyone remembering to add it.
fn button_variants(css: &str) -> Vec<&str> {
    let mut names: Vec<&str> = css
        .lines()
        .filter_map(|line| {
            let name = line
                .trim()
                .strip_prefix(".m3-button--")?
                .strip_suffix(" {")?;
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
                .then_some(name)
        })
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

#[test]
fn every_button_variant_keeps_a_shape_on_every_surface_it_is_posed_on() {
    let css = project_file("assets/app.css");
    let dark = dark_block(&css);
    let light = css.replace(dark, "");

    // DESIGN.md §2 — « a control must own a signal, fill or edge, that clears
    // 3:1 against the surface it is posed on ».
    //
    // This guard has now been widened twice, both times after shipping the bug
    // it should have caught. First along the variant axis: it covered only the
    // tonal button, the one where the defect had shown up, and the filled
    // variant sat at 1.97:1 on the light chrome for as long as it did. Then
    // along the host axis: it covered only the chrome bar, and the tonal and
    // outlined variants sat between 1.04:1 and 1.53:1 on every content surface,
    // across 19 call sites. The rule was right both times; the guard was a copy
    // of the symptom.
    //
    // So it now reads the variants out of the stylesheet, resolves each one's
    // fill and edge per scheme, and measures them against every surface the app
    // poses a button on. Nothing here is a list of known cases.
    for selector in [
        ".chrome-action-bar .m3-button--tonal",
        ".chrome-action-bar .m3-button--filled",
    ] {
        assert!(css.contains(selector), "{selector} must be remapped");
    }

    // The bar, every card and panel, the page, and the bottom sheets.
    const HOSTS: [&str; 4] = [
        "--color-chrome",
        "--color-surface",
        "--color-bg",
        "--color-elevated",
    ];

    let variants = button_variants(&css);
    assert!(
        variants.len() >= 4,
        "expected the four M3 variants, found {variants:?}"
    );

    for variant in variants {
        let base = rule_block(&css, &format!(".m3-button--{variant}"))
            .unwrap_or_else(|| panic!(".m3-button--{variant} must declare a rule"));
        // On the chrome the descendant rule wins wherever it declares anything.
        let on_chrome = rule_block(&css, &format!(".chrome-action-bar .m3-button--{variant}"));

        for (scheme_name, scheme) in [("clair", light.as_str()), ("sombre", dark)] {
            for host_token in HOSTS {
                let host = css_token(scheme, host_token);
                let overriding = (host_token == "--color-chrome")
                    .then_some(on_chrome)
                    .flatten();
                let pick = |property| {
                    overriding
                        .and_then(|block| declaration(block, property))
                        .or_else(|| declaration(base, property))
                };

                let fill = signal(scheme, pick("background"));
                let edge = signal(scheme, pick("border-color"));
                let where_ = format!("{variant}/{scheme_name} sur {host_token}");

                // A text button owns no container at all: its affordance is the
                // label, whose AA is checked by the palette tests. Nothing to
                // measure, and nothing 1.4.11 asks for.
                if fill.is_none() && edge.is_none() {
                    continue;
                }

                let fill_ratio = fill.map_or(0.0, |value| contrast_ratio(value, host));
                let edge_ratio = edge.map_or(0.0, |value| contrast_ratio(value, host));
                assert!(
                    fill_ratio.max(edge_ratio) >= 3.0,
                    "{where_}: the button has no shape (aplat {fill_ratio:.2}:1, bord {edge_ratio:.2}:1)"
                );

                // And AA of the label riding the fill.
                if let (Some(fill), Some(label)) = (fill, signal(scheme, pick("color"))) {
                    let text = contrast_ratio(label, fill);
                    assert!(
                        text >= 4.5,
                        "{where_}: the label is {text:.2}:1 on its fill"
                    );
                }
            }
        }
    }
}

#[test]
fn the_bottom_system_inset_is_counted_once_on_its_axis() {
    let css = project_file("assets/app.css");
    let body = |selector: &str| {
        css.split(selector)
            .nth(1)
            .and_then(|rule| rule.split('}').next())
            .unwrap_or_else(|| panic!("missing rule {selector}"))
            .to_string()
    };

    // A sticky bar's constraint rectangle is the scrollport *minus this
    // container's padding*, so any bottom padding here parks the chrome that
    // many pixels above the screen edge — the navigation band then shows the
    // content colour, with content scrolling into it.
    assert!(
        !body(".screen-scroll {").contains("--system-inset-bottom"),
        "the scroll container must not reserve the bottom inset when a bar can paint it"
    );

    // The bar carries it instead: background to the edge, labels above the band.
    assert!(body(".chrome-action-bar {").contains("var(--system-inset-bottom)"));

    // And screens with no bar reserve it themselves, so it is counted once
    // either way — never twice, never zero times.
    assert!(
        css.contains(".screen-scroll:not(:has(.chrome-action-bar))"),
        "screens without a chrome bar must reserve the navigation band"
    );
}

// Kotlin cannot know which screen the WebView is showing, so any navigation
// bar appearance derived from something other than a constant is wrong on part
// of the app. Issue #36: derived from `uiMode`, it put dark icons on the red
// action bar at 1,7:1 on the four screens that carry one. The band is chrome on
// every screen instead, and the icons are light in both schemes.
#[test]
fn the_navigation_band_keeps_one_background_so_its_icons_can_stay_light() {
    let css = project_file("assets/app.css");
    let rule = |selector: &str| {
        css.split(selector)
            .nth(1)
            .and_then(|body| body.split('}').next())
            .unwrap_or_else(|| panic!("missing rule {selector}"))
            .to_string()
    };
    const BAND: &str = "border-bottom: var(--system-inset-bottom) solid var(--color-chrome);";

    // Everything that can cover the band paints it. The action bar has it on
    // four screens; the scroll container takes over on the other three; and the
    // sheet is in the top layer, so it reaches the edge over any screen at all —
    // including its error variant, whose `border` shorthand resets that edge.
    for selector in [
        ".screen-scroll:not(:has(.chrome-action-bar)) {",
        ".bottom-sheet {",
        ".bottom-sheet-layer.is-error .bottom-sheet {",
    ] {
        assert!(
            rule(selector).contains(BAND),
            "{selector} must paint the navigation band in chrome"
        );
    }

    // Except with the keyboard up: the shell stops at the top of the IME, which
    // owns the band from there and paints it itself.
    assert!(
        rule("html.ime-visible .screen-scroll:not(:has(.chrome-action-bar)) {")
            .contains("border-bottom-width: 0;"),
        "the band must not be reserved above the keyboard"
    );

    let activity = project_file("android/MainActivity.kt");
    let assignments: Vec<&str> = activity
        .lines()
        .filter(|line| line.contains("isAppearanceLightNavigationBars"))
        .map(str::trim)
        .collect();
    assert_eq!(
        assignments,
        ["isAppearanceLightNavigationBars = false"],
        "the navigation bar icons must be light unconditionally"
    );
}

#[test]
fn the_record_action_bar_keeps_its_two_delivery_paths_at_parity() {
    let css = project_file("assets/app.css");
    let bar = css
        .split(".record-action-bar {")
        .nth(1)
        .and_then(|rule| rule.split('}').next())
        .expect("the record action bar rule must exist");

    // PRODUCT.md keeps sharing and sending at parity, so the bar gives them
    // one column each — a wider one for either would privilege it by form.
    assert!(
        bar.contains("grid-template-columns: 1fr 1fr;"),
        "the two delivery actions must share the bar evenly"
    );

    // The disabled-send explanation is not a third action: it spans.
    let hint = css
        .split(".record-screen .record-action-hint {")
        .nth(1)
        .and_then(|rule| rule.split('}').next())
        .expect("the hint rule must exist");
    assert!(hint.contains("grid-column: 1 / -1;"));
}

#[test]
fn keyboard_focus_is_visible_on_every_focusable_element_the_app_renders() {
    let css = project_file("assets/app.css");
    // The email body is a `textarea` and the record's line list a `summary`:
    // both take focus by default and neither is a button, a link or an input,
    // so both fell through the global ring.
    for selector in [
        "button:focus-visible",
        "a:focus-visible",
        "input:focus-visible",
        "textarea:focus-visible",
        "summary:focus-visible",
    ] {
        assert!(css.contains(selector), "{selector} has no focus ring");
    }
}

#[test]
fn the_fab_glyph_turns_with_its_menu_and_still_cuts_under_reduced_motion() {
    let css = project_file("assets/app.css");
    assert!(
        css.contains(".fab[aria-expanded=\"true\"] .lucide"),
        "the open state must reach the glyph, not only the accessible label"
    );

    // Motion here is state, not decoration, so « Remove animations » must keep
    // the end position and drop only the travel.
    //
    // The rule is positional, and that is the whole point: the escape carries
    // the same specificity as the rule it cancels, so only source order decides.
    // Asserting « a reduce block mentions it somewhere » passed for a while
    // with the escape declared 89 lines *before* the transition, where it did
    // nothing at all.
    let declaration = css
        .find(".fab .lucide {\n    transition:")
        .expect("the FAB glyph must declare its transition");
    let escape = css
        .match_indices("@media (prefers-reduced-motion: reduce)")
        .filter_map(|(at, _)| {
            let block = &css[at..];
            let block = &block[..block.find("\n}\n").unwrap_or(block.len())];
            (block.contains(".fab .lucide") && block.contains("transition: none")).then_some(at)
        })
        .next()
        .expect("the FAB rotation has no reduced-motion escape");

    assert!(
        escape > declaration,
        "the escape is declared before the transition it cancels, so it loses the cascade"
    );
}

#[test]
fn the_segmented_button_styles_the_state_it_announces() {
    let css = project_file("assets/app.css");
    let actions = project_file("src/ui/components/actions.rs");

    // Mutually exclusive filters are radios: TalkBack then carries the
    // exclusivity. The active-segment rule has to follow the attribute the
    // component emits, or the selection silently stops being visible.
    assert!(actions.contains("role: \"radio\""));
    assert!(actions.contains("aria_checked: index == selected"));
    assert!(css.contains("[aria-checked=\"true\"] .segmented-button__label"));
    assert!(
        !css.contains("aria-pressed"),
        "a leftover aria-pressed rule would style a state nothing sets"
    );
}

#[test]
fn dark_scheme_overrides_every_light_color_token() {
    let css = project_file("assets/app.css");
    let dark = dark_block(&css);

    // Both on-chrome colors sit on the chrome, which is dark in either
    // scheme, and they clear AA on the darker one — no override needed. Gold
    // is the document's own, and paper does not follow the scheme: it reaches
    // 6.57:1 on the dark surface with the light value unchanged.
    const SHARED: [&str; 3] = [
        "--color-on-chrome",
        "--color-on-chrome-muted",
        "--color-gold",
    ];

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

/// DESIGN.md §2 — « L'app et le document tirent des mêmes cinq teintes ; sept
/// tokens clairs sont les valeurs Vitrine verbatim. » Nothing checked that. The
/// document is the source, so the screen follows it: every colour the A4
/// template declares must exist, to the digit, in the app's light scheme. A
/// hue the document owns and the screen has no answer for is the gap this
/// found — gold, which ruled the letterhead and appeared nowhere on screen.
#[test]
fn the_screen_carries_every_colour_the_document_declares() {
    let app = project_file("assets/app.css");
    let document = project_file("templates/document.css");
    let dark = dark_block(&app);
    let light = app.replace(dark, "");

    let values = |css: &str, prefix: &str| -> Vec<String> {
        css.lines()
            .filter_map(|line| {
                let line = line.trim();
                let (name, value) = line.split_once(':')?;
                name.trim().starts_with(prefix).then(|| {
                    value
                        .trim()
                        .trim_end_matches(';')
                        .to_ascii_uppercase()
                        .to_string()
                })
            })
            .collect()
    };

    let screen = values(&light, "--color-");
    let paper = values(&document, "--");
    assert_eq!(paper.len(), 8, "the A4 template declares eight colours");

    for colour in paper {
        assert!(
            screen.contains(&colour),
            "{colour} rules the document and has no token on screen"
        );
    }
}

/// The record's thumbnail is the document, not a picture of it, so its content
/// is already on the screen as text and its frame is focusable by default.
/// Left alone it would be read twice by a screen reader and land in the tab
/// order, and it would look tappable without being tappable.
#[test]
fn the_record_thumbnail_is_inert() {
    let css = project_file("assets/app.css");
    let record = project_file("src/ui/record.rs");
    let frame = rule_block(&css, ".record-thumb__frame").expect("the frame rule must exist");

    assert!(
        frame.contains("pointer-events: none;"),
        "the thumbnail must not take taps: the button below is the way in"
    );
    for attribute in ["aria_hidden: \"true\"", "tabindex: \"-1\""] {
        assert!(
            record.contains(attribute),
            "the thumbnail must carry {attribute}"
        );
    }
}

#[test]
fn every_a4_sheet_stays_white_in_both_schemes() {
    let css = project_file("assets/app.css");
    let dark = dark_block(&css);

    // Paper, not a surface: the sheet must not follow the scheme — on the
    // full-screen preview and on the record's thumbnail alike. The guard used
    // to name the preview alone, and counted *mentions* of it, so a comment
    // naming the class was enough to trip it while a second sheet added beside
    // it went unchecked. It counts rule openings now, over every element that
    // renders paper.
    for selector in [".preview-frame", ".record-thumb", ".record-thumb__frame"] {
        let opening = format!("\n{selector} {{\n");
        assert_eq!(
            css.matches(&opening).count(),
            1,
            "{selector} must be declared exactly once"
        );
        assert!(
            !dark.contains(&opening),
            "{selector} must not be repainted in the dark scheme"
        );
        let rule = rule_block(&css, selector).unwrap_or_else(|| panic!("{selector} must exist"));
        assert!(
            rule.contains("background: #FFFFFF;"),
            "{selector} must stay white"
        );
    }
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
        "rgb(30 18 10 / 0.45)",
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
