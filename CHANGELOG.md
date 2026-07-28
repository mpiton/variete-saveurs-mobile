# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- The search on the home screen takes a number as well as a name. Every card is
  headlined « Devis n° 10 », the paper she is holding carries that number and a
  client quotes it back over the phone, and it was the one handle the field
  could not use — typing « 12 » answered « Aucune cliente de ce nom » on a
  history that contained « Devis n° 12 ». The field is now labelled
  « Rechercher (nom ou n°) » and its empty state says « Aucun document trouvé »,
  because a search with two handles cannot report a nothing that names only one.
  Digits pass through `normalize_client_search` unchanged, so the accent folding
  that finds « Église » from « eglise » is untouched.

- The IME's action key moves to the next field. Eight fields on the brouillon,
  four on the catalogue sheet, and every one of them ended a keystroke run with
  a keyboard that had nothing to offer but « close ». `enterkeyhint` alone would
  have relabelled the key and moved nothing, which DESIGN.md §7 forbids, so the
  hint travels with the behaviour: `OutlinedField` takes an `enter_key_hint`, and
  `"next"` walks the DOM to the following field of the same screen or sheet. The
  walk is resolved at press time rather than declared at the call site, because
  what is rendered moves — SIRET and the billing address only exist behind the
  « Professionnel » segment, the home search only past fifteen documents. The
  last field of a run is left unset and closes the keyboard.

### Removed

- The ECC bundle, and the app that wrote it. `.claude/ecc-tools.json`,
  `.claude/identity.json`, `.claude/homunculus/`, `.agents/` and `.codex/`
  arrived together in #55 and were never read afterwards: the Codex agent roles
  and the inherited instincts describe a harness this repo does not run, and
  `.agents/skills/variete-saveurs-mobile/SKILL.md` was a stale copy of a skill
  that has since been rewritten by hand.

- `.claude/` is no longer tracked. What is left of it — the hand-written repo
  skill — is a property of the machine editing this repository, not of the app
  it builds, and there is one developer to share it with. The directory stays
  on disk behind a `.gitignore` entry.

### Fixed

- Predictive back actually runs. `DESIGN.md §5` has promised « Back système
  (geste prédictif) partout » since it was written, and the app never delivered
  it: `MainActivity` registered an `OnBackPressedCallback` enabled
  unconditionally, and a callback held at default priority suppresses the system
  animations — back-to-home, and the long-press preview Android 16 gives
  three-button navigation — whatever `android:enableOnBackInvokedCallback` says.
  Back worked; it just never showed her where it was going.

  The callback now stands down when the app has nothing of its own to do with
  Back: no sheet open, no route to return to. Kotlin can see neither, so the web
  side reports one boolean over a single-method `@JavascriptInterface`. The
  sheet half is counted by `BottomSheet` — every sheet in the app goes through
  it, so a sheet added later is covered without anyone registering it — and the
  route half is the router's own `can_go_back`. It starts enabled, so the window
  before the first report behaves exactly as it did before: a missing animation
  is the safe way to be wrong, a sheet that will not close is not.

  The seam is the part no compiler checks — a global name written in Kotlin and
  called from Rust — so the guard reads the name out of `MainActivity.kt` and
  requires it of `app.rs` rather than writing it twice.

- The brouillon is written on the way out, not only every 500 ms. The debounced
  auto-save lives on the screen's scope and Dioxus drops a scope's spawned tasks
  when it unmounts, so leaving inside the debounce window took the last
  keystrokes of a burst with it: Back, or the top bar's menu on the way to the
  Catalogue. « Aperçu » was the only path that flushed, and only because it
  needed the draft on disk for the next screen. CONTEXT.md calls the brouillon
  « auto-sauvegardé en continu » and DESIGN.md §1 puts « la reprise sans perte »
  above the rest; both were true within 500 ms of the last keystroke and false
  after it. A `use_drop` now persists the draft and the half-typed line, behind
  the same guard the debounce and « Aperçu » already carry — an issue in flight
  owns the draft, and a late write would resurrect a document already issued.
  `draft_to_flush` holds that decision alone so it can be tested.

- A failed validation is announced once on the brouillon. The aggregated block
  and the field's own message both carried `role="alert"`, so a single tap on
  « Émettre » read the same sentence twice — three times counting the focus
  `reveal_first_error` moves onto the first faulty control. The block stays the
  live region there, since it lists everything that needs fixing, and those five
  fields pass `announce_error: false`.

  Only those five. The first attempt took the live region out of `OutlinedField`
  itself, which silenced the eight other places that set a field error with no
  aggregated block and no focus move behind them: the recipient on the compose
  screen, the quantity and the price in the line sheet, the quantity in the
  catalogue picker, the name and the price in the catalogue sheet, the sender
  address and the Brevo key in Réglages. For all of those the message *is* the
  announcement. `announce_error` therefore defaults to true — a caller that sets
  an error without offering something better cannot silence it by forgetting a
  prop.

- The heading tree of the two longest screens. The bar's title is the page's
  `h1`; the brouillon then put its own title and all four section titles at `h2`,
  leaving nothing for TalkBack's heading navigation to descend into, and the
  Catalogue jumped straight from `h1` to `h3`. Sections are `h3` under the
  gold-ruled `h2` on the form, and the Catalogue's groups are `h2`. The
  catalogue picker keeps its `h3`: it sits under a sheet title that is already an
  `h2`. `.screen h3` shares the Title Medium rule, so nothing moves on screen.

### Changed

- The logo is base64-encoded once per process instead of once per render.
  `render_document_html` ran a byte-at-a-time encode over 180 KB of PNG on every
  call, and the fiche calls it on every render — each snackbar tick, each share
  phase, each confirmation sheet opening or closing. `LOGO_DATA_URI` is a
  `LazyLock`, and the fiche's thumbnail is a `use_memo` tagged by its document
  id, for the reason `PageCount` gives in `preview.rs`: the router leaves nowhere
  to key `Record` by its document, and Dioxus keeps the instance alive when only
  the route parameter changes, so an untagged cache would paint one document's
  paper under another's number.

- `Cargo.lock` refreshed to the latest compatible versions — `cc`, `either`,
  `foreign-types-macros`, `libc`, `rustls-pki-types`, `syn`, `thin-vec`, plus
  three `windows-*` crates the resolver pulled in for the host build. Patch
  bumps only, no API touched. `cargo audit` reports no vulnerability and
  `cargo deny check` passes; the 18 advisory warnings that remain are all
  unmaintained crates inside the dioxus/wry tree, which `deny.toml` already
  scopes away from our direct deps.

- The two Dependabot alerts open on the repo are explained in `deny.toml`. Both
  are `unsound`, not vulnerabilities, which is why `cargo audit` lets them
  through as warnings while GitHub raises them: glib 0.18.5 (RUSTSEC-2024-0429)
  arrives through muda/tray-icon and compiles for the Linux host only — nothing
  on `cargo tree --target aarch64-linux-android`, so it is not in the APK — and
  rand 0.7.3 (RUSTSEC-2026-0097) is a build-dependency of `selectors` doing
  perfect-hash codegen, never linked. `cargo update --precise` refuses both:
  dioxus 0.7.9 pins `gtk ^0.18` and `kuchiki =0.8.8-speedreader`. They are not
  in `ignore` on purpose, so a reclassification would break the build instead of
  passing unnoticed.

- Two major bumps are deliberately skipped, and `Cargo.toml` now says why.
  reqwest 0.13 removes the webpki-roots feature and verifies against the device
  trust store through `rustls-platform-verifier`, which wants a `JavaVM` handed
  to it before the first request — `platform/mail.rs` picked baked-in roots on
  purpose (ADR 0002), so the upgrade buys new JNI startup code and a TLS failure
  mode that only shows up on the phone. jni 0.22 would put a second copy of the
  bindings in the APK: dioxus, tao and wry are all on 0.21, and only
  `webbrowser` pulls 0.22 today. Both move when the reason to hold them goes.

- The button-shape guard reads the stylesheet instead of a list of known cases.
  It had already been widened once, from the tonal variant to every variant on
  the chrome bar, and it was still scoped to the bar. It now enumerates
  `.m3-button--*` from the CSS, resolves each variant's fill and edge per
  scheme, and measures both against every surface the app poses a button on, so
  a variant added later is covered without anyone remembering to add it.
  `DESIGN.md §2` and `§4` record the rule and the two traps behind it.

