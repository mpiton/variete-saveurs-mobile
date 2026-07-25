# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
