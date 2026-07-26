//! DESIGN §8: the in-app splash loops `assets/splash-loop.mp4` behind the real
//! `templates/logo.png`, on the chrome red, for at most 2.5 s. The asset is
//! committed and the timings are split between CSS and Rust — this guards the
//! file, keeps the two halves in step, and pins the reduced-motion contract.

use std::{fs, path::Path};

/// Acceptance criterion of the task: the splash may add at most 1.3 MB to the
/// APK, and it is the only asset it adds.
const MAX_VIDEO_BYTES: u64 = 1_300_000;
/// DESIGN §8 budget for the whole splash, from first paint to hand-off.
const MAX_SPLASH_MS: u64 = 2_500;

fn project_path(path: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn project_file(path: &str) -> String {
    let path = project_path(path);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn position_of(haystack: &[u8], needle: &[u8; 4]) -> Option<usize> {
    haystack.windows(4).position(|window| window == needle)
}

/// Body of a CSS rule, matched on the exact selector at the start of a line so
/// `.splash` never picks up `.splash__video`.
fn rule_body<'a>(css: &'a str, selector: &str) -> &'a str {
    let opening = format!("\n{selector} {{\n");
    let start = css
        .find(&opening)
        .unwrap_or_else(|| panic!("missing CSS rule {selector}"))
        + opening.len();
    let end = css[start..]
        .find("\n}")
        .unwrap_or_else(|| panic!("unterminated CSS rule {selector}"));
    &css[start..start + end]
}

/// Milliseconds of the `<duration> <easing> <delay>` shorthand in an
/// `animation:` declaration — the two `NNNms` values, in source order.
fn animation_timings(rule: &str) -> (u64, u64) {
    let declaration = rule
        .lines()
        .find_map(|line| line.trim().strip_prefix("animation:"))
        .unwrap_or_else(|| panic!("no animation declaration in rule: {rule}"));
    let mut millis = declaration
        .split_whitespace()
        .filter_map(|token| token.trim_end_matches(&[';', ')'][..]).strip_suffix("ms"))
        .filter_map(|value| value.parse::<u64>().ok());
    let duration = millis.next().expect("animation duration");
    let delay = millis.next().expect("animation delay");
    (duration, delay)
}

#[test]
fn the_video_asset_stays_inside_the_apk_budget_and_carries_no_sound() {
    let path = project_path("assets/splash-loop.mp4");
    let bytes = fs::read(&path).expect("assets/splash-loop.mp4 is committed");

    let size = bytes.len() as u64;
    assert!(
        size <= MAX_VIDEO_BYTES,
        "splash-loop.mp4 is {size} bytes, over the {MAX_VIDEO_BYTES} byte APK budget"
    );
    assert_eq!(&bytes[4..8], b"ftyp", "not an MP4 container");

    // Scan the header boxes only. The `mdat` payload is compressed frames, and
    // a four-byte needle does hit them by chance — `vide` already matches
    // inside this file's payload. `+faststart` is what puts the header first,
    // and the player needs it anyway to start decoding before the whole file
    // is read.
    let mdat = position_of(&bytes, b"mdat").expect("no mdat box");
    let moov = position_of(&bytes, b"moov").expect("no moov box");
    assert!(
        moov < mdat,
        "moov must precede mdat — re-encode with `-movflags +faststart`"
    );
    let header = &bytes[..mdat];

    // Sample entry fourcc in the sample description box.
    assert!(
        position_of(header, b"avc1").is_some(),
        "no H.264 track — the Android WebView may refuse to play it"
    );
    // The track handler, not a codec fourcc: `soun` covers every audio codec,
    // where rejecting `mp4a` alone still lets Opus or MP3 through.
    assert!(
        position_of(header, b"soun").is_none(),
        "the splash video must have no audio track"
    );
}