- A blank draft is now replaced silently on all three paths that overwrite the
  single draft slot. The duplication path already worked that way, and said why
  in a comment; the conversion and the home « + » still asked. Confirming the
  destruction of an empty draft is a question with nothing at stake, and it is
  the same question that could not name its object.

- Three more places gave way at 200 % system font, all the same rule as the
  floating label: a box that holds text was locked to a pixel height, or a
  threshold governing text was written in pixels. The top app bar title was
  truncated — 318px of « Envoi par email » into 276 — costing the screen its
  only context marker while `DESIGN.md §5` makes those seven titles normative;
  it wraps now and the bar grows with it. « Professionnel » spilled out of a
  segment locked to 40px; segments take a `min-height`, wrap, hyphenate an
  unbreakable word in French rather than overflow, and the label fills its
  button so the group keeps one height instead of two. The line sheet's row
  stayed at two columns until « Descendre » ran past its own button; its
  threshold is `minmax(10rem, 1fr)` now, so it falls back to one column on its
  own. Swept over all 34 screens at 200 %: nothing is clipped anywhere.

  The audit also flagged the catalogue grid as frozen at two columns. Measured,
  its chips wrap and nothing overflows, so it is left alone.

- The catalogue asks how many at the moment she says which. Every line copied
  from the catalogue arrived at `quantity: 1`, so « 40 mini burgers » meant
  closing the sheet, tapping the row, clearing the 1 and typing 40 — once per
  line, five to ten times a quote, in the evening session `PRODUCT.md` describes
  as several documents in a row. Tapping an item now opens a prompt under the
  grid: the item's name, a numeric field, « Ajouter », then back to the grid
  with the counter up. An empty field means one and the placeholder says so, so
  one of something is still two taps while forty is typed straight in. Tapping
  another chip switches item rather than adding the pending one, and « Terminé »
  steps down to tonal while the prompt is up so the sheet has one primary
  action. The quantity rule — bounds and French message — is the line sheet's
  own, shared rather than copied, so a quantity means the same thing wherever
  she says it.

  The audit proposed ± steppers on the row for this. That is the wrong
  affordance for a caterer: reaching 40 would be 39 taps.

- A floating label no longer covers the value it names when the system font
  grows. The field reserved `--space-xs` — 6 fixed px — for a label sized in
  rem, so at 200 % there were 24px of label in 6px of room, and 42px once a long
  one wrapped to two lines, on a field 50px tall: the value became unreadable at
  exactly the size chosen to make it readable. The label sits in the flow now
  and takes back half of its own line box, so it takes the height it needs at
  any scale and however many lines, and only its last line straddles the border.
  Measured at 412×915: 6px of overlap on a 48px field at 100 % — the M3 notch,
  unchanged — 8px at 130 %, and 12px on 50px at 200 % where it was 42. The
  loading spinner moved into the input's grid row for the same reason: its
  20px offset assumed the input never moved.

- The text fields own their outline too, which closes the family the previous
  pass opened. An M3 outlined field *is* its outline — its background is the
  panel's own white, so there is no fill to fall back on — and it had stayed on
  the container rail at 1.53:1. Raising the suggestions and the chips first made
  that visible as an inversion: the field she types into all evening was the
  faintest control on screen while the list serving it was the clearest. Both
  `input` and `textarea` take `muted` now: 6.57:1 in a panel, 7.05:1 dark, and
  5.78:1 on the cream for the history search that sits straight on the page.
  Focus and error still take the border, which is what a neutral at rest is for.

- Four controls whose outline was their whole affordance are visible again. The
  inactive segment sat at 1.53:1, a catalogue chip at 1.30:1 with a fill the
  same white as the sheet behind it, a client suggestion at 1.30:1, a FAB menu
  item at 1.34:1 — all under the 3:1 `DESIGN.md §2` asks of a control, and all
  missed because that rule had only ever been applied where a defect had shown
  up. The value is chosen per element rather than uniformly: `primary` for the
  segmented group, since the three segments draw one outline and the group is
  the interactive object; `muted` for the chips and the suggestions, where
  twenty red outlines would be a wall and a suggestion must not shout louder
  than the field above it; `ink` for the FAB menu items, the same answer as
  `.app-menu` because it is the same situation. Worst case after: 5.42:1.

  The tappable containers — document card, draft card, form line, the lines
  fold — are deliberately left at 1.30 to 1.63:1. `DESIGN.md §4` already
  assumed that limit for the flat 1px parti, and §4 now says explicitly that it
  covers tappable containers too, so the rule in §2 and the limit in §4 stop
  overlapping in silence.

- The top app bar's menu keeps a shape over everything it covers. It is the one
  panel that floats free — it overlaps the bar and spills onto the page and onto
  whatever card is beneath it — and the container rail left it at 1.34:1 on the
  cream and 1.53:1 on a card. In the dark scheme nothing carried it at all: the
  elevated fill measures 1.02:1 against the dark chrome and the shadow is pure
  black on a near-black page, so it is inert. No single palette value fixes the
  light scheme, since clearing 3:1 on both `#6B1220` and `#FFFFFF` needs a
  relative luminance between 0.207 and 0.30 and the palette holds nothing there.
  The edge is `ink` now, so the fill carries the chrome and the edge carries the
  content: measured at render against the background actually painted beneath,
  12.12 / 11.09 / 12.60:1 light and 12.43 / 15.01 / 13.74:1 dark, where dark was
  1.35 / 1.63 / 1.49:1.

- The preview says how many pages the exported PDF will have, before she issues
  it. The screen renders `render.rs` HTML — one continuous strip with no pages —
  while the file she sends is laid out by Typst (ARCHI §5), so a quote that fell
  badly across pages was invisible until after the document was frozen. The
  count now comes from the compiler that produces the PDF, on a worker, and is
  absent until it answers rather than estimated.

  Estimating was tried first and measured against the truth on fourteen
  documents: the HTML height disagrees with the real layout twice and in both
  directions — 12 lines make one HTML page and two PDF pages, 58 lines make
  three HTML pages and two PDF pages — so no page height reconciles them, and
  drawing page marks from the strip would have been wrong exactly at a
  boundary, which is the only place the answer matters. `templates/` was left
  byte-identical to the frozen desktop's throughout.

- The draft screen borrows the letterhead rule from the document it produces.
  `DESIGN.md §1` says the register is « rouge/rose/or/crème/brun partout »,
  paper and screen alike, and gold — the hue that rules the A4 header — existed
  nowhere in `app.css`: the app had the document's palette and none of its
  forms. The screen title now steps up to Title Large under a 2px gold rule,
  and the five section headings stay at Title Medium beneath it; they were all
  the same size and weight, on the longest screen of the app. One rule, not
  five: a mark rather than a grammar. Ornament, not information — 2.29:1 on the
  cream where a control would owe 3:1 — and a single value serves both schemes,
  clearing 6.57:1 on the dark surface unchanged.

- `templates/document.css` and `assets/app.css` are checked against each other.
  `DESIGN.md §2` claimed seven of the app's light tokens were Vitrine values
  verbatim and nothing verified it, so the document could have drifted from the
  screen that extends it without a word. The template declares eight colours;
  the test requires every one to exist, to the digit, in the light scheme.
  Removing `--color-gold` fails it with `#C49A45 rules the document and has no
  token on screen`.

- The record leads with the document. Issuing is the one irreversible act in
  the app, and it landed her on a summary of the fields she had just typed:
  six undifferentiated muted lines, badges, and « Aperçu » as an outlined
  button underneath them. `PRODUCT.md` principle 3 calls the document the
  product, and it sat at the third level of navigation. An A4 thumbnail now
  opens the record beside the number, the client and the total, « Voir le
  document » is the screen's one primary action, and the metadata moves under a
  « Détails » fold — it confirms a document once found, it does not help find
  one. The thumbnail is the same `render_document_html` the preview shows,
  scaled and clipped to the head of the page, deliberately not the exported
  PNG: that export runs in the background and can fail, and the thumbnail would
  then be missing exactly when it matters. It is inert — `aria-hidden`,
  `tabindex="-1"`, `pointer-events: none` — because its content is already on
  screen as text and an image that looks tappable without being tappable
  promises a gesture the app does not have. Measured at 412×915: the head sits
  in two columns at 100% and 130% and wraps to one at 200% system font, with no
  horizontal overflow at any of the three.

