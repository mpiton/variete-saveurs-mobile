# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Debug-only reference PDF export trigger in the overflow menu (debug builds
  only): runs the Typst reference export on a worker thread with panic
  containment, announces the result or a French error via a polite status
  line. Used by the task 05 fidelity verification on Android 35.
- Full-screen document preview (« Aperçu »): the draft (next number peeked
  read-only, never reserved, discreet « aperçu » pill) and any issued
  document rendered exactly in an A4 iframe `srcdoc` on the neutral
  background, with pinch-zoom, pan and double-tap fit-to-width gestures,
  and a contextual chrome action bar (Issue / Export-Share-Send buttons
  staged disabled for tasks 19/20/22/26).
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