#[test]
fn the_splash_paints_on_the_chrome_red_so_the_hand_off_is_invisible() {
    let css = project_file("assets/app.css");
    let splash = rule_body(&css, ".splash");

    assert!(
        splash.contains("background: var(--color-chrome);"),
        "the splash backdrop must be the chrome token, not a hard-coded red"
    );
    assert!(
        splash.contains("position: fixed;") && splash.contains("inset: 0;"),
        "the splash must cover the whole viewport"
    );
    assert!(
        rule_body(&css, ".splash__video").contains("object-fit: cover;"),
        "the video must fill the screen without letterboxing"
    );
    // Otherwise the WebView flashes its grey play button before the first
    // decoded frame — on a normal launch as well as under reduced motion.
    assert!(
        rule_body(&css, ".splash__video").contains("opacity: 0;"),
        "the video must stay hidden until it has a frame to paint"
    );
    let splash_rs = project_file("src/ui/splash.rs");
    assert!(
        splash_rs.contains("loadeddata") && splash_rs.contains(r#"video.style.opacity = "1""#),
        "the start script must reveal the video once a frame is decoded"
    );
}

#[test]
fn the_logo_fades_in_on_the_design_curve() {
    let css = project_file("assets/app.css");
    let (duration, delay) = animation_timings(rule_body(&css, ".splash__logo"));

    assert_eq!((duration, delay), (240, 300), "DESIGN §8 logo timing");
    assert!(
        rule_body(&css, ".splash__logo").contains("cubic-bezier(0.165, 0.84, 0.44, 1)"),
        "the logo must use ease-out-quart"
    );

    let start = rule_body(&css, "@keyframes splash-logo-in");
    assert!(
        start.contains("opacity: 0.6;") && start.contains("transform: scale(0.96);"),
        "the logo must start at 60% opacity and 0.96 scale"
    );
}

#[test]
fn the_splash_clears_within_the_design_budget_and_rust_agrees_with_the_css() {
    let css = project_file("assets/app.css");
    let (fade_out, fade_out_delay) = animation_timings(rule_body(&css, ".splash"));
    let total = fade_out_delay + fade_out;

    assert!(
        total <= MAX_SPLASH_MS,
        "the splash lasts {total} ms, over the {MAX_SPLASH_MS} ms budget"
    );

    // The overlay is unmounted by Rust; if it went first the fade would jump.
    let splash_rs = project_file("src/ui/splash.rs");
    let unmount: u64 = splash_rs
        .lines()
        .find_map(|line| {
            line.split("Duration::from_millis(")
                .nth(1)?
                .split(')')
                .next()?
                .parse()
                .ok()
        })
        .expect("SPLASH_DURATION in src/ui/splash.rs");
    assert_eq!(
        unmount, total,
        "SPLASH_DURATION must unmount the overlay exactly when the CSS fade ends"
    );
}

#[test]
fn reduced_motion_freezes_the_first_frame_and_the_logo() {
    let css = project_file("assets/app.css");
    let splash_rs = project_file("src/ui/splash.rs");

    // The override has to win the cascade, so it must come after the rules.
    let logo_rule = css.find("\n.splash__logo {").expect("logo rule");
    let query = css[logo_rule..]
        .find("@media (prefers-reduced-motion: reduce)")
        .map(|offset| logo_rule + offset)
        .expect("no reduced-motion override after the splash rules");
    let block = &css[query..];
    let block = &block[..block.find("\n}\n").unwrap_or(block.len())];
    assert!(
        block.contains(".splash,") && block.contains(".splash__logo"),
        "both the overlay fade and the logo animation must be disabled"
    );
    assert!(block.contains("animation: none;"));

    // Playback is script-driven precisely so this query can stop it; an
    // `autoplay` attribute would keep looping whatever the script does.
    assert!(
        splash_rs.contains("prefers-reduced-motion: reduce"),
        "the start script must check the media query"
    );
    assert!(
        !splash_rs.contains("autoplay:"),
        "an autoplay attribute would defeat the reduced-motion contract"
    );
    // Never starting the loop is not enough: with no decoded frame the WebView
    // paints a grey play button over the overlay instead of the first image.
    assert!(
        splash_rs.contains("started.then(() => video.pause())"),
        "reduced motion must pause the loop, not skip it, so a frame is painted"
    );
}

/// `app.css` is a linked asset: until it arrives the overlay is a plain block
/// and the home screen paints through it unstyled, logo at full width. The
/// inline pre-render style has to carry the geometry that makes it cover, and
/// the animations too — an animation started late by a slow stylesheet is still
/// mid-fade when `SPLASH_DURATION` unmounts the overlay.
#[test]
fn the_overlay_covers_the_screen_before_the_stylesheet_arrives() {
    let app_rs = project_file("src/ui/app.rs");
    // The terminator is `);` at the start of a line: `min(46%,220px);` inside
    // the declarations carries the same two characters.
    let pre_render = app_rs
        .split("const PRE_RENDER_STYLE")
        .nth(1)
        .and_then(|rest| rest.split("\n);").next())
        .expect("PRE_RENDER_STYLE in src/ui/app.rs");

    for declaration in [
        ".splash{position:fixed;inset:0;z-index:10;",
        "place-items:center",
        // Without a backdrop the overlay is transparent and hides nothing.
        // Trailing `;`, so this cannot match the `html,body,#main` rule.
        "background:#6B1220;",
        ".splash__video{position:absolute;inset:0;width:100%;height:100%;object-fit:cover;opacity:0}",
        // Unsized, the logo renders at its full intrinsic width.
        ".splash__logo{position:relative;width:min(46%,220px);",
        "@keyframes splash-out{to{opacity:0}}",
        "@keyframes splash-logo-in{from{opacity:0.6;transform:scale(0.96)}}",
        // Animations shipped without their override would run under « Remove
        // animations » for as long as the stylesheet takes to arrive.
        "@media(prefers-reduced-motion:reduce){.splash,.splash__logo{animation:none}}",
    ] {
        assert!(
            pre_render.contains(declaration),
            "PRE_RENDER_STYLE must contain `{declaration}`"
        );
    }

    // Same geometry as the stylesheet, or the overlay jumps when it lands.
    let css = project_file("assets/app.css");
    assert!(rule_body(&css, ".splash__logo").contains("width: min(46%, 220px);"));
    assert!(rule_body(&css, ".splash").contains("place-items: center;"));

    // Same timings too: a different value in the stylesheet is a new
    // `animation-name`/duration, which restarts the animation when it lands.
    let (fade, fade_delay) = animation_timings(rule_body(&css, ".splash"));
    let (logo, logo_delay) = animation_timings(rule_body(&css, ".splash__logo"));
    for declaration in [
        format!("animation:splash-out {fade}ms ease-in {fade_delay}ms both}}"),
        format!(
            "animation:splash-logo-in {logo}ms cubic-bezier(0.165,0.84,0.44,1) {logo_delay}ms both}}"
        ),
    ] {
        assert!(
            pre_render.contains(&declaration),
            "PRE_RENDER_STYLE must contain `{declaration}`"
        );
    }
}

/// `use_future` spawns while the component renders, before Dioxus has patched
/// the node into the DOM: a script started there queries a null element and the
/// loop silently never plays. It has to hang off the video's mount instead.
#[test]
fn playback_starts_on_mount_and_never_during_render() {
    let splash_rs = project_file("src/ui/splash.rs");

    let mount = splash_rs
        .find("onmounted:")
        .expect("the player must start from an onmounted handler");
    let start = splash_rs
        .find("eval(START_PLAYBACK)")
        .expect("the start script must be evaluated");

    // The rule is about the *player*, not about scripting in general: counting
    // every `document::eval` guarded the symptom and broke the day the timer
    // needed to read a media query.
    assert_eq!(
        splash_rs.matches("eval(START_PLAYBACK)").count(),
        1,
        "the mount handler must be the only place that starts playback"
    );
    assert!(
        start > mount,
        "playback is started before the video is mounted"
    );
}

#[test]
fn the_player_is_wired_to_the_bundled_assets_and_stays_silent() {
    let splash_rs = project_file("src/ui/splash.rs");

    for expected in [
        "asset!(\"/assets/splash-loop.mp4\")",
        // The logo is the copy `domain::render` already embeds: a second one
        // would put the APK delta over the 1.3 MB budget.
        "logo_data_uri",
        "muted: true",
        "r#loop: true",
        "playsinline: true",
    ] {
        assert!(
            splash_rs.contains(expected),
            "src/ui/splash.rs must contain `{expected}`"
        );
    }
    assert!(
        !splash_rs.contains("controls"),
        "the splash player must show no controls"
    );
    assert!(
        !splash_rs.contains("http"),
        "the splash must not fetch anything over the network"
    );

    assert!(
        project_file("src/ui/app.rs").contains("Splash { on_done:"),
        "the app shell must render the splash and dismiss it"
    );
    // The WebView gates even muted playback behind a tap by default.
    assert!(
        project_file("android/MainActivity.kt")
            .contains("mediaPlaybackRequiresUserGesture = false"),
        "the WebView must allow the splash to start without a gesture"
    );
}

/// « Remove animations » freezes the overlay on its first frame — which turns
/// the brand beat into a plain wait, since nothing behind it is loading. The
/// Rust timer has to take the same branch the CSS and the player already take.
#[test]
fn reduced_motion_shortens_the_wait_instead_of_freezing_it() {
    let splash_rs = project_file("src/ui/splash.rs");

    assert!(
        splash_rs.contains("REDUCED_SPLASH_DURATION"),
        "the timer must have a reduced-motion branch, not only the animations"
    );
    assert!(
        splash_rs.contains("prefers-reduced-motion: reduce"),
        "the branch must be driven by the media query itself"
    );

    let millis = |name: &str| -> u64 {
        splash_rs
            .split(name)
            .nth(1)
            .and_then(|rest| rest.split("from_millis(").nth(1))
            .and_then(|rest| rest.split(')').next())
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or_else(|| panic!("missing duration for {name}"))
    };
    let reduced = millis("const REDUCED_SPLASH_DURATION");
    let full = millis("const SPLASH_DURATION");
    assert!(
        reduced < full,
        "the reduced-motion wait ({reduced} ms) must be shorter than the full beat ({full} ms)"
    );
    // A splash that vanishes on the first frame reads as a glitch.
    assert!(reduced >= 200, "too short to avoid reading as a flash");
}

/// The overlay is brand time, and nothing waits on it: a tap must end it. She
/// opens the app several times in an evening and has seen the logo already.
#[test]
fn the_splash_can_be_dismissed_by_tapping_it() {
    let splash_rs = project_file("src/ui/splash.rs");
    let overlay = splash_rs
        .split("class: \"splash\"")
        .nth(1)
        .and_then(|rest| rest.split("video {").next())
        .expect("the overlay must exist");

    assert!(
        overlay.contains("onclick:") && overlay.contains("on_done.call(())"),
        "tapping the overlay must dismiss it"
    );
}