### Fixed

- The fiche appears when the document exists, not when its files do. « Émettre »
  committed the number, cleared the draft, then compiled the PDF and rendered
  the PNG before publishing anything — so for the length of a Typst compile
  (~1 s) she sat on a form whose draft had already been deleted, with a spinner
  inside a button as the only sign of life, at the one moment in the app that
  cannot be undone. Long enough to read as a hang, and a force-close there hides
  an emission that already happened. The emission now publishes the fiche as
  soon as the number is committed and exports behind it, which also unsticks the
  « Génération du PDF en cours… » line the fiche has carried since it was
  written for this moment — it was unreachable, only a manual re-export ever set
  that phase. A failed export and the « Devis n° 10 émis » snackbar no longer
  land in the same frame either.

- A draft she never typed in is replaced without asking, as DESIGN.md §6 says.
  The rule was there and unreachable: `is_blank` counted the issue date as
  content, and every draft is created with today's date already stamped, so no
  freshly opened draft was ever blank. Creating a devis, going back, then
  creating a facture raised « Devis — 0,00 € sera remplacé par un document vide,
  sans retour possible » over an empty document — on all three overwrite paths,
  three times in an evening of several documents. The one sheet that has to be
  believed was the one she was learning to dismiss, and there is no undo behind
  it. The issue date is machine-stamped and cannot say whether she wrote
  anything; the event date, which is always her choice, still counts. Three
  tests changed sides, and a new one ties the two halves together — the draft
  the home actually creates is now asserted blank, which is the check neither
  `models.rs` nor `home.rs` was making on its own.

- The « Remplacer le brouillon ? » sheets say what they destroy. All three
  described what was *arriving* — an empty document, the pre-filled invoice, a
  copy — and never what was leaving, which is the only thing actually lost and
  there is no undo anywhere in the app. « Le brouillon actuel » was the whole of
  what she knew about a quote she may have spent an evening on. The sheets now
  name it by who it is for and what it comes to (`Devis pour Mairie de Lyon —
  340,00 €`), and say that there is no way back. Measured at 412×915 in the
  error state, worst case: the sheet goes from 29% to 34% of the screen at
  normal font size and from 63% to 76% at 200%, with no horizontal overflow and
  no clipped text on a 46-character client name.

- The draft resume card on the home screen names the draft too. It showed
  « Reprendre le brouillon » and the kind, which does not tell two quotes apart.

- A failed validation takes her to the first field to fix. On a five-section
  form she taps « Émettre » from the bottom, and the aggregated block renders
  above the action bar with the faulty fields further up still: nothing moved,
  so the tap read as a broken app and she tapped again. The screen now scrolls
  the first faulty anchor into view and focuses it — instantly, since an error
  path is no place to wait for a camera move. Anchors that cannot take focus
  (the lines heading, the total) scroll; their message already carries
  `role="alert"`.

- The draft preview no longer swallows a validation failure outright. Its
  « Émettre » ran the same gate as the form, but that screen has neither a
  block to show the errors in nor a field to fix, so the tap did nothing at
  all. It goes back to the form, which shows them and reveals the first faulty
  field on arrival.

- Tonal and outlined buttons own a shape wherever they sit, not only on the
  chrome bar. `DESIGN.md §2` asks every control for a fill or an edge clearing
  3:1 against the surface under it, and the fix shipped for the bar was never
  extended: off the bar the tonal kept the raw tint — 1.23:1 on a card, 1.08:1
  on the cream, 1.04:1 on a dark sheet — and the outlined carried
  `--color-border`, the §4 container rail, at 1.53:1. 19 of those two variants'
  24 call sites had no perceptible boundary. Both now take a `primary` edge
  (4.83 to 6.16:1 light, 7.55 to 9.31:1 dark) and keep their fill; the bar drops
  that edge and keeps its own pair, primary being 1.97:1 on the light chrome.
  Measured over 34 reconstructed screens in both schemes, the weakest signal in
  the app moves from 1.04:1 to 3.08:1 — the dark chrome container hitting the
  M3 threshold it was designed for.

- The preview screen reaches the bottom of the window again. `height: 100%`
  resolved against `.screen-scroll`'s content box, already short by the padding
  the screen's own negative margin cancels, so the screen and its action bar
  stopped 18px high and the Android navigation band was painted in content
  colour — 66px once the export snackbar rendered, since it sat after the bar
  instead of before it. `min-height: calc(100% + var(--space-lg))`, and the
  snackbar moved ahead of the bar; measured at 915 on all three preview states,
  both schemes.

- The bottom-sheet scrim has a dark value. It was inlined as
  `rgb(30 18 10 / 0.45)` with no token and no dark override, and that brown is
  *lighter* than the dark page (0.0073 against 0.0061 relative luminance), so it
  lifted what it was meant to push back. It is `--color-scrim` now, pure black
  at 60% in the dark scheme — the same measurement that already governs the
  shadow there.

- CI gate tools bumped along with the action pin that installs them:
  cargo-llvm-cov 0.8.5 → 0.8.7, cargo-audit 0.22.0 → 0.22.2, cargo-deny
  0.19.4 → 0.20.2, `taiki-e/install-action` v2.84.0 → v2.85.2. `deny.toml`
  needed no change under 0.20, checked locally before the pin moved. The
  `dtolnay/rust-toolchain` comment claimed a 2026-06-06 pin; the SHA next to it
  is from 2026-07-16 and is still master, so the date was corrected rather than
  the pin.

- The coverage gate runs unconditionally. It sat behind a grep for a function
  in `src/domain/`, because llvm-cov exits 1 when it measures nothing and the
  directory was an empty stub — dead since task 03, and a gate whose regex
  could quietly stop matching is worse than no guard at all.

### Fixed

- `.claude/skills/variete-saveurs-mobile/SKILL.md` described a different
  project. Auto-generated, it claimed camelCase file names (there are none —
  45 files, all snake_case, and clippy would refuse otherwise), tests in
  `*.test.rs` files (none exist; tests are inline `#[cfg(test)]` modules plus
  `tests/*.rs`), an unknown test framework, and only `feat`/`fix` commit
  prefixes. The whole file was also wrapped in a stray ``` fence with no
  frontmatter, which is why its description read as ```` ```markdown ````.
  It matters more than a stale doc usually would: `CLAUDE.md`, `ARCHI.md`,
  `DESIGN.md`, `PRODUCT.md` and `CONTEXT.md` are all in `.git/info/exclude`, so
  this was the only conventions document a clone actually contains. Rewritten
  from the repository, and it now carries what the local documents do not — the
  three renderings of the document and why the HTML cannot predict the PDF's
  pagination, how to measure a layout rule no Rust test can reach, and the
  guard-the-rule-not-the-instance discipline that three repeated defects paid
  for.

- `CLAUDE.md` listed a coverage command that cannot run: `cargo llvm-cov
  --fail-under-lines 85 -p devis-mobile --lib` exits with « no library targets
  found in package `devis-mobile` », the crate being a binary. It now shows the
  invocation CI actually uses, exclusions included.

- `README.md` still said the code was arriving with the sprint. The app is on
  the phone; the file now lists the real source layout, links the two ADRs it
  was missing, and says plainly that the shipped APK does not come out of a `dx`
  command. It points only at files the repo actually contains — `CLAUDE.md`,
  `ARCHI.md` and `DESIGN.md` live in `.git/info/exclude`, so linking them from
  the one README that *is* tracked would be a dead link for anyone but us.

- `CONTRIBUTING.md` had `TODO: Add install commands` and `TODO: Add test
  commands` where the setup belongs. With the working notes excluded from the
  repo, that template stub was the only tracked description of how to build and
  check this project, and it described nothing. It now carries the toolchain
  setup and the five CI gates of ARCHI §8 verbatim — numbered there, since audit
  and deny are one gate in two commands and a flat list reads as six — plus the
  dependency and changelog rules a PR is held to. Its « Code of Conduct » link pointed at a `CODE_OF_CONDUCT.md`
  that was never written — the repo is public, so that was a 404 for anyone who
  clicked it. Replaced with the two lines it would have said.

