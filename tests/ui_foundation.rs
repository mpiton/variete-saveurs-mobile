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
/// The `var()` is found anywhere in the value, so a shorthand such as
/// `border: 1px solid var(--color-ink)` resolves like a longhand does.
fn signal<'a>(scheme: &'a str, value: Option<&str>) -> Option<&'a str> {
    let value = value?;
    let open = value.find("var(")? + "var(".len();
    let close = open + value[open..].find(')')?;
    let mut resolved = css_token(scheme, value[open..close].trim());
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

/// A box that holds text is not locked to a pixel height, and a threshold that
/// governs text is written in the text's unit.
///
/// Same rule as the floating label, three other places. At 200 % system font the
/// top app bar title was truncated — 318px of « Envoi par email » into 276 —
/// which cost the screen its only context marker, and DESIGN.md §5 makes those
/// seven titles normative. « Professionnel » spilled out of its segment, which
/// was locked to 40px. The line sheet's two columns stayed two columns until
/// « Descendre » ran past its own button. Only a browser measures the result;
/// what this pins is that the causes cannot come back.
#[test]
fn a_box_that_holds_text_is_not_locked_to_a_pixel_height() {
    let css = project_file("assets/app.css");
    let rule = |selector: &str| {
        rule_block(&css, selector).unwrap_or_else(|| panic!("{selector} must exist"))
    };

    let title = rule(".top-app-bar__title");
    assert!(
        !title.contains("white-space: nowrap") && !title.contains("text-overflow: ellipsis"),
        "the bar title wraps rather than losing the name of the screen"
    );
    for selector in [".segmented-button__label", ".segmented-button__option"] {
        let segment = rule(selector);
        assert!(
            !segment.contains("height: ") || segment.contains("min-height: "),
            "{selector} must not be locked to a height its label can outgrow"
        );
    }
    assert!(
        rule(".line-sheet__row").contains("minmax(10rem"),
        "the column threshold follows the system font, or it never gives way"
    );
}

/// Room reserved for text is expressed in that text's own unit, or it stops
/// being enough the moment the system font grows.
///
/// The field used to reserve `--space-xs` — 6 fixed px — for a label sized in
/// rem. At 200 % the label was 24px tall, 48px once it wrapped, and it lay
/// across the value on a field 50px high: the text became unreadable at exactly
/// the size chosen to make it readable. Only a browser can measure that, so
/// what this pins is the shape of the answer — the label sits in the flow and
/// takes back half of its *own* line box, rather than being positioned over a
/// gap someone guessed at.
#[test]
fn the_floating_label_reserves_room_in_its_own_unit() {
    let css = project_file("assets/app.css");
    let field = rule_block(&css, ".outlined-field").expect(".outlined-field must exist");
    let label = rule_block(&css, ".outlined-field label").expect("its label rule must exist");
    let spinner =
        rule_block(&css, ".outlined-field__spinner").expect("its spinner rule must exist");

    assert!(
        !field.contains("padding-top"),
        "the field must not reserve a fixed gap for a label that scales"
    );
    assert!(
        label.contains("0.5em"),
        "the overlap must follow the label's own line box, not a spacing token"
    );
    assert!(
        !label.contains("position: absolute"),
        "an absolute label cannot push the field down when it wraps"
    );
    assert!(
        !spinner.contains("top:"),
        "the spinner rides the input's row; a fixed offset assumes the input never moves"
    );
}

