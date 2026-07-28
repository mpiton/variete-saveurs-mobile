---
name: variete-saveurs-mobile
description: Conventions et pièges vérifiés du dépôt variete-saveurs-mobile — app Android Rust/Dioxus de devis et factures. Où vivent les tests, comment vérifier une règle de rendu qu'aucun test Rust ne peut mesurer, les trois rendus du document, et les faits que la documentation locale ne couvre pas. À charger avant de modifier ce dépôt.
---

# variete-saveurs-mobile

App Android mono-utilisatrice : rédiger des devis, les convertir en factures,
exporter en PDF/PNG, remettre au client. Rust et Dioxus dans une WebView,
SQLite en local, pas de backend.

## Lire d'abord — et savoir que ce n'est pas dans le dépôt

`CLAUDE.md`, `ARCHI.md`, `DESIGN.md`, `PRODUCT.md` et `CONTEXT.md` font
autorité : vocabulaire du domaine, architecture, système visuel, vérité
produit. **Aucun n'est suivi par git.** Ils sont dans `.git/info/exclude`, donc
un clone frais ne contient que `README.md`, `CONTRIBUTING.md` et ce fichier.
S'ils manquent, les demander avant de décider quoi que ce soit — ce fichier ne
les remplace pas.

## Conventions réelles

- **Fichiers et modules en `snake_case`** : `catalog_picker.rs`,
  `pdf_renderer.rs`, `line_sheet.rs`. Zéro fichier en camelCase sur 45, et
  `clippy -D warnings` refuserait le reste.
- **Tests** : modules `#[cfg(test)] mod tests` à la fin du fichier qu'ils
  couvrent, plus les tests d'intégration de `tests/*.rs` (`ui_foundation.rs`,
  `splash.rs`, `launcher_icon.rs`, `dependency_rule.rs`). Harnais `cargo test`
  intégré. **Aucun fichier `*.test.rs` n'existe** et il ne faut pas en créer.
- **Imports** : `use crate::domain::{...}` d'une couche à l'autre,
  `use super::{...}` à l'intérieur d'une couche.
- **Commits** : Conventional Commits, portées comprises — `feat:`, `fix(ui):`,
  `docs:`, `chore(deps):`, `chore(release):`, `test:`. Jamais d'attribution IA.
- **Règle de dépendance** : `ui → domain ← platform`. `domain` ne connaît ni
  `platform::` ni `dioxus` ; `tests/dependency_rule.rs` le vérifie au lieu de
  s'en remettre à la relecture.

## Les gardes se posent sur la règle, pas sur l'instance

C'est le défaut récurrent de ce dépôt, et il a coûté trois corrections du même
bug. `DESIGN.md §2` exige qu'un contrôle possède un signal — aplat ou bord —
franchissant 3:1 contre la surface qui le porte. La règle a d'abord été
appliquée au seul variant où le défaut s'était montré, puis à la seule barre où
il s'était montré. Il a resurgi deux fois ailleurs, et à chaque fois le test
écrit pour l'attraper passait au vert.

Un test qui énumère les cas connus reproduit l'angle mort. Les gardes d'ici
lisent donc la source et en dérivent leurs cas :

- `every_button_variant_keeps_a_shape_on_every_surface_it_is_posed_on` énumère
  les `.m3-button--*` depuis `app.css`, résout aplat et bord par schéma, et
  mesure contre chaque hôte.
- `the_screen_carries_every_colour_the_document_declares` compare
  `templates/document.css` et `assets/app.css` valeur par valeur.
- `every_validation_error_has_an_anchor_the_form_actually_renders` relie un
  `match` exhaustif sur `DocumentField` aux `id` que le RSX émet réellement.

**Un garde qui n'a jamais échoué ne prouve rien.** Une fois écrit :
réintroduire le défaut, lire le message d'échec, restaurer. C'est ce qui
distingue un test qui mord d'un test qui passe à côté.

## Le document a trois rendus, et ils ne se prédisent pas

- **Aperçu et vignette** : `domain/render.rs` produit du HTML avec
  `templates/document.css`, affiché en `srcdoc`. Bande continue, **sans pages**.