- The command list in `CLAUDE.md` called `dx build --platform android --release`
  the « APK signé à installer sur le téléphone ». Its own §Release process says
  the opposite, and says it in bold: dx assembles the *debug* Gradle variant, so
  that APK is `debuggable="true"` and signed with the debug keystore — installed
  on the phone, `adb run-as` would read the accounting database and the Brevo
  key out of it. The two sections now agree, and the command list points at the
  `gradlew assembleRelease` sequence instead of pretending to replace it.

- `dx` is pinned. The setup step said `cargo install dioxus-cli` with no version
  while the release process reasons specifically about what dx 0.7.9 does with
  the Gradle variant — the tool that assembles the APK was the one dependency
  free to drift out from under its own documentation. Now `@0.7.9 --locked`,
  matching the dioxus crate.

## [0.1.2] - 2026-07-26

### Fixed

- The Brevo key can be pasted. The field opened as a `password` input, and OEM
  keyboards answer that with a « clavier sécurisé » that disables the clipboard
  — on a Realme there was no way in but typing a Brevo key out by hand, which
  nobody is going to do, and getting one character wrong costs an envoi that
  fails with no clue why. The field now opens revealed; « Masquer la clé » is
  still one tap away. Masking bought nothing anyway: `spellcheck="false"` and
  `writingsuggestions="false"` already keep the key out of the IME's suggestion
  strip, its personalised learning and the browser's own suggestions, and the
  autofill framework is refused separately. What is left is a shoulder to hide
  from, and she enters this alone, once.

## [0.1.1] - 2026-07-26

### Changed

- The app is called « Variété de Saveurs » on the phone. `dx` scaffolds a
  `strings.xml` naming it after the crate, so the icon on her home screen, the
  app switcher and every share sheet read « DevisMobile » — a developer's name
  for it, and not the one the documents and the emails already carry. Ours is
  copied over `dx`'s by the `build.rs` that was already replacing its
  `styles.xml`, so no new mechanism. Verified on the built APK, where
  `application-label` now reads the marque, accent included.

- The release process in `CLAUDE.md` now describes the sequence that actually
  produces the APK we ship. Step 5 claimed `dx build --platform android
  --release --device` emitted one signed with the keystore. It does not: dx
  compiles the Rust side in release but assembles the *debug* Gradle variant, so
  the APK comes out `android:debuggable="true"` and signed with
  `~/.android/debug.keystore` — and debuggable means `adb run-as` reads the app's
  private storage, which holds her accounting and the Brevo key. The release
  variant only comes from `gradlew assembleRelease` on the project dx generates;
  `zipalign` and `apksigner` follow, reading the password from a file outside
  the repo rather than from `Dioxus.toml`'s `[android.signing]`, which would
  want it committed in clear. Written down while cutting v0.1.0, which is
  therefore built that way. The `versionCode` rule is now marked as what it is —
  unmet, dx bakes it to 1 with no key to change it (ADR 0004).

## [0.1.0] - 2026-07-26

Première version installée sur le téléphone de la gérante. Elle rédige un devis,
l'émet, l'exporte en PDF et en PNG fidèles au template desktop, le partage ou
l'envoie par email, et le convertit en facture — hors ligne, sans compte.

### Added

- Réglages can now update the app itself (ADR 0004). The APK ships direct, never
  through the Play Store, so a fix only ever reached the phone if someone came by
  with a cable — and nothing on the phone said a fix existed. « Vérifier les mises
  à jour » reads the latest published GitHub release (public repo, so no token to
  protect), compares its `vX.Y.Z` tag against the running version numerically, and
  offers the attached APK; anything not served from `github.com` is refused. The
  install permission is checked before the download, not after tens of megabytes
  are spent, and when Android has not granted it the screen opens « Installer des
  applications inconnues » itself. The APK streams into the app cache under a
  `.part` name and is renamed only once flushed, so a dropped connection leaves
  nothing the installer would open and reject. Handing it over is an Intent — the
  system installer owns the confirmation, the progress and any failure. Nothing
  runs at startup: the check is that button and nothing else, keeping the network
  off the saisie path. The section states, in every phase, that a mise à jour
  keeps her devis, factures and exports and that uninstalling is the one thing
  that would erase them.

- « Afficher la clé » under the Brevo key field in Réglages. The key is a long
  opaque string typed into a `password` input, so a typo left no trace until an
  email was refused — discovered from the compose screen, after writing the
  message, with a client waiting, and worded so it didn't say whether the key,
  the sender address or the recipient was at fault. The toggle only ever
  applies to the candidate being typed: `new_api_key` is never filled from the
  store, the stored key still shows as « configurée » and nothing reveals it.
  Saving clears the candidate and the reveal together, so a shown field can't
  outlive its own save. Revealed, the field is plain `text`, which hands back
  the text-assistance surfaces `password` suppressed for free — so it also
  carries `spellcheck="false"` and `writingsuggestions="false"`, keeping the
  key out of IME learning and browser suggestion UIs.

### Changed

- The record screen no longer offers six flat actions. Its chrome bar took 422
  px of a 915 px screen — starting at mid-height, not in the bottom third the
  design asks for — and truncated the line list behind it, with nothing saying
  what to do first. The bar now carries only the two paths that reach the
  client, « Partager » and « Envoyer par email », one column each: PRODUCT.md
  keeps the delivery paths at parity, so neither takes the filled weight or the
  wider column. « Aperçu » moves onto the summary card, where looking at the
  document belongs to the document. « Convertir en facture » and « Dupliquer »
  move into the body as outlined buttons — real actions, but accounting
  maintenance rather than delivery.

- The catalogue sheet stays open while picking. It closed on every chip, so a
  traiteur quote of five to ten items cost seven interactions each; picks now
  accumulate behind a « Terminé » button, with a live count as the receipt for
  taps that land behind the scrim.

- The home screen can be searched by client name, past 15 documents. That is the
  handle she actually has — « le devis de la mairie » — so it is the only thing
  the search matches, through the same accent- and case-folding the form's
  autocomplete uses: « eglise » finds « Église » on both screens. It combines
  with the kind filter, and it does not survive navigation, unlike the filter:
  one looks something up, consults it, and comes back to the whole list.

  The field only appears past the threshold, on the size of the history rather
  than on what is displayed. Below it the list is short enough to read, and a
  permanent field would be a fifth thing competing for a home screen already
  found crowded; at a few documents a month it lands near the end of the first
  year. No month separators and no pagination: she does not look by period, and
  the volume never gets there.

  The empty state now tells three nothings apart — no documents, nothing in this
  filter, no client of that name — each with the action that undoes the cause.
  « Aucun document » on a history full of quotes, because the filter said
  Factures, sent her looking for a bug that was not one.

- The share sheet names printing. PRODUCT.md keeps the three deliveries at
  parity and the app never wrote the word — it is reached through the Android
  chooser, so the sheet is the only place it can be said. « Partager le
  document » becomes « Partager ou imprimer », with one line naming where the
  choice happens. No `PrintManager`: that would be a v1 scope change, and the
  path already exists.

- Containers posed on the page take the firm border, rows posed inside a
  container keep the soft one. On the cream, the soft filet reached only 1.15:1
  against the background and the document list read as one continuous block;
  the firm one reaches 1.34:1 in light and 1.63:1 in dark. Still short of WCAG
  1.4.11's 3:1, and deliberately so: no Vitrine value clears that threshold
  against the cream without leaving the palette. Documented as a limit of the
  flat parti, not as a solved problem.

- The splash is no longer a wall. Nothing waits on it — the database opens
  before the first paint — so a tap dismisses it, and under
  `prefers-reduced-motion` the wait drops from 2.24 s to 320 ms instead of
  freezing a still image for the full brand beat. She opens the app several
  times in an evening.

- The email body is plain text again. « Message » held the whole of
  `templates/email.html` after substitution — `<!DOCTYPE>`, a presentation
  table, inline styles — in an editable textarea whose content went straight to
  Brevo, so one keystroke inside a tag shipped a broken branded mail to a
  client. `CLAUDE.md` asks for the opposite: « modèle fixe, texte retouchable ».
  The template now carries a `{message}` placeholder; `render_email` returns
  the sentences addressed to the client as plain text, and `render_email_html`
  wraps them at send time — blank lines open paragraphs, single newlines become
  `<br>`, everything she typed is escaped. The greeting, the signature, the
  validity sentence and the letterhead are no longer hers to break.

- Screen titles come from `CONTEXT.md` instead of the router: « Formulaire » →
  « Brouillon », « Fiche » → « Document émis », « Composition » → « Envoi par
  email ». The paths are unchanged — they are history entries.

