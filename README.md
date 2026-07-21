# Variété de Saveurs — mobile

Application Android de devis et factures pour une TPE artisanale (boulangerie/traiteur). Elle remplace l'app desktop existante : rédaction de devis, conversion en facture, export PDF et partage au client.

## Stack

- Rust + [Dioxus](https://dioxuslabs.com/) (voir [ADR 0001](docs/adr/0001-dioxus-pour-l-app-mobile.md))
- Envoi d'email via l'API Brevo (voir [ADR 0002](docs/adr/0002-envoi-email-via-api-brevo.md))

## Structure

```
docs/adr/    décisions d'architecture
assets/      ressources embarquées
templates/   gabarits de documents (logo, rendu)
```

Le projet est en cours de développement, le code applicatif arrive au fil du sprint en cours.

## Licence

Tous droits réservés.
