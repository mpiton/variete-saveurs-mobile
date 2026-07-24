# Typst embarqué pour l'export PDF

Le pipeline prévu (`ARCHI.md §5` initial : WebView offscreen + `createPrintDocumentAdapter` → PDF fichier dans le stockage privé) n'existe pas dans le SDK Android public : l'impression WebView passe obligatoirement par le dialogue système `PrintManager`, sans descripteur de fichier privé, et piloter `PrintDocumentAdapter.onLayout()/onWrite()` exige des callbacks hors SDK (constructeurs `@hide`, API non-SDK bloquable depuis Android 9). Le spike (tâche 05) a documenté ce blocage avec sources primaires, puis évalué le plan B : le moteur **Typst** compilé en Rust dans l'app.

Le plan B est vérifié et adopté :

- PDF A4 multi-pages généré hors ligne dans `exports/`, sans réseau ni dialogue système, sur AVD Android 35 (~700 ms) et téléphone physique Pixel 6 Pro (~1,1 s) ;
- fidélité visuelle confirmée par comparaison page par page avec le PDF Chromium desktop du même document (logo, couleurs, filets or, groupes, sauts de page, signatures, folios) ; écarts résiduels cosmétiques listés dans le verdict de la tâche 05 (polices Liberation embarquées vs Georgia/système, pastille « Total du devis », rythme vertical) ;
- rendu déterministe : même octet à l'écran entre x86_64 (AVD) et arm64 (téléphone) ; polices Liberation Serif/Sans embarquées ;
- surcoût APK mesuré : +24,3 Mio (sous le budget de 25 Mio) ;
- échec d'export = erreur française propre, pas de crash, staging nettoyé.

Le pipeline devient : `DocumentInput → Typst → PDF privé → PdfRenderer → PNG → partage/email`. Le renderer HTML (`domain/render.rs`) est conservé pour l'aperçu in-app (`srcdoc`) et comme référence de comparaison ; le template Typst (`templates/document.typ`) devient la source du PDF livré. Les tâches 19/20 sont adaptées à ce pipeline.

## Considered Options

- **Impression WebView silencieuse** (design initial) — impossible en SDK public, démontré par le spike.
- **Dialogue `PrintManager`** — supporté, mais ne fournit pas le fichier privé requis par les exports PNG, le partage et l'email automatiques.
- **`android.graphics.pdf.PdfDocument`** — supporté, mais impose de réécrire manuellement texte, tableaux, pagination et typographie en Kotlin/Canvas.
- **Callbacks WebView non-SDK (réflexion/JNI)** — rejetés pour stabilité et compatibilité (restrictions API non-SDK).
- **Service de conversion distant** — rejeté : l'app doit rester 100 % locale.