- Document cards carry their issue date, beside the status badges. The list had
  no date and has no ceiling, so finding last week's quote was a scroll.

- The segmented filter announces exclusivity. Three mutually exclusive filters
  were a `role="group"` of `aria-pressed` toggles, which TalkBack reads as
  « activé » with no sense of the set; they are now a `radiogroup` of radios
  read as « coché, 2 sur 3 ». The active-segment rule follows the attribute the
  component emits, and a test pins the two together — the previous pairing had
  already drifted once. No arrow-key roving: the app is touch-only, and the
  segments stay in the normal tab order.

- The app menu is a disclosure rather than an ARIA menu. Its three children
  carried `role="menuitem"` under a plain `<nav>` with no `role="menu"`, and
  the trigger advertised `aria-haspopup`. Both are gone: `aria-expanded` plus
  `aria-controls` describes what the code actually implements, without
  promising the arrow-key roving an ARIA menu implies.

- The app runs on the document's palette (DESIGN §2, task 33): the
  deep-teal « Le Comptoir » scheme is gone. Seven light tokens are now the
  Vitrine values verbatim (`bg` = Crème Vitrine #F6F0E2, `ink` = Brun
  Cacao, `surface-dim` = Filet, `muted` = Gris, `on-chrome-muted` and the
  dark `primary` = Rose Praliné, `primary` = Rouge Enseigne), the rest
  derive from them, and the screen stops being a second universe next to
  the paper. `chrome` is #6B1220 — Rouge Enseigne darkened to the tone
  measured in the splash loop, so the system window, the video and the top
  app bar are one colour through the cold start; white on it reads 12.12:1,
  slightly better than the teal it replaces. Dark `primary` moves to Rose
  Praliné because Rouge Enseigne only reaches 2.7:1 on a dark surface.
  `danger` becomes M3's own #B3261E and changes role rather than hue: no
  legible red clears 3:1 against the brand red, so the two are told apart
  by what they do — primary fills, danger only outlines and labels, always
  under a French message. `PRE_RENDER_STYLE`, both `vs_colors.xml` and the
  two `MainActivity` chrome constants follow, and a new test walks `src/`,
  `assets/` and `android/` to fail on any surviving teal literal. No logic
  changed; `templates/document.css` is untouched, so the exported A4 is
  byte-identical.

### Fixed

- The system navigation icons were dark on the red action bar in the light
  scheme — 1,7:1, on the four screens out of seven whose chrome reaches the
  bottom edge. Kotlin derived their appearance from `uiMode` alone, and it has
  no way of knowing which screen the WebView is showing, so no value was right
  everywhere: light icons would have been invisible on the cream of the three
  screens without an action bar. The band is chrome on all seven instead — the
  three others paint the inset they already reserved rather than leaving it in
  content colour — and the icons are white everywhere, at 12,12:1. The bottom
  sheet paints it too: it is in the top layer, so it covers that band over any
  screen at all, and its elevated white would have taken the icons to 1:1. Above
  the keyboard the band belongs to the IME, which paints it and sets its own
  icons, so nothing is reserved there. No page-to-Kotlin channel to keep in
  sync, and the red frames the cream top and bottom.

- The line being typed is persisted with the draft. It lived in a sheet, so it
  existed only in memory: a WebView the system recycled took it with it while
  the rest of the draft survived, and « Pièce montée 60 choux / 3 / 450,00 € »
  had simply never happened. It now goes to its own one-row table, on the same
  debounce as the draft, as raw text — a half-typed « 12, » is a legitimate
  price that no numeric column can hold. Clearing the draft clears it too, so an
  editor can never reopen over a blank form. Two things are deliberately not
  restored: the armed-delete window, a 400 ms safety guard that would come back
  as a trap, and the validation messages, which are recomputed from the values.

- Back left the screen instead of closing an open bottom sheet, losing the line
  being edited, the catalogue picks or the confirmation about to be answered.
  The sheets are `<dialog>` elements with an `oncancel` handler, but the Android
  callback was registered as always-enabled and called `goBack()` straight away,
  so the WebView never saw the key. Back now cancels the topmost open sheet
  first — through the component's own handler, which still refuses to dismiss a
  sheet whose job is running — and only moves in history when there is none.
  Verified on device: sheet closes, screen stays; back again returns to the
  list; back at the root leaves the app. No Rust test covers this: the logic is
  Kotlin, which is why it survived until a phone was plugged in.

- A disabled `textarea` looked exactly like an active one: the 38 % opacity rule
  listed `button` and `input` only, so the email body dimmed nothing but its
  floating label. `textarea` and `summary` were also keeping the UA's grey tap
  highlight instead of the project's.

- Short documents stretched their own buttons. `.record-screen` and
  `.compose-screen` inherited `display: grid` and added `min-height: 100%`, so
  the auto tracks shared the leftover space — buttons measured 53 to 85 px
  against the 48 px they specify, and a collapsed `<details>` reached 98 px for
  a 48 px summary. Both are flex columns now, like `.form-screen` already was:
  items keep their size *and* `margin-top: auto` still anchors the sticky action
  bar to the bottom. (`align-content: start` fixes the stretch but silently
  breaks that anchor, and the bar then floats mid-screen — caught on the phone,
  not by the tests.)

- Issuing asked for confirmation before knowing whether it could issue.
  Tapping « Émettre » read the next number and opened the sheet naming it;
  validation only ran afterwards, in the worker. She endorsed an irreversible
  act for a document the system already knew was invalid, then watched the
  sheet close on an apparently unchanged screen — with no way to tell whether
  the number had been spent. It never was, `peek_next_number` only reads the
  counter, but nothing said so. `check_before_issue` now gates both entry
  points: an invalid draft goes straight to the ordinary error path and the
  sheet is never shown.

- The primary action had no shape on the chrome bar in the light scheme.
  « Émettre le devis » and « Envoyer » are `--filled`, so Rouge Enseigne #C0182B
  on the chrome #6B1220: **1,97:1** — the very figure DESIGN cites to forbid
  Primary as *text* on the chrome. The label stayed legible; the button stopped
  looking like one. The fill is kept, because it is what tells the primary
  action apart from the tonal beside it, and the shape is carried by an edge:
  white at 12.12:1 in light, stepping aside in dark where the Rose Praliné fill
  already clears 7.71:1. The guard now walks every variant the bar carries, in
  both schemes, and accepts either signal — the previous one covered only the
  variant where the bug had first appeared.

- Four defects introduced by the fixes above, all found by a second critique
  that was deliberately not told what had changed:
  - `home-confirmation-actions` (`record.rs:428`, `468`) survived the rename to
    `confirmation-actions`, which only touched `home.rs` and the stylesheet.
    Measured side by side: the right class gives `display: flex`, `gap: 8px`,
    right-aligned; the orphan gives `display: block`, 0 px apart, left-aligned —
    in both confirmation sheets of the record screen, one of which carries a
    destructive button.
  - The bottom inset was counted twice on the home screen: `.screen-scroll`
    reserved it for a screen with no chrome bar, and `.home-screen` already
    added it on top of its 72 px of FAB clearance. Measured Δ of 72 px for 24 px
    of inset, which made the list scroll by the inset alone. Same rule as the
    fix that introduced it — the inset now sits only in the FAB clearance's
    container.
  - The preview kept a broken mirror: `.preview-screen`'s negative bottom margin
    cancelled a `.screen-scroll` padding that had become 0, leaving an 18 px band
    of content colour under the action bar. The margin is 0 and the comment now
    says which padding it mirrors.
  - The FAB's reduced-motion escape was declared 89 lines *before* the transition
    it cancels. Equal specificity, later wins: measured `transition-duration:
    0.2s` under « Remove animations ». Every motion escape now lives in the block
    at the end of the file.

- Two guards were rewritten because they had been written against the symptom
  rather than the rule, and both passed while the defect was live: the FAB
  reduced-motion test asserted that some block mentioned the selector, never
  that it came after; the splash test counted every `document::eval` instead of
  the playback call it was actually protecting.