- **PDF livré** : Typst compile `templates/document.typ` depuis
  `platform/export.rs`. C'est la référence.
- **PNG livré** : rendu **depuis le PDF** par `PdfRenderer` en JNI, une image
  par page, puis empilé (`ARCHI §4`). Le commentaire de
  `templates/document.css:340` affirme le contraire ; il date du desktop et il
  est faux.

La hauteur du HTML **ne prédit pas** la pagination du PDF : mesuré sur quatorze
documents, elle se trompe deux fois et dans les deux sens — 12 lignes tiennent
en 1 page HTML et 2 pages PDF, 58 lignes en 3 pages HTML et 2 pages PDF. Aucune
hauteur de page ne réconcilie les deux. Pour un nombre de pages exact, appeler
`export::count_pdf_pages`, qui compile le même Typst que l'export.

`templates/document.css` et `templates/logo.png` sont **identiques à l'octet** à
ceux du desktop gelé `../app/src-tauri/templates/`. Rien ne le garde : vérifier
au `md5sum` avant et après toute intervention à proximité.

## Vérifier une règle de rendu

Aucun test Rust ne mesure une mise en page. Les contrastes se calculent depuis
les tokens, mais la géométrie, le retour à la ligne et l'ordre de cascade ne se
voient qu'au rendu. Le défaut qui laissait l'écran d'aperçu s'arrêter 18 px
au-dessus de la fenêtre — bande de navigation Android peinte en couleur de
contenu — n'était détectable que comme ça.

Le banc, à reconstruire au besoin :

1. Reconstituer les écrans en HTML statique **depuis le RSX courant** — classes
   et structure réellement émises — en liant `assets/app.css` tel quel.
   Vérifier son `md5sum` avant et après : le CSS mesuré doit être celui du
   disque.
2. Piloter Chromium en CDP à 412×915, `--system-inset-top/-bottom: 24px`, dans
   **les deux schémas**.
3. Mesurer le fond réellement peint sous un élément avec `elementsFromPoint` :
   l'ancêtre DOM d'un élément `position: absolute` n'est pas ce qu'il y a
   derrière lui.

Pièges d'outillage constatés ici :

- Chromium 149 : `--headless` seul sort en 144 sans rien écrire. Utiliser
  `--headless=new`, ou le `chrome-headless-shell` de Playwright.
- Une feuille ouverte par l'attribut `open` ne crée ni top layer ni
  `::backdrop` : appeler `showModal()`.
- `Page.captureScreenshot` renvoie du PNG en type couleur 2 — RGB, 3 octets,
  pas RGBA. Un décodeur qui suppose 4 octets rend une image noire.

## Autres faits écrits nulle part ailleurs

- `document::eval` enveloppe chaque script dans `new AsyncFunction(...)`
  (`dioxus-desktop-0.7.9/src/query.rs:79`). Chaque appel a son propre scope :
  un `const` en tête de script est sûr.
- Travail bloquant hors du thread UI : `use_signal_sync` +
  `std::thread::spawn` + `issue::write_from_worker`, qui réessaie l'écriture et
  l'abandonne bruyamment quand le scope de l'écran a disparu.
- Le routeur `Routable` ne laisse pas keyer un composant par ses paramètres, et
  Dioxus conserve l'instance quand seuls les paramètres changent. Un état
  dérivé d'un paramètre doit donc porter son identité — voir `PageCount` dans
  `ui/preview.rs`.
- `rtk` réécrit la sortie de `cargo test` en résumé d'une ligne. Pour lire les
  compteurs par suite : `rtk proxy cargo test`.

## Gates

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo llvm-cov --fail-under-lines 85 \
  --ignore-filename-regex 'src/(ui|platform)/|src/main\.rs|tests/'
cargo audit && cargo deny check
```

`CHANGELOG.md` est mis à jour dans chaque PR qui touche du code, section
`[Unreleased]`. L'APK livré ne sort pas d'une commande `dx` : voir
`CLAUDE.md §Release process`.