/// Controls whose outline is the whole of their affordance: their fill is the
/// surface they sit on, so if the edge does not clear 3:1 there is nothing left
/// to say they can be tapped. Same DESIGN.md §2 rule the buttons and the menu
/// answer to, applied where it was never applied.
///
/// The list is maintained by hand, and that is a real limit: « is this
/// tappable » is not derivable from a stylesheet, and the host a rule sits on
/// is not either. What the test does buy is that the four cannot drift back —
/// and the pairing of an element with the surface it is posed on is written
/// down somewhere instead of living in whoever last measured it.
#[test]
fn a_control_that_is_only_an_outline_keeps_that_outline_visible() {
    let css = project_file("assets/app.css");
    let dark = dark_block(&css);
    let light = css.replace(dark, "");

    // (rule, the surfaces it can be posed on)
    let cases: [(&str, &[&str]); 6] = [
        // Floats over the home screen, and over a card once the list scrolls.
        (".fab-menu__item", &["--color-bg", "--color-surface"]),
        // Proposals under the client field, inside a form panel.
        (".client-suggestion", &["--color-surface"]),
        // The filter on the home screen, the client kind inside a panel.
        (
            ".segmented-button__label",
            &["--color-bg", "--color-surface"],
        ),
        // A grid of them inside the catalogue sheet.
        (".catalog-chip", &["--color-elevated"]),
        // An M3 outlined field is its outline: no fill to fall back on, since
        // its background is the panel's own. In panels, in sheets, and — for
        // the history search — straight on the page.
        (
            ".outlined-field input",
            &["--color-surface", "--color-elevated", "--color-bg"],
        ),
        (
            ".outlined-field textarea",
            &["--color-surface", "--color-elevated"],
        ),
    ];

    for (selector, hosts) in cases {
        let rule = rule_block(&css, selector).unwrap_or_else(|| panic!("{selector} must exist"));
        for (scheme_name, scheme) in [("clair", light.as_str()), ("sombre", dark)] {
            let fill = signal(scheme, declaration(rule, "background"));
            let edge = signal(scheme, declaration(rule, "border"));
            for host_token in hosts {
                let host = css_token(scheme, host_token);
                let fill_ratio = fill.map_or(0.0, |value| contrast_ratio(value, host));
                let edge_ratio = edge.map_or(0.0, |value| contrast_ratio(value, host));
                assert!(
                    fill_ratio.max(edge_ratio) >= 3.0,
                    "{selector} {scheme_name} sur {host_token}: nothing says it is tappable (aplat {fill_ratio:.2}:1, bord {edge_ratio:.2}:1)"
                );
            }
        }
    }
}