- The bottom system inset was counted twice, so the chrome never reached the
  screen edge. `.screen-scroll` reserved it and `.chrome-action-bar`, one of its
  descendants, reserved it again; a sticky bar's constraint rectangle is the
  scrollport minus the scroll container's padding, so the bar parked
  `18 + inset` px above the bottom. The navigation band was painted in the
  content colour instead of the chrome, and content scrolled into it — exactly
  what DESIGN §5 rules out. The inset now belongs to the bar when there is one
  and to the scroll container when there is not
  (`:not(:has(.chrome-action-bar))`), so it is counted once on that axis. A test
  pins the invariant.

- The only route to the settings once the home prompt is dismissed was a 63 × 19
  px inline link inside the disabled-send hint — a third of the 48 dp floor. It
  is now a full-width text button, « Ouvrir les Réglages », in white on the
  chrome (Rouge Enseigne reads 1.97:1 there and was never an option).

- « Ajoutez au moins une prestation » pointed at nothing. The message appeared
  only in the aggregated block at the bottom of the form, while the Prestations
  section — several screens up — kept saying « Aucune prestation pour
  l'instant. » with no error treatment. The section now carries the message
  itself when it is the one at fault.

- Issuing a document asked nothing. It assigns the number for good, freezes the
  document and empties the draft, and the only way back is a duplication that
  spends another number — while deleting a single line asks twice, 400 ms
  apart. Both entry points now open a confirmation naming what is about to
  happen: « Émettre le devis n° 10 ? », with the rule spelled out. The number is
  peeked through `numbering::next_number`, never reserved, so cancelling leaves
  the counter untouched.

- A new draft opened with both dates blank, and both are required. The issue
  date now defaults to today — it is the day she writes it, and conversion and
  duplication already date themselves that way. The event date deliberately
  stays empty: a plausible wrong default would ship inside a document she can no
  longer amend. `blank_draft` takes the date from its caller rather than reading
  the clock, so the test stays deterministic.

- The app menu and the FAB menu could not be dismissed by tapping beside them;
  only choosing an entry closed them. Both now follow the same contract as the
  form's client suggestions — any tap or scroll elsewhere in the shell closes
  them, and the triggers stop their own taps from reaching the shell.

- The home filter reset to « Tous » on every return from a document. It now
  survives navigation for the session; a cold start still opens on everything.

- The database-open failure now says what to do, and what not to do: reopen the
  app, and do not uninstall it, since the documents live in its storage.

