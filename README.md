# Variété de Saveurs — mobile

Application Android de devis et factures pour une TPE artisanale (boulangerie/traiteur). Elle remplace l'app desktop existante : rédaction de devis, conversion en facture, export PDF et partage au client.

## Stack

- Rust + [Dioxus](https://dioxuslabs.com/) (voir [ADR 0001](docs/adr/0001-dioxus-pour-l-app-mobile.md))
- Envoi d'email via l'API Brevo (voir [ADR 0002](docs/adr/0002-envoi-email-via-api-brevo.md))
- Export PDF par [Typst](https://typst.app/) embarqué (voir [ADR 0003](docs/adr/0003-typst-embarque-pour-l-export-pdf.md))
- Mise à jour de l'APK depuis l'app (voir [ADR 0004](docs/adr/0004-mise-a-jour-de-l-app-depuis-l-app.md))

## Structure

```
src/domain/    logique métier, testée sur l'hôte
src/ui/        écrans Dioxus (RSX)
src/platform/  ponts JNI Android (fichiers, partage, PDF, mise à jour)
tests/         tests d'intégration hôte
docs/adr/      décisions d'architecture
android/       manifeste et MainActivity Kotlin
assets/        ressources embarquées
templates/     gabarits de documents (logo, rendu)
tools/         scripts de build (icône, vérification d'APK)
```

## Build

Commandes de dev, gates de qualité et règles de contribution :
[CONTRIBUTING.md](.github/CONTRIBUTING.md).

```bash
dx serve --platform android    # dev, device ou émulateur branché
```

L'APK livré est construit localement, où vit le keystore de signature — la CI ne
fait tourner que les gates sur hôte Linux. Il ne sort pas d'une commande `dx` :
celle-ci assemble la variante Gradle debug, `debuggable="true"` et signée par la
clé de debug. La séquence de release est `gradlew assembleRelease`, puis
`zipalign` et `apksigner` avec le keystore gardé hors du repo.

## Licence

Tous droits réservés.
