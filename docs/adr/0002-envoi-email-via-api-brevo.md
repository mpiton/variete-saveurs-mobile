# Envoi des emails via l'API Brevo

L'envoi d'un document par email se fait depuis l'app via l'API Brevo (domaine propre vérifié SPF/DKIM, adresse expéditrice professionnelle), et non en remettant un brouillon pré-rempli à l'app mail du téléphone. Motivation : écran de composition entièrement dans l'app (template HTML brandé, choix PDF/PNG en pièce jointe) et envoi en un geste, sans dépendre de la configuration de Gmail sur le téléphone.

## Consequences

- Une clé API Brevo et la vérification DNS du domaine deviennent des prérequis au premier lancement ; la clé est saisie dans l'app (jamais dans le code).
- Les emails n'apparaissent pas dans les « Messages envoyés » de la boîte pro : compensé par un BCC automatique vers l'adresse pro et un statut « envoyé » sur le document.
- Volume très en dessous du tier gratuit (300/jour) ; pas de coût attendu.

## Considered Options

- **Remise à l'app mail (intent Android)** — zéro identifiant à gérer, email dans les « envoyés » ; écarté car la composition sort de l'app et dépend de la boîte configurée dans Gmail.
- **SMTP direct** — dépendant du fournisseur de la boîte, identifiants lourds à gérer ; écarté.
