# Mise à jour de l'app depuis l'app

L'app se livre en APK direct, jamais par le Play Store (CLAUDE.md §Périmètre). Sans mécanisme interne, un correctif n'atteint le téléphone de la gérante que si quelqu'un se déplace avec un câble. Un bouton dans Réglages vérifie s'il existe une version plus récente, la télécharge et la donne à l'installeur système.

## Ce que la mise à jour ne doit jamais faire

La base SQLite et `exports/` vivent dans le stockage privé de l'app. Une mise à jour normale — même signature, `versionCode` non inférieur — les conserve. **Tout ce qui ressemble à une désinstallation les détruit** : c'est la comptabilité de la gérante. Rien sur ce parcours ne propose « désinstallez puis réinstallez », et l'écran porte la phrase en clair, au-dessus du bouton, dans toutes les phases.

## Décisions

**Le manifeste, ce sont les releases GitHub.** L'APK y est déjà attaché (CLAUDE.md §Release, étape 8) ; `/releases/latest` exclut brouillons et pré-releases et renvoie le tag et ses assets. Pas de fichier `version.json` à publier en parallèle, donc pas de second endroit où la version peut se désynchroniser.

**Le dépôt est public** (`gh repo view` → PUBLIC), donc l'appel se fait sans token : un secret de moins à protéger. Le quota anonyme (60 requêtes/heure) est hors sujet pour un bouton tapé quelques fois par mois. En contrepartie, l'app envoie un `User-Agent` — GitHub répond 403 sans.

**`REQUEST_INSTALL_PACKAGES` fait partie du parcours, pas d'une découverte en cours de route.** La permission est déclarée au manifeste, mais Android exige en plus un accord manuel dans « Installer des applications inconnues ». `canRequestPackageInstalls()` est donc interrogé **avant** le téléchargement : découvrir le refus après des dizaines de mégaoctets, sans rien pouvoir en faire, est exactement le parcours à éviter. Si c'est non, l'écran explique et ouvre `MANAGE_UNKNOWN_APP_SOURCES`. Android ne rend aucun résultat au retour, alors le tap suivant relit simplement la permission.

**Intent `ACTION_VIEW` plutôt que `PackageInstaller`.** L'API de session apporte les APK splittés et des codes d'échec précis, dont un sideload mono-APK n'a aucun usage, au prix d'un `BroadcastReceiver` et d'un `PendingIntent`. L'Intent a la forme déjà en place dans `share.rs` — construire, accorder, `startActivity` — et l'installeur système affiche sa propre progression et ses propres erreurs. `ACTION_INSTALL_PACKAGE`, lui, est déprécié.

**L'APK descend dans `getCacheDir()/updates/`, pas dans `getFilesDir()`.** Les règles d'Auto Backup énumèrent ce qui vit à côté de la base ; un APK de release y mangerait le quota de 25 Mo dont dépend la comptabilité. Le cache est aussi ce qu'Android récupère sous pression de stockage. Il est servi par `UpdateFileProvider`, non exporté, en lecture seule, borné à ce répertoire — la garde anti-traversée est mutualisée avec `ExportFileProvider` dans `PrivateFileProvider`.

**Pas de somme de contrôle publiée.** Ce qui protège l'installation est la règle d'Android : un APK signé d'une autre clé ne remplace pas celui-ci, et le keystore vit hors du dépôt (CLAUDE.md §Sécurité). Un hash publié à côté de l'asset serait signé de la même autorité que l'asset — il ne prouverait rien de plus. Restent HTTPS et le refus de toute URL d'asset qui ne commence pas par `https://github.com/`, pour qu'un manifeste malformé ne puisse pas pointer le téléchargement ailleurs.

**Échec en cours de route : on s'arrête, on le dit, elle réessaie.** Pas de reprise ni de file d'attente — même choix que l'envoi email. Le téléchargement écrit dans `mise-a-jour.apk.part` et ne renomme qu'après `sync_all()` : une connexion coupée ne laisse pas un fichier que l'installeur ouvrirait pour le refuser. Le fichier cible précédent est supprimé avant de commencer, pour qu'un ancien APK ne soit jamais ce qui s'installe. Une fois l'Intent parti, l'installeur possède l'écran : succès et échecs sont les siens.

**Rien au démarrage.** `PRODUCT.md` garde le réseau hors du chemin de saisie. La vérification est ce bouton, et rien d'autre : une app qui traîne sur un splash parce que GitHub est lent est pire qu'une app en retard d'une version.

## Conséquence à traiter avant la première release mise à jour

Le `build.gradle.kts` généré par `dx` fixe `versionCode = 1` en dur, et le binaire `dx` n'expose aucune clé de configuration pour le changer. Android refuse une rétrogradation, pas une réinstallation à `versionCode` égal : l'installation passe et `/data/data/` est conservé. Mais CLAUDE.md §Release exige un incrément strict à chaque APK livré, et cette exigence n'est aujourd'hui pas tenable telle quelle. À arbitrer hors de cette tâche : patcher le `build.gradle.kts` après génération, ou aligner CLAUDE.md sur ce que l'outil permet.

## Considered Options

- **Fichier `version.json` sur un hébergement tiers** — rejeté : un second point de publication à tenir synchronisé avec les releases, pour rien.
- **Dépôt privé + token embarqué** — rejeté : le token serait un secret à protéger dans l'app, alors que le dépôt public rend l'appel anonyme.
- **`PackageInstaller` (API de session)** — rejeté : `BroadcastReceiver` + `PendingIntent` pour des fonctions (APK splittés, codes d'échec) qu'un sideload mono-APK n'utilise pas.
- **Vérification silencieuse au démarrage** — rejetée : le réseau ne doit pas toucher le chemin de saisie.
- **Play Store / F-Droid** — hors périmètre v1 (CLAUDE.md).