/// The top app bar's menu is the one panel that floats free: it overlaps the
/// bar by `--space-xs` and spills over the page and over whatever card sits
/// beneath it, so it has to keep a shape against all three at once. The bottom
/// sheets are not in this guard — they arrive with a scrim, which separates
/// them by construction.
///
/// It failed on three of four hosts in the light scheme and on all four in the
/// dark one, where the elevated fill measures 1.02:1 against the chrome and the
/// shadow is pure black on a near-black page. Same rule as the buttons, same
/// blind spot: DESIGN.md §2 was written for the chrome bar and applied there.
#[test]
fn the_floating_menu_keeps_a_shape_over_everything_it_covers() {
    let css = project_file("assets/app.css");
    let dark = dark_block(&css);
    let light = css.replace(dark, "");
    let menu = rule_block(&css, ".app-menu").expect(".app-menu must declare a rule");

    // Everything it can be painted over: the bar it hangs from, the page, a
    // card, and a passive zone.
    const HOSTS: [&str; 4] = [
        "--color-chrome",
        "--color-bg",
        "--color-surface",
        "--color-surface-dim",
    ];

    for (scheme_name, scheme) in [("clair", light.as_str()), ("sombre", dark)] {
        let fill = signal(scheme, declaration(menu, "background"));
        let edge = signal(scheme, declaration(menu, "border"));
        for host_token in HOSTS {
            let host = css_token(scheme, host_token);
            let fill_ratio = fill.map_or(0.0, |value| contrast_ratio(value, host));
            let edge_ratio = edge.map_or(0.0, |value| contrast_ratio(value, host));
            assert!(
                fill_ratio.max(edge_ratio) >= 3.0,
                "{scheme_name} sur {host_token}: the menu has no shape (aplat {fill_ratio:.2}:1, bord {edge_ratio:.2}:1)"
            );
        }
    }
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
    // Anchored on the start of a line, so a descendant rule ending in the same
    // selector — `html.ime-visible .chrome-action-bar` — is not mistaken for
    // the base one it overrides.
    let body = |selector: &str| {
        css.split(&format!("\n{selector}"))
            .nth(1)
            .and_then(|rule| rule.split('}').next())
            .unwrap_or_else(|| panic!("missing rule {selector}"))
            .to_string()
    };

    // A sticky bar's constraint rectangle is the scrollport *minus this
    // container's padding*, so any bottom padding here parks the chrome that
    // many pixels above the screen edge — the navigation band then shows the
    // content colour, with content scrolling into it.
    //
    // The `padding` declaration alone, not the whole rule: `scroll-padding`
    // names the box a scroll anchors against and has no effect on layout, so
    // the bar's rectangle does not move when it reserves the same inset.
    let layout_padding = body(".screen-scroll {")
        .split(';')
        .find(|declaration| declaration.trim_start().starts_with("padding:"))
        .unwrap_or_else(|| panic!(".screen-scroll declares no padding"))
        .to_string();
    assert!(
        !layout_padding.contains("--system-inset-bottom"),
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

    // Once on the axis also means zero times while the keyboard is up: the IME
    // covers the navigation band and paints its own. The bar kept reserving it
    // anyway, which floated 71px of red between the buttons and the keyboard.
    assert!(
        !body("html.ime-visible .chrome-action-bar {").contains("--system-inset-bottom"),
        "the bar must not reserve the navigation band the keyboard already covers"
    );
}

#[test]
fn programmatic_reveals_stop_above_the_sticky_action_bar() {
    let css = project_file("assets/app.css");
    let scrollport =
        rule_block(&css, ".screen-scroll").expect(".screen-scroll must declare its geometry");

    for part in [
        "scroll-padding-bottom: calc(",
        "var(--touch-target)",
        "2 * var(--space-sm)",
        "var(--system-inset-bottom)",
    ] {
        assert!(
            scrollport.contains(part),
            "the scrollport must reserve every part of the sticky action bar for reveal targets"
        );
    }
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

/// The thumbnail leads where it looks like it leads.
///
/// It is a picture of the document, 72 by 102, first thing on the fiche, sitting
/// above a button labelled « Voir le document ». It gets tapped. `DESIGN.md §7`
/// forbids promising a gesture that does not exist, and the answer to that is to
/// let it be tapped rather than to leave it inert.
///
/// The test this replaces claimed inertness and checked three strings that all
/// survived the reversal — `pointer-events: none` on the frame, `aria_hidden`,
/// `tabindex="-1"` — so it went green while its own subject became a control.
/// What it should have pinned is the relationship: one destination, two ways in.
#[test]
fn the_record_thumbnail_opens_the_document_it_shows() {
    let css = project_file("assets/app.css");
    let record = project_file("src/ui/record.rs");

    // Counted rather than named: however the route is written, both ways in have
    // to be written the same, or the picture leads somewhere else than the
    // button under it.
    assert_eq!(
        record
            .matches("Route::Preview { document: Some(id) }")
            .count(),
        2,
        "the thumbnail and « Voir le document » must push the same route"
    );

    let (before, after) = record
        .split_once("class: \"record-thumb\",")
        .expect("the fiche must render a thumbnail");
    assert!(
        before.trim_end().ends_with("button {"),
        "the thumbnail must be a button: it is a picture of the document, and it reads as tappable"
    );
    let opening = &after[..after.find("iframe").expect("the frame sits inside it")];
    assert!(
        opening.contains("aria_label:") && opening.contains("onclick:"),
        "a control needs a name and a destination, not just a border"
    );

    // The frame stays out of the way: the tap belongs to the button around it,
    // and the button is announced once.
    let frame = rule_block(&css, ".record-thumb__frame").expect("the frame rule must exist");
    assert!(
        frame.contains("pointer-events: none;"),
        "the frame must not swallow the tap the button exists to receive"
    );
    for attribute in ["aria_hidden: \"true\"", "tabindex: \"-1\""] {
        assert!(
            record.contains(attribute),
            "the frame must carry {attribute}"
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

/// A field that shows an error has to say it.
///
/// `OutlinedField` is the only error surface eight of the app's thirteen error
/// bindings have: the recipient on the compose screen, the quantity and the
/// price in the line sheet, the quantity in the catalogue picker, the name and
/// the price in the catalogue sheet, the sender address and the Brevo key in
/// Réglages. None of them publishes an aggregated block, and none moves the
/// focus, so taking `role="alert"` off the component silences all eight — which
/// is exactly what a first attempt at de-duplicating the brouillon's three
/// announcements did.
///
/// The brouillon is the one screen with something better: an aggregated block
/// listing every failure, plus `reveal_first_error` putting the focus on the
/// first faulty control, which reads its own `aria-describedby`. So it opts out,
/// and nothing else may.
///
/// Derived from the source rather than listed. A test naming the five fields
/// that opt out today would have passed while the other eight went quiet.
#[test]
fn a_field_that_carries_an_error_announces_it_unless_its_screen_does_better() {
    fn walk(directory: &Path, files: &mut Vec<std::path::PathBuf>) {
        let entries = fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }

    let fields = project_file("src/ui/components/fields.rs");

    // Announcing is the default, so a caller that forgets the prop is audible.
    assert!(
        fields.contains("#[props(default = true)]\n    announce_error: bool,"),
        "announce_error must default to true: silence has to be asked for"
    );
    // …and the opt-out is what removes the live region, nothing else.
    assert_eq!(
        fields
            .matches("role: announce_error.then_some(\"alert\"),")
            .count(),
        2,
        "both OutlinedField and OutlinedTextArea gate their alert on the prop"
    );

    let mut opted_out = Vec::new();
    let mut sources = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui"),
        &mut sources,
    );
    for path in sources {
        let source = fs::read_to_string(&path).expect("read a ui source file");
        if source.contains("announce_error: false") {
            opted_out.push((path, source));
        }
    }

    assert_eq!(
        opted_out.len(),
        1,
        "only the brouillon may opt out, and it is one file: {:?}",
        opted_out.iter().map(|(path, _)| path).collect::<Vec<_>>()
    );
    let (path, form) = &opted_out[0];
    assert!(
        path.ends_with("form.rs"),
        "the opt-out belongs to the brouillon, not to {}",
        path.display()
    );

    // What it opts out of has to actually be there.
    assert!(
        form.contains("reveal_first_error"),
        "the brouillon opts out because it moves the focus — so it must"
    );
    assert!(
        form.contains("items: issue_errors"),
        "the brouillon opts out because it publishes an aggregated block — so it must"
    );
    // One opt-out per field bound to that block, and not one more: a field
    // carrying any other error would go silent with nothing covering it.
    // Counted as lines the RSX actually emits, so the prose above explaining
    // the opt-out is not mistaken for one.
    let count = |source: &str, needle: &str| {
        source
            .lines()
            .filter(|line| line.trim().starts_with(needle))
            .count()
    };
    assert_eq!(
        count(form, "announce_error: false"),
        count(form, "error: field_error(&issue_errors"),
        "every silenced field must be one the aggregated block speaks for"
    );
}

/// Back belongs to Android unless this app has a use for it.
///
/// An `OnBackPressedCallback` left enabled at default priority suppresses the
/// predictive back animations — the back-to-home preview, and the long-press
/// preview Android 16 gives three-button navigation — whatever
/// `android:enableOnBackInvokedCallback` says. This one was enabled
/// unconditionally while `DESIGN.md §5` promised « geste prédictif partout », so
/// the doc and the code disagreed for as long as both existed.
///
/// The interesting failure is not the flag but the seam: Kotlin cannot see a
/// `<dialog>` or the router, the web side cannot see the callback, and the two
/// meet on a global name no compiler on either side checks. Rename it in one
/// file and Back silently stops closing sheets. So the name is read out of the
/// Kotlin and required of the Rust, rather than written twice here.
#[test]
fn back_is_only_intercepted_when_the_app_has_something_to_do_with_it() {
    /// The string literal after `needle`, e.g. a Kotlin `const val`.
    fn quoted_after<'a>(source: &'a str, needle: &str) -> &'a str {
        let tail = source
            .split_once(needle)
            .unwrap_or_else(|| panic!("no `{needle}` in MainActivity.kt"))
            .1;
        let opening = tail.find('"').expect("an opening quote") + 1;
        let closing = opening + tail[opening..].find('"').expect("a closing quote");
        &tail[opening..closing]
    }

    let activity = project_file("android/MainActivity.kt");
    let shell = project_file("src/ui/app.rs");
    let sheet = project_file("src/ui/components/feedback.rs");

    // The callback can stand down…
    assert!(
        activity.contains("backCallback.isEnabled = intercepts"),
        "the back callback must be able to stand down, or the animations never run"
    );
    // …and it starts enabled, so the window before the first report behaves the
    // way this activity always did: a missing animation, never a trapped sheet.
    assert!(
        activity.contains("object : OnBackPressedCallback(true)"),
        "the callback starts enabled"
    );

    // The seam. Both halves are read from the Kotlin, so renaming either one
    // breaks this test instead of breaking Back on the phone.
    let global = quoted_after(&activity, "const val BACK_BRIDGE_NAME =");
    let method = activity
        .split_once("@JavascriptInterface")
        .expect("a bridge method")
        .1
        .split_once("fun ")
        .expect("a bridge method name")
        .1
        .split('(')
        .next()
        .expect("a bridge method name")
        .trim()
        .to_string();
    assert!(
        activity.contains("addJavascriptInterface(BackBridge(), BACK_BRIDGE_NAME)"),
        "the bridge must be exposed under the constant this test reads"
    );
    let call = format!("window.{global}?.{method}(");
    assert!(
        shell.contains(&call),
        "app.rs must call `{call}` — the bridge Kotlin actually exposes"
    );

    // What it reports has to be both things Kotlin cannot see. Either alone is a
    // trap: sheets only, and Back walks off a deep screen; routes only, and it
    // walks out of an open confirmation.
    let reported = shell
        .lines()
        .find(|line| line.trim().starts_with("let intercepts_back ="))
        .expect("app.rs must derive what it reports");
    assert!(
        reported.contains("open_sheets") && reported.contains("can_go_back"),
        "Back is intercepted for a sheet or for a route, not for one of them: {reported}"
    );

    // The count is kept by the component every sheet goes through, so one added
    // tomorrow is covered without anyone remembering to register it.
    let registers = sheet
        .find("use_context::<OpenSheets>()")
        .expect("BottomSheet must join the count");
    let early_return = sheet
        .find("if !open {")
        .expect("BottomSheet returns early when closed");
    assert!(
        registers < early_return,
        "the hooks must run before the early return, or their order changes when `open` flips"
    );
}

/// The acknowledgement in the hand needs a permission to exist at all.
///
/// `navigator.vibrate` is a silent no-op without `android.permission.VIBRATE` —
/// no error, no log, just nothing — so the two have to travel together. The
/// permission is the reason to keep the call rare: one pulse, on the one act
/// that cannot be taken back.
#[test]
fn the_pulse_on_emission_has_the_permission_it_needs_and_a_way_out() {
    let manifest = project_file("android/AndroidManifest.xml");
    let sheet = project_file("src/ui/components/issue_sheet.rs");

    assert!(
        sheet.contains("navigator.vibrate"),
        "the emission confirms itself in the hand"
    );
    assert!(
        manifest.contains("android.permission.VIBRATE"),
        "…which does nothing at all without the permission"
    );
    // `navigator.vibrate` answers to no user setting of its own, so it takes the
    // only one the web exposes.
    assert!(
        sheet.contains("prefers-reduced-motion: reduce"),
        "the pulse must have the same escape every other motion in the app has"
    );
    // One place, one pulse: a permission is not worth spending twice. The call
    // shape rather than the name, so the paragraph above explaining it is not
    // counted as a second pulse.
    assert_eq!(
        sheet.matches("navigator.vibrate?.(").count(),
        1,
        "the pulse belongs to the emission and to nothing else"
    );
}

/// The screen transition may not move anything.
///
/// M3 would ask for a shared axis here, and a shared axis is a `transform` — but
/// a transformed element becomes the containing block of everything inside it,
/// including `.form-sticky`, `.record-sticky` and `.compose-sticky`. They would
/// come unstuck and take the chrome action bar off the bottom of the window with
/// them, which is the one place the bar is for. Opacity is what is left.
#[test]
fn the_screen_transition_cannot_unstick_the_action_bars() {
    let css = project_file("assets/app.css");

    let keyframes = css
        .split_once("@keyframes screen-in")
        .expect("the screen transition must exist")
        .1
        .split_once("\n}")
        .expect("a keyframes body")
        .0;
    for moving in [
        "transform",
        "translate",
        "scale",
        "rotate",
        "inset",
        "margin",
    ] {
        assert!(
            !keyframes.contains(moving),
            "`{moving}` in the screen transition unsticks the action bars"
        );
    }
    assert!(
        keyframes.contains("opacity"),
        "the transition still has to be a transition"
    );

    // And it has its escape, in the block at the end of the file where every
    // other one lives — same specificity, so only source order decides.
    let reduced = css
        .rsplit_once("@media (prefers-reduced-motion: reduce)")
        .expect("the reduced-motion block must exist")
        .1;
    assert!(
        reduced.contains(".preview-screen") && reduced.contains("animation: none"),
        "« Supprimer les animations » must reach the screen transition too"
    );
}
