# Product

<!-- impeccable:product-schema 1 -->

## Platform

android

## Users

Une seule utilisatrice : la gérante de **Variété de Saveurs**, boulangerie/traiteur artisanale en micro-entreprise. Elle rédige elle-même ses devis et ses factures, sur son téléphone Android personnel. Pas de compte, pas de collègue, pas de second appareil, pas de comptable dans la boucle de l'app.

Le développeur n'est pas l'utilisatrice : les faits d'usage ci-dessous sont ceux qu'il a confirmés pour elle. Ce qui n'a pas été établi est marqué comme tel, pas comblé.

## Product Purpose

Produire un devis ou une facture brandé, prêt à remettre, sans allumer d'ordinateur : saisie → **émission** (numéro définitif, document figé) → export PDF/PNG → remise au client.

Succès : elle traite sa paperasse le soir depuis son téléphone, en quelques minutes par document ; le client reçoit un document qui a l'air d'une maison installée ; l'ordinateur n'est jamais nécessaire.

## Positioning

Ce qu'un outil de facturation voisin ne peut pas revendiquer honnêtement :

- **Local d'abord.** SQLite dans le stockage privé de l'app, aucun backend, aucun compte, aucun abonnement, aucune synchronisation, aucune télémétrie. **Une seule exception, explicite** : quand elle choisit d'envoyer un document par email, le PDF ou le PNG part chez Brevo, avec une copie en BCC à son adresse professionnelle (`ARCHI.md §4`) — c'est le geste d'envoi qui fait sortir la donnée, jamais l'app d'elle-même. Rien d'autre ne quitte le téléphone : le partage Android reste local à l'appareil, et sa base n'est jamais transmise.
- **Le document n'est pas un template paramétré.** C'est le gabarit exact de sa marque (`templates/document.css` + `templates/logo.png`), repris à l'identique de l'app desktop, pas un thème choisi dans une liste.
- **Continuité comptable.** L'app reprend la numérotation là où le desktop s'est arrêté (devis n° 10, facture n° 1) : pas de remise à zéro, pas de rupture dans les archives.
- **Micro-entreprise assumée.** Pas de TVA (art. 293 B du CGI), mention portée par le document — un défaut du produit, pas une case à décocher.

## Operating Context

**Quand.** Le soir, au calme. Session admin posée, souvent plusieurs documents à la suite. Le confort de saisie et la reprise sans perte priment sur les raccourcis d'urgence. *(confirmé — corrige l'hypothèse « debout au comptoir, en pleine lumière » écrite dans `DESIGN.md §1`)*