- Tonal buttons vanished from the chrome action bar in the dark scheme.
  `--color-primary-tint` (#3A181C) and `--color-chrome` (#4A0C16) sit at the
  same luminance, 1.02:1, so the six actions of the record screen, the form's
  « Aperçu » and the preview's three buttons lost their shape and left only
  floating labels — indistinguishable from the help sentence beside them. The
  chrome bar now carries its own pair, `--color-on-chrome-container` /
  `--color-on-chrome-container-label`, resolving to the plain tint in light
  (unchanged at 9.85:1) and to #B44E58 with white in the dark, which clears
  M3's 3:1 for the fill (3.08:1) and AA for the label (5.03:1). A regression
  test checks both ratios in both schemes.

- Snackbars were painted underneath the record's action bar. They were rendered
  after the sticky block and `.snackbar` carried no positioning of its own, so
  « Devis n° 10 émis » and « Email envoyé » — the only confirmation either
  action produces — landed behind it. They now sit inside the sticky block,
  above the bar.

- The catalogue chip dropped the unit its own Catalogue screen displays: the
  same item read « 0,85 € » in the picker and « 0,85 € / pièce » in the list.
  One formatter now serves both, and it covers the chip's accessible label too.
  For a traiteur selling to the piece, the unit is what makes the price read.

- Keyboard focus was invisible on two controls. The global `:focus-visible`
  ring listed `button, a, input`, so the email body (`textarea`) and the
  record's line list (`summary`, a 378×48 target) took focus with nothing to
  show for it.

- The FAB kept its plus while its menu was open — only the accessible label
  changed. The glyph now turns a quarter, which is the whole icon since a plus
  is symmetrical, and the rotation cuts rather than travels under
  `prefers-reduced-motion`.

- Long button labels collided at large system font scales: `.m3-button` set
  `line-height: 1`, so « Exporter le PDF / PNG » wrapped onto two touching
  lines around 130 %. Now `1.2`; the single-line case is unchanged, the button
  being centred inside a min-height.

- The startup database error rendered through `.startup-error`, a class with no
  rule anywhere, on a `<p>` sitting outside `.screen` — it fell back to browser
  defaults on an empty screen. It now uses the same `ErrorBlock` as every other
  error path, titled « Base de données inaccessible ».

- Email archive copy with a `noreply@` sender (ADR 0002): the BCC no
  longer goes to the unread `noreply@` mailbox — `archive_address`
  (domain, pure) maps `noreply@<domain>` to `contact@<domain>`, any
  other sender still archives to itself.

### Removed

- A second error channel that no screen ever used. `Button`, `Fab`,
  `SegmentedButton`, `DocumentCard` and `EmptyState` each carried `error` and
  `announce_error` props, four `.is-error` rules and a `visually-hidden`
  `ActionErrorStatus` — complete, styled, announced, and dead. Errors go through
  `ErrorBlock`, consistently, on every screen. Two ways to report an error is
  worse than one, and the unused one was the one the next screen would have
  found first. The live `.is-error` states stay: the faulty line row and the
  bottom sheet.

- « Exporter le PDF / PNG » from the record screen, with the export state it
  carried. It wrote two files into app-private storage that no file manager can
  open, and both « Partager » and « Envoyer » already generate whatever is
  missing — its only visible effect was a snackbar the bar was hiding. The
  manual export survives on the preview screen, one tap away via the summary's
  « Aperçu », where producing a file from the document you are looking at is
  coherent. Recovery after a failed export at issue time is unchanged
  (« Réessayer l'export »).

- The preview's « Envoyer » button, disabled since it was written and marked
  « branché dans les tâches 26/27 » — tasks long since done. Sending belongs to
  the record screen; a permanently greyed button taught nothing.

- The bottom sheet's drag handle. In M3 the handle *is* the drag affordance and
  these sheets are not draggable, so it promised a gesture that does not exist.
  Dismissal is unchanged: the scrim, the Back gesture, or the sheet's own
  cancel action.

- Dead stylesheet rules (`.route-list`, `.route-link`, `.field` and its
  children) referenced by no component, and two class names carried in RSX with
  no rule behind them (`catalog-screen`, `outlined-field--multiline`).

### Added

- `tools/check-apk.sh`, run before any install: one native binary, for the
  targeted ABI, referencing the stylesheet the APK actually bundles and matching
  `assets/app.css` on disk. A `dx` build can succeed while shipping something
  else than the working tree, silently — `asset!()` bakes a content hash into
  the binary, dx caches it, and editing only the stylesheet invalidates neither
  that cache nor the crate; since the Gradle assets directory is never purged,
  the stale file is still there to be served. Successive builds also leave their
  `libmain.so` behind, so an APK can carry two ABIs from two different builds.
  A whole day of UI work was tested against an eight-hour-old stylesheet before
  this was noticed. `rm -rf target/dx` prevents both; the script proves it, and
  the release process now requires both.

- Animated splash (DESIGN §8, task 31): the bundled `splash-loop.mp4`
  plays muted and looping under the real logo, which fades and scales in
  over 240 ms on an ease-out-quart curve after a 300 ms hold, then the
  overlay fades out at 2 s — 2.24 s in total, inside the 2.5 s budget.
  The backdrop is the `--color-chrome` token the top app bar already
  uses, which is what shows if the video cannot be decoded; nothing
  waits on the app, since the database opens before the first paint.
  Playback is started from script rather than an `autoplay`
  attribute, so under `prefers-reduced-motion: reduce` the loop is
  stopped on its first frame and the logo is left un-animated. It is
  stopped rather than never started because a `<video>` the WebView has
  no decoded frame for paints its own grey play button over the overlay;
  for the same reason the element stays transparent until `loadeddata`.
  The overlay rules are repeated in the inline pre-render style, since
  the stylesheet is a linked asset and until it arrives the overlay would
  not cover the home screen. The animations are repeated for a second
  reason: their clock starts when the rule reaches the element, while the
  Rust dismissal counts from mount, so a stylesheet arriving late enough
  left the overlay part-way through its fade when the overlay was
  dropped. The logo reuses the PNG `domain::render`
  already embeds instead of shipping a second copy, so the video is the
  only thing the splash adds to the APK (+1.17 MiB).
  `MainActivity` turns off `mediaPlaybackRequiresUserGesture`, without
  which the WebView gates even muted playback behind a tap.

- Dark scheme (DESIGN §2, task 30): the app follows the system setting
  through `prefers-color-scheme`. The eight `dark-*` tokens of the DESIGN
  frontmatter are applied as specified; `surface-dim`, `label`,
  `border-soft`, `primary-tint` and `danger` have no dark counterpart
  there and are derived — `surface-dim` rises *above* `surface` (M3 tonal
  elevation reverses in the dark), `primary-tint` becomes a dark red
  container, and `danger` is lightened because the light one only reaches
  2.6:1 on the dark surface. The fourteen token pairs the components
  paint are checked at WCAG AA in `tests/ui_foundation.rs` (worst pair
  5.56:1), and a companion test pins the only three colours left outside
  the tokens — the white A4, the DESIGN §4 scrim, and the press state on
  the chrome — so no light value can be hardcoded back in. Bottom sheets
  and the overflow menu now rise to their own `elevated` tone: a shadow
  cannot separate two dark greys, and they previously used the same
  token as the cards behind them (1.07:1). Pressed states move to an M3
  state layer, since `brightness(0.88)` shifts a near-black surface by
  about 2 L*. The Android theme is `Theme.AppCompat.DayNight` with a
  `values-night` chrome: the WebView answers `prefers-color-scheme` from
  the theme's `isLightTheme`, so under the scaffolded `.Light` parent the
  dark stylesheet would never have matched on a device. The pre-render
  style, the theme's `windowBackground` and the activity window all carry
  both chromes so the light chrome no longer flashes at startup on a dark
  phone, and `uiMode` is declared in `configChanges` so switching themes
  repaints the app without recreating the activity. The A4 preview stays
  white in both schemes — it is paper.

- Android Auto Backup (ARCHI §6, task 29): `allowBackup` is now declared
  explicitly, with rules for both Android generations —
  `res/xml/backup_rules.xml` (API 24-30) and
  `res/xml/data_extraction_rules.xml` (API 31+, cloud backup and
  device-to-device transfer). Both back up the SQLite database and
  `exports/` from `getFilesDir()`, and nothing else: declaring an
  `<include>` makes the rest opt-out, so caches and the ART profile
  marker stay out. `paths::backup_rules_cover_stored_data` fails the
  build if the names on disk and the names in the rules drift apart.
  Verified on a Pixel 6 Pro (Android 16): forced backup, uninstall,
  reinstall — database restored byte-identical (documents, catalogue,
  numbering counters, settings) along with all exported files.

- Adaptive launcher icon (DESIGN §8, task 28): the VS monogram is cut
  out of `templates/logo.png` — colour mask plus connected components,
  pastries dropped — and painted Crème Vitrine over a flat Rouge
  Enseigne background. Adaptive layers plus square and round fallbacks
  at every density (mdpi→xxxhdpi), and the foreground doubles as the
  Android 13+ monochrome layer. `tools/gen-launcher-icon.py` regenerates
  the whole set from the logo; `build.rs` copies `android/res/` into the
  Gradle project, which `dx` scaffolds without it.

- Compose screen (DESIGN §5, ARCHI §4 « Envoi email », task 27): the
  fiche's « Envoyer par email » now opens `/composition/:id` — recipient
  pre-filled from the client (editable, blocked with a French message
  while empty or implausible via `plausible_email`), subject and branded
  HTML body pre-filled from `render_email` (the « valable jusqu'au »
  sentence reuses `render::validity_end_date`, now `pub(crate)`, so the
  email never contradicts the document), all freely retouchable and
  never persisted. The PDF ○ PNG radio (PDF default, real file names)
  drives the attachment: `export_document` regenerates a missing file
  on the fly, then a worker thread sends via `BrevoMailer` — the
  database lock never wraps the network call (credentials read under a
  short lock, `mark_sent` under a fresh one). One call, one verdict:
  success marks the document sent (`mark_sent` keeps the FIRST
  `sent_at` on resends), publishes « Email envoyé » through the new
  app-level `SendNotice` context and navigates back — the fiche reloads
  from the database, so the « envoyé » badge is already there and shows
  the snackbar (auto-dismissed, cleared on unmount); failure stays as a
  persistent French `ErrorBlock`, retry possible. Double-tap is guarded
  by the send phase (button loading). New `OutlinedTextArea` component
  (multiline counterpart of `OutlinedField`) for the body. The
  `#[expect(dead_code)]` on `mod domain` is gone — this screen wired the
  last domain API without a UI caller. Validated on a physical phone
  (real Brevo send, « envoyé » badge on fiche and home, `sent_at` kept
  on resend) and on emulator with radios cut (persistent French network
  error, retry reaching the API).

- Brevo email client and branded email template (ADR 0002, ARCHI §4,
  task 26): `platform::mail` posts to `/v3/smtp/email` via reqwest
  blocking + rustls (webpki roots — no openssl, no device trust store)
  with a 30 s timeout, redirects disabled so the `api-key` header can
  never be re-POSTed elsewhere, the sender copied in BCC (off-device
  archive) and the document attached as base64 (client-side refusal past
  2.9 MB, under Brevo's 4 MB cap after base64 inflation). Failures reach
  the gérante as typed French messages — invalid key (401), network,
  API refusal with Brevo's message, oversized attachment — never a raw
  HTTP code. `domain::email` owns the network-free side: the `Mailer`
  trait (Brevo impl + mock), the FR subject/body per document kind built
  from `templates/email.html` (Vitrine palette, inline-styled table
  layout, remotely loaded logo — CID unsupported by Brevo, data URIs
  blocked by Gmail) with HTML-escaped user text substituted last so a
  client named « {total} » stays literal, and `interpret_response` as a
  pure, host-tested mapping. `domain::settings` (extracted from `db.rs`,
  which drops back near 1.3k lines) exposes `load_email_credentials` —
  the single path where the raw key leaves storage, straight into the
  redacted-`Debug` `MailConfig`. New deps: reqwest 0.12 (pinned, ring
  backend for painless Android cross-compile) and base64; deny.toml
  allows CDLA-Permissive-2.0 (webpki-roots Mozilla data); `db.rs` sheds
  the settings block and drops to ~1.35k lines.

- Email settings (ARCHI §3 `settings`, ADR 0002, DESIGN §5 « Réglages »):
  the Réglages screen collects the Brevo API key (password field, never
  re-displayed once stored — the row reads « configurée » and « Modifier »
  reveals an empty field whose submission replaces it), the sender address
  and the optional sender name, with minimal validation (plausible email,
  non-empty key, no network call). Values live in the `settings` table
  through a transactional write; the key value never leaves `domain::db`
  outside the future send path (the loaded struct only carries
  `has_api_key`) and never reaches the logs. Until configured, a
  non-blocking prompt on the home screen offers « Configurer » / « Plus
  tard » (dismissal persisted) and the fiche's « Envoyer par email » stays
  disabled with an explanation linking to Réglages — export and share keep
  working; once configured, the button opens the compose screen (tasks
  26/27).
- Document duplication (CONTEXT.md « dupliquer », ARCHI §4): « Dupliquer »
  on the fiche of any issued document — quote or invoice — writes a deep
  copy into the single draft slot (client, lines and payment terms carried
  over, both dates reset to the duplication day, no number and no
  `source_quote_id`: the copy keeps no link with the original and marks
  nothing « facturé ») and opens the form on it, so the copy goes through
  the normal issue flow and gets a fresh number while the original stays
  frozen. A draft with real content is confirmed away first (« Remplacer le
  brouillon ? » — any entered date, client detail, payment terms or line
  counts as content, per the new `DocumentInput::is_blank`); a blank one is
  replaced silently. The draft-slot write is now shared with the conversion
  (`persist_prefilled_draft`).
- Quote → invoice conversion (CONTEXT.md « Conversion », ARCHI §4):
  « Convertir en facture » on the fiche of an unconverted issued quote writes
  a pre-filled invoice draft — deep copy of the client and lines, event date
  and payment terms carried over, issue date set to today, everything
  editable — and opens the form on it; an existing draft is confirmed away
  first (« Remplacer le brouillon ? », same guard as the home's new-document
  flow). `DocumentInput` carries `source_quote_id` through the draft slot and
  the autosave up to the emission (drafts persisted before the field still
  load), the emission then marks the quote « facturé » (derived status — the
  quote is never written) and the action disappears. The domain refuses a
  second conversion with « Ce devis a déjà été converti en facture. » inside
  the emission transaction, before any number is reserved, and a partial
  unique index on `source_quote_id` carries the same invariant in the schema
  alongside the existing foreign key, CHECK and trigger.
- Document sharing (ARCHI §4 « Partage »): « Partager » on the fiche and the
  aperçu opens a bottom sheet offering the real file names (« devis-12.pdf »
  / « devis-12.png »), then a worker re-exports any missing file
  (`export_document` keeps existing ones) and hands its `content://` URI to
  the Android share sheet (`ACTION_SEND` + read grant, fire-and-forget — no
  per-channel logic, Android routes). PNG shares now advertise `image/png`
  both in the intent and in the provider's `getType`, so targets like
  WhatsApp render them inline instead of as raw files. A share failure
  surfaces as a persistent French error block (DESIGN §6); dismissing the
  sheet, the chooser or the target app leaves no state behind.
- Manual export on the fiche: « Exporter le PDF / PNG » regenerates the
  missing files on a worker thread (shared `ExportJobState` + `start_export`
  next to the issue-flow plumbing) and confirms with a snackbar. The
  aperçu's export now reports failures through the same persistent block
  instead of a transient snackbar (DESIGN §6).
- Issued document record (fiche, DESIGN §5): a summary card (kind + number,
  client, dates, payment terms, total, « envoyé »/« facturé » badges) with
  read-only collapsible lines, and the action stack in a sticky bottom-third
  chrome bar (Règle du Pouce). « Aperçu » opens the full-screen preview;
  send and duplicate render as disabled placeholders until tasks 24–27 wire
  them. « Convertir en facture » only appears on an
  unconverted quote (derived `is_invoiced`, task 08), and a converted invoice
  discreetly references its source quote number. An issued document stays
  frozen — no edit entry point anywhere. The post-emission snackbar and the
  « Réessayer l'export » retry move here from the app-shell placeholder.
- End-to-end issue flow behind « Émettre » (draft form and draft preview):
  the full chain — structured validation → transactional emission (number +
  insert) → draft clear → decoupled PDF/PNG export — runs on a worker thread
  under the button's loading state, and the phase machine itself makes a
  double-tap a no-op. Validation errors stay on screen as a persistent
  aggregated block with the faulty fields and line rows flagged (cleared on
  the next edit, never on a timer); a failed export leaves the document
  issued — the number is never rolled back after commit (ARCHI §4) — and the
  fiche shows « PDF non généré » with a working « Réessayer l'export ».
  Success replaces the draft screen with the fiche and a transient
  « Devis n° 10 émis » snackbar.
- `validate_document_fields` attributes each validation error to its field
  (`DocumentField`, zero-based line indices) so the form can flag inputs;
  `validate_document` keeps its flat-messages contract on top of it.
- `DocumentKind::label()` centralizes the French display name
  (« Devis »/« Facture ») and `DocumentExport::files_label()` the export
  result snackbar text; `ErrorBlock` gains an aggregated list variant for
  validation errors.

- PNG export pipeline for issued documents: « Exporter » on the preview writes
  `exports/{devis|facture}-N.pdf` (Typst) and a single stacked PNG
  (`PdfRenderer` at ~150 dpi per the task spec, pages stacked vertically with
  the desktop's white background and gray separators). Existing files are
  kept (frozen documents); missing ones are regenerated, so the action is
  also the re-export path. Exports are serialized, atomic (tmp + rename with
  cleanup), panic-contained, and surface French errors without crashing.
- Typst document template now handles invoices as well as quotes (FACTURE
  header, payment terms, règlement block with IBAN, late-penalty mention for
  professional clients, closing line) — a faithful port of the `render.rs`
  invoice variant.
- Debug-only reference PDF export trigger in the overflow menu (debug builds
  only): runs the Typst reference export on a worker thread with panic
  containment, announces the result or a French error via a polite status
  line. Used by the task 05 fidelity verification on Android 35.
- Share sheet right after the debug reference export: `platform::share`
  (Intent `ACTION_SEND` over a `content://` URI) backed by
  `ExportFileProvider`, a read-only Kotlin provider on `exports/` declared in
  the manifest (the Dioxus build accepts neither extra resources nor extra
  Kotlin sources, ruling out the androidx FileProvider XML).
- Full-screen document preview (« Aperçu »): the draft (next number peeked
  read-only, never reserved, discreet « aperçu » pill) and any issued
  document rendered exactly in an A4 iframe `srcdoc` on the neutral
  background, with pinch-zoom, pan and double-tap fit-to-width gestures,
  and a contextual chrome action bar (Export and Share live since tasks
  19/22, Send staged disabled for tasks 26/27).
- Shared `issue_label` helper and `.chrome-action-bar` style now backing
  both the form and the preview action bars.

- Catalog management screen: every item grouped by group, add/edit in a
  bottom sheet (name, euro price, unit, group, active toggle), deactivation
  instead of deletion so issued documents keep their copied lines.
- Catalog picker bottom sheet in the draft form: active items as two-column
  chips (name + price) grouped by group, one tap adds a pre-filled line
  (quantity 1), free-form entry stays available via the line sheet.
- Catalog persistence queries (desktop pattern): full list, active-only
  list, and insert/update upsert with regression tests.

- Draft form line editor: summarized rows with group, detail and subtotal,
  add/edit/delete/reorder in a bottom sheet (two-tap inline delete
  confirmation, no dialog), integer-only euro price parsing in the domain,
  and a sticky teal-tint total pill above the chrome action bar.

- Draft form screen with stacked client, dates, and payment-terms sections,
  kind-aware labels, debounced auto-save to the draft store, and a sticky
  chrome action bar (Preview wired, Issue pending the task 20 flow).
- Home screen with filtered recent documents, issued-status badges, draft
  resumption, and confirmed draft replacement.
- Reusable Material 3 buttons, FAB menu, outlined fields, document cards,
  status badges, segmented controls, bottom sheet, snackbar, error and empty states.
- Light mobile UI foundation with design tokens, teal Material app chrome,
  typed stack navigation and system back, and edge-to-edge Android insets.
- Singleton JSON draft persistence with restore, replacement, corruption tolerance,
  and explicit clearing.
- Transactional document issuance with validation before number reservation,
  atomic persistence, and post-commit export decoupling.
- Issued-document persistence queries for transactional inserts, filtered recent
  lists with derived statuses, detail loading, first-send tracking, and client
  autocomplete.
- Transactional quote and invoice number reservations with rollback before commit.
- Complete idempotent SQLite schema, shared connection, app-private database
  path, and desktop-compatible counters and catalog seeds.
- Experimental offline Typst PDF export with embedded fonts and Android-private
  output for the task 05 fidelity spike.
- Desktop-faithful quote and invoice HTML rendering with embedded template and
  logo, escaped user content, grouped lines, totals, and print pagination.
- Desktop domain models, euro formatting, and validation migrated with issued
  `Document` state and regression tests.
- CI workflow with the 5 blocking gates from ARCHI §8 (fmt, clippy, tests,
  domain coverage ≥ 85 %, audit + deny) on the Linux host, plus `deny.toml`
  (licenses, advisories, duplicate versions, sources).
- Scaffold `devis-mobile` Dioxus crate: `ui → domain ← platform` module layout,
  Android config (`fr.variete_saveurs.devis_factures`), document templates
  copied from the desktop app, dependency-rule regression test.

- Client autocomplete in the draft form name field: from two typed
  characters, up to five clients suggested from the issued-document history,
  one tap pre-fills every client field (kind, address, email, phone, SIRET,
  billing address) and stays editable afterwards; the inline list dismisses
  on outside taps and scroll gestures and never overlays the keyboard.

### Fixed

- System bar insets never reached the WebView: the insets listener set on the
  WebView was never dispatched, so the top app bar slid under the status bar
  on physical devices. Insets are now cached from the decor view listener and
  replayed into the WebView a few times after attach (evaluateJavascript is a
  no-op until a page loads), keeping the chrome below the status bar.

### Changed

- Architecture: Typst adopted as the PDF engine (ADR 0003); `ARCHI.md §5`
  and the export stack note now describe the
  `DocumentInput → Typst → PDF privé → PdfRenderer → PNG → partage/email`
  pipeline instead of the impossible WebView print design.
