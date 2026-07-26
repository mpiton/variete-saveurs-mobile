//! In-app splash (DESIGN.md §8): the ambient teal loop plays behind the real
//! `templates/logo.png` — the generator never draws the logo, the app overlays
//! it. The logo comes from the copy `domain::render` already embeds, so the
//! splash adds the video and nothing else to the APK. The backdrop is the
//! chrome teal, so dropping the overlay lands on the home screen without a cut.
//!
//! Nothing here waits on the app: the database opens synchronously before the
//! first paint (`app::initialize_database`), so the screen behind is already
//! live. The delay is brand time, bounded by `SPLASH_DURATION`.

use std::time::Duration;

use dioxus::prelude::*;
use tokio::time::sleep;

use crate::domain::render::logo_data_uri;

const SPLASH_VIDEO: Asset = asset!("/assets/splash-loop.mp4");

/// Logo delay (300 ms) + fade/scale (240 ms) from DESIGN §8, a beat to read
/// it, then the 240 ms fade-out that `.splash` runs at 2000 ms — 2.24 s, under
/// the 2.5 s budget. Kept in step with the CSS by `tests/splash.rs`.
const SPLASH_DURATION: Duration = Duration::from_millis(2240);

/// The element carries no `autoplay` on purpose: playback starts from here, so
/// the reduced-motion branch stays in control of it.
///
/// Muting is re-applied as a property — the attribute alone does not always
/// lift the WebView's autoplay gate (`mediaPlaybackRequiresUserGesture` is
/// turned off in `MainActivity`, muted playback still has to be unambiguous).
///
/// Under « Remove animations » the loop is stopped as soon as it starts rather
/// than never started: a `<video>` the WebView has not decoded a frame for
/// paints its own grey play-button chrome over the whole overlay. Playing one
/// frame and pausing leaves the still image DESIGN §8 asks for.
///
/// That chrome also shows for a beat on a normal launch, between mount and the
/// first decoded frame, so the element starts transparent and is revealed here
/// once it actually has something to paint. A video that never loads simply
/// stays hidden and the overlay is the plain chrome teal.
const START_PLAYBACK: &str = r#"
    const video = document.querySelector(".splash__video");
    if (video) {
        video.muted = true;
        const reveal = () => { video.style.opacity = "1"; };
        if (video.readyState >= 2) {
            reveal();
        } else {
            video.addEventListener("loadeddata", reveal, { once: true });
        }
        const started = video.play().catch(() => {});
        if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
            started.then(() => video.pause());
        }
    }
"#;

#[component]
pub(super) fn Splash(on_done: EventHandler<()>) -> Element {
    // The timer is deliberately independent of the player: a video that fails
    // to load still leaves after `SPLASH_DURATION` instead of pinning the app
    // behind a frozen overlay.
    use_future(move || async move {
        sleep(SPLASH_DURATION).await;
        on_done.call(());
    });

    rsx! {
        // Decorative and transient: the home screen underneath is already
        // rendered and is what assistive technology should read.
        div { class: "splash", aria_hidden: "true",
            video {
                class: "splash__video",
                src: SPLASH_VIDEO,
                muted: true,
                r#loop: true,
                playsinline: true,
                preload: "auto",
                // `use_future` spawns during render, before the node exists —
                // the script would then query a null element and never start
                // the loop. Same reason `preview.rs` mounts its gesture script
                // this way.
                onmounted: move |_| {
                    let _ = document::eval(START_PLAYBACK);
                },
            }
            img { class: "splash__logo", src: use_hook(logo_data_uri), alt: "" }
        }
    }
}