**Pourquoi le téléphone.** Pour ne plus dépendre de l'ordinateur — loin, partagé ou lent à démarrer. Le critère est la **disponibilité immédiate**, pas la vitesse de frappe : ouvrir l'app doit suffire (pas de login, pas de sync, pas d'attente réseau sur le chemin de saisie). *(confirmé)*

**Remise au client.** Les trois chemins coexistent selon le client, à parité — aucun n'est un cas dégradé d'un autre : *(confirmé)*

1. **Envoi** email depuis l'adresse professionnelle (Brevo), pièce jointe PDF ou PNG, copie à soi en BCC (qui fait archive hors-appareil), statut « envoyé » posé.
2. **Partage** via le share sheet Android — WhatsApp, SMS, mail, Facebook, n'importe quelle app.
3. **Impression papier**, remise en main propre.

**Rituel comptable.** Un document émis est figé : correction = duplication → nouveau numéro. Les statuts se limitent à « facturé » (devis converti) et « envoyé ».

**Non établi.** Le modèle et la taille d'écran du téléphone. La manière dont l'impression se fait concrètement (service d'impression Android depuis le partage, ou PDF repris sur un autre appareil).

## Capabilities and Constraints

Vocabulaire normatif : `CONTEXT.md`. Architecture : `ARCHI.md`. Décisions : `docs/adr/`.

**Confirmé :** brouillon unique auto-sauvegardé ; émission (numérotation + gel) ; conversion devis → facture 1:1 ; duplication ; catalogue d'items réutilisables et éditable ; autocomplétion client depuis l'historique (pas d'entité client) ; export PDF (Typst embarqué, ADR 0003) et PNG ; partage par Intent Android ; envoi email via l'API Brevo (ADR 0002) ; sauvegarde par Android Auto Backup.

**Contraintes durables :** Android uniquement, APK installé directement (pas de Play Store) ; français uniquement, écran et documents ; montants en centimes `i64` ; clé Brevo saisie au premier lancement et stockée en privé, jamais dans le repo ni les logs ; le PDF exporté est la référence de vérité, pas l'écran.

**Hors périmètre v1 (décidé, pas ouvert) :** backend, comptes, sync, multi-appareil, iOS, Play Store, acomptes et factures partielles, statuts accepté/refusé/payé, carnet de clients, éditeur du template email, file d'attente d'envoi hors-ligne, i18n, TVA.

## Brand Commitments

- Nom : **Variété de Saveurs**. Logo officiel `templates/logo.png` — jamais regénéré.
- Le gabarit du document A4 porte la marque : rouge #C0182B, filets or, crème, Georgia, format A4 imprimable. Copie conforme du desktop ; toute modification se juge sur le PDF exporté.
- Voix : française, artisanale, sobre, fiable. Une maison qui prend sa paperasse au sérieux.
- Anti-références héritées et toujours valables : la sortie de SaaS de facturation générique (gris, Excel-ish, sans identité) ; la surcharge kitsch artisanale (scriptes, rustique encombré, couleurs qui se battent).

## Evidence on Hand

- **Documents réels exportés** (rendu de référence, pas des maquettes) : `../Devis n° 8 — Variété de Saveurs.pdf`, `../Devis n° 9 — Variété de Saveurs.pdf`, `../devis-10-variete-de-saveurs.html`, `../facture-variete-de-saveurs.pdf` et `.png`.
- **Gabarit et marque** : `templates/document.css`, `templates/logo.png`, `templates/email.html`.
- **App desktop gelée** `../app/` : logique métier et spécification produit de référence (`../app/PRODUCT.md`, `../app/DESIGN.md`).
- **Splash animée livrée** : `assets/splash-loop.mp4`.

**Absences à ne jamais fabriquer :** aucun témoignage, aucun chiffre d'usage, aucune donnée de vente, aucun benchmark, aucune télémétrie. Une utilisatrice, zéro mesure.

## Product Principles

1. **Disponible d'abord.** La raison d'être du mobile est de ne plus dépendre d'un ordinateur : ouvrir l'app doit suffire. Rien sur le chemin de saisie ne peut exiger un réseau, un login ou une attente.
2. **La session du soir, pas le sprint du comptoir.** L'usage réel est calme et enchaîne les documents : confort de saisie, reprise à l'identique après interruption, lisibilité durable.
3. **Le document est le produit.** Toute décision se juge sur le PDF imprimé dans les mains de la cliente, jamais sur l'écran.
4. **Trois remises à parité.** Email, partage et impression sont trois chemins de premier rang ; l'app n'en privilégie aucun par sa forme.
5. **Émis = figé.** L'immuabilité n'est pas une contrainte technique à contourner, c'est une promesse comptable : correction = duplication, nouveau numéro.

## Accessibility & Inclusion

Pas d'audit WCAG formel exigé, mais un socle non négociable : contrastes lisibles dans les deux schémas, cibles ≥ 48dp, respect de la taille de police système (texte en sp/rem, jamais px), respect de « réduire les animations », back prédictif jamais piégé, insets edge-to-edge appliqués.

Documents lisibles en niveaux de gris à l'impression.

Aucun besoin d'accessibilité personnel n'a été établi pour la gérante — ne pas en inventer.
