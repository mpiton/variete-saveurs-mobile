# Spike Typst pour l'export PDF Android

**Date :** 2026-07-21

**Statut :** approuvé pour expérimentation, pas encore retenu comme architecture

**Portée :** remplacement potentiel du PDF WebView de la tâche 05

## Contexte

L'app doit produire automatiquement, hors ligne et sans dialogue système un PDF
A4 dans son stockage privé. Ce même fichier alimente ensuite l'aperçu, l'export
PNG, le partage et l'envoi email.

Le pipeline prévu dans `ARCHI.md §5` n'est pas réalisable avec le SDK Android
public. `WebView.createPrintDocumentAdapter()` est destiné à
`PrintManager.print()`, qui ouvre l'interface système. Les callbacks nécessaires
pour piloter directement `onLayout()` et `onWrite()` ont des constructeurs hors
SDK ; ils ne seront pas utilisés par réflexion ou JNI.

## Décision du spike

Évaluer Typst comme moteur de mise en page et de génération PDF embarqué en
Rust. Le spike doit prouver cette option sur Android avant toute migration du
renderer HTML ou modification des documents normatifs.

Typst est retenu comme premier candidat parce qu'il :

- produit directement des PDF paginés sans service réseau ni dialogue Android ;
- gère pages A4, tableaux, images, en-têtes, pieds de page et compteurs ;
- s'embarque comme bibliothèque Rust sous licence Apache-2.0 ;
- permet d'utiliser des polices embarquées pour un rendu déterministe.

Sa compatibilité effective avec les cibles Android du projet, son poids et sa
vitesse restent à prouver. Le spike n'anticipe pas son succès.

## Options écartées pour ce spike

1. **Dialogue `PrintManager`** : supporté, mais ne fournit pas le chemin du PDF
   privé et casse les flux PNG, partage et email automatiques.
2. **`android.graphics.pdf.PdfDocument`** : supporté, mais impose de réécrire
   manuellement texte, tableaux, pagination et typographie dans Kotlin/Canvas.
3. **Callbacks WebView non-SDK** : rejetés pour stabilité et compatibilité.
4. **Service de conversion distant** : rejeté ; l'app doit rester locale.

## Architecture expérimentale

Le même `DocumentInput` de référence alimente deux sorties :

```text
DocumentInput
├── renderer HTML existant ──→ reference.html ──→ Chromium desktop ──→ reference.pdf
└── template Typst embarqué ─→ compilateur Typst Android ─────────────→ candidate.pdf
```

Le spike prépare `reference.html` et `candidate.pdf` dans un répertoire de
génération temporaire, puis publie la paire par renommage du répertoire vers
`exports/reference-<génération>/`. Une erreur supprime la génération temporaire,
donc un nouveau HTML ne peut pas être associé à un ancien PDF. Les deux fichiers
sont extraits avec `adb` ; Chromium produit ensuite le PDF desktop depuis le HTML
exact généré par le mobile.

Le document de référence contient :

- plusieurs groupes et assez de lignes pour forcer plusieurs pages ;
- le logo embarqué ;
- coordonnées client complètes ;
- total, conditions et signatures ;
- au moins un groupe proche d'un saut de page.

Les PDF desktop existants utilisent Liberation Serif et Liberation Sans. Le
spike embarque ces mêmes familles, avec leurs licences, afin d'éviter tout écart
causé par les polices présentes sur le téléphone.

## Surface du prototype

L'écran Dioxus actuellement vide reçoit un unique bouton de debug
« Générer le PDF de référence ». Il affiche le chemin produit ou une erreur en
français. Aucun écran d'export définitif ni abstraction prévue pour les tâches
futures n'est ajouté.

Le prototype :

- compile un template et ses données entièrement embarqués, sans import Typst
  distant ;
- effectue le travail hors du thread UI ;
- publie les deux artefacts ensemble après leur écriture complète ;
- renvoie un `Result`, sans `unwrap()` ou `expect()` sur le chemin utilisateur ;
- conserve les détails techniques en anglais pour les logs et présente un
  message français à l'utilisatrice.

## Vérifications

### Automatiques

- test RED puis GREEN pour toute transformation pure ajoutée au domaine ;
- smoke test hôte compilant le template avec les données de référence ;
- vérification que la sortie commence par `%PDF` et contient plusieurs pages ;
- `cargo fmt --check` ;
- `cargo clippy --all-targets -- -D warnings` ;
- `cargo test --locked` ;
- couverture domaine ≥ 85 % si `src/domain/` change ;
- compilation Android `x86_64-linux-android` et `aarch64-linux-android` ;
- `dx build --platform android`.

### Android et visuelles

- génération réussie sur l'AVD Android 35 ;
- génération réussie sur le téléphone cible avant verdict final ;
- `pdfinfo` confirme A4 et plusieurs pages ;
- comparaison côte à côte, page par page, aux mêmes dimensions ;
- vérification explicite du logo, des polices, couleurs, filets or, tableaux,
  groupes, sauts de page, signatures et folios `Page X / Y` ;
- mesure du temps de génération et de l'augmentation de taille de l'APK.

Un temps supérieur à 5 secondes sur le téléphone ou une augmentation d'APK
supérieure à 25 Mio déclenche une décision explicite ; ce n'est pas masqué comme
un succès du spike.

Mesure après suppression des symboles release, appliquée aux deux builds :
baseline `12 871 861` octets, candidat `38 347 713` octets, soit un delta de
`25 475 852` octets (`24,296 Mio`) et `0,704 Mio` de marge sous le seuil.

## Critères de décision

Le candidat Typst est accepté seulement si :

- les deux architectures Android compilent sans dépendance native manquante ;
- le PDF est généré automatiquement dans `exports/`, sans réseau ni UI système ;
- aucune page ne contient de texte coupé, chevauchement, groupe orphelin ou
  élément manquant ;
- les différences visuelles restantes sont listées et acceptées explicitement ;
- l'échec de compilation ou d'écriture produit une erreur française sans crash ;
- les mesures téléphone et APK sont consignées.

Si le spike passe, une décision séparée :

1. ajoute l'ADR 0003 ;
2. met à jour `ARCHI.md §5` ;
3. remplace progressivement le renderer HTML, sans suppression anticipée ;
4. adapte les tâches 04, 18, 19 et 20 au pipeline :

```text
DocumentInput → Typst → PDF privé → PdfRenderer → aperçu/PNG → partage/email
```

Si le spike échoue, la tâche 05 reste bloquée et le choix produit entre dialogue
d'impression et renderer PDF manuel est rouvert. Aucun contournement non-SDK
n'est réintroduit.

## Hors périmètre

- implémentation complète des écrans d'aperçu, export ou partage ;
- suppression du renderer HTML existant ;
- export PNG ;
- modification immédiate d'`ARCHI.md`, `DESIGN.md` ou `CONTEXT.md` ;
- moteur configurable ou prise en charge de plusieurs templates.

## Sources

- [Typst — dépôt officiel](https://github.com/typst/typst)
- [Typst — export PDF](https://typst.app/docs/reference/pdf/)
- [Typst — mise en page](https://typst.app/docs/reference/layout/page/)
- [Android — impression HTML](https://developer.android.com/training/printing/html-docs)
- [Android — restrictions sur les API non-SDK](https://developer.android.com/guide/app-compatibility/restrictions-non-sdk-interfaces)
