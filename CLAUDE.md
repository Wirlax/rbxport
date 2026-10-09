# CLAUDE.md — fork Wirlax/rbxport

Fork personnel de [chrisle/rbxport](https://github.com/chrisle/rbxport) : gestionnaire de
bibliothèque rekordbox (Tauri 2, React/TypeScript, crates Rust `rbl-*`). `origin` =
`Wirlax/rbxport`, `upstream` = `chrisle/rbxport`.

La doc du projet d'origine reste la référence ; la lire avant de toucher une zone
inconnue, ne pas la recopier ici :
[architecture](docs/development/architecture.md) ·
[conventions](docs/development/conventions.md) ·
[tests](docs/development/testing.md) · [débogage](docs/development/debugging.md).

Upstream vise « moins de features » ; ce fork ajoute les nôtres. Le code propre au fork
le dit en commentaire (« This fork's own ») pour s'y retrouver lors d'un merge upstream.

## Ce que le fork ajoute

| Feature | Où | PR |
| --- | --- | --- |
| **Memory cues toutes les 16 mesures** : bouton « 16 » du groupe MEMORY (mode 1 PLAYER) ou touche `V`. Part de la tête de lecture (calée sur le beat si Q), compte 64 temps sur la beatgrid jusqu'au début, remplace les memory cues simples, garde les loops, limite rekordbox de 10. | `src/views/player/useMemoryCues.ts` (`storeEvery16Bars`), `src/lib/player.ts` (`beatsBackMs`), `src/lib/shortcuts.ts` | [#1](https://github.com/Wirlax/rbxport/pull/1) |
| **Organize Library** : Préférences › Advanced › Database. Déplace chaque fichier dans `<dossier>/<Artiste>/<Album>/<nom d'origine>`, met à jour `FolderPath`/`FileNameL`, aperçu + confirmation, sauvegarde avant, journal, annulation du dernier rangement. Laisse de côté cloud/streaming et `~/Music/rekordbox`, `~/Music/PioneerDJ`. | `src-tauri/src/organize.rs`, `crates/rbl-db/src/track_files.rs`, `src/views/settings/AdvancedPane.tsx` | [#2](https://github.com/Wirlax/rbxport/pull/2) |

Journal d'Organize Library : `~/Library/Application Support/rbxport/backups/organize/*.jsonl`.

## Explorer le code : CodeGraph d'abord

- Index dans `.codegraph/` (exclu de git localement), mis à jour automatiquement.
  Résultats qui semblent périmés : `codegraph sync` ; reconstruction : `codegraph index`.
- Outil MCP `codegraph_explore` (avec `projectPath` = racine du repo) **avant** grep ou
  lecture de fichiers : une requête faite de noms de symboles et de fichiers renvoie leur
  source et les chemins d'appel. Un seul appel bien ciblé suffit souvent.
- Avant de modifier une fonction partagée : `codegraph impact <symbole>` ou
  `codegraph callers <symbole>` pour voir ce qui en dépend.
- Pour une plage précise d'un fichier déjà localisé, `sed -n` ou Read restent plus directs.

## Environnement local

- Node 24 et pnpm via mise ; Rust 1.99 via mise (version imposée par `rust-toolchain.toml`).
  Un shell ouvert avant l'installation de Rust ne voit pas `cargo` : `mise exec -- <cmd>`.
- `ls` est un alias (eza) dans le shell : utiliser `/bin/ls` dans les scripts.
- `pnpm dev:web` : interface seule, bibliothèque factice (`src/ipc/backend-mock.ts`),
  en lecture seule par défaut. Pour écrire : URL `/?writable=1` **et** localStorage
  `rbl.preferences` = `{"advanced":{"protectLibrary":false}}`.
- `pnpm dev` : la vraie app sur la vraie bibliothèque. Elle se recompile et redémarre à
  chaque changement de fichier Rust, `git switch` compris.
- `/Applications/rbxport.app` (version officielle) a le même identifiant
  (`com.rbxport.app`) : une seule instance à la fois, la nôtre se ferme aussitôt si
  l'officielle est ouverte.
- macOS doit autoriser l'app à lire le dossier Téléchargements (une partie de la
  bibliothèque y était), sinon « Operation not permitted ».

## Sécurité de la vraie bibliothèque

- Jamais de test d'écriture sur la vraie bibliothèque : tests Rust avec `RBXPORT_TEST=1`
  sur `rbl_db::fixture::build` dans un tempdir.
- Inspection en lecture seule :
  `RB_LITE_TEST=1 mise exec -- cargo run -q -p rbl-db --example sql -- "SELECT …"`.
- Toute écriture passe par `rbl_db::write::Writer` (bookkeeping USN de rekordbox) ; les
  requêtes SQL vivent dans `rbl-db`, pas dans `src-tauri`.
- rekordbox (et son agent) fermé pour écrire ; Library Protection est active par défaut.
- Opération de masse = aperçu + confirmation, sauvegarde avant, journal pour reprendre
  ou annuler (modèle : `organize.rs`).
- Les fichiers d'analyse ANLZ de rekordbox ne retiennent que `?/<nom de fichier>` : ne
  pas renommer les fichiers audio sans gérer ce point.

## Workflow git

- Une branche par sujet depuis `main` à jour (`feat/…`, `fix/…`, `docs/…`) ; commits
  atomiques, conventionnels, en anglais, terminés par la ligne `Co-Authored-By`.
- PR **sur le fork** : `gh pr create --repo Wirlax/rbxport --base main` (sans `--repo`,
  gh vise chrisle/rbxport). Merge : `gh pr merge <n> --repo Wirlax/rbxport --rebase
  --delete-branch`, puis `git switch main && git pull --ff-only origin main`.
- GitHub Actions est désactivé sur le fork : la validation locale fait foi.
- Récupérer upstream : `git fetch upstream`, puis merge de `upstream/main` sur une branche
  dédiée, tests complets avant de fusionner.

## Avant de dire « terminé »

- Front : `pnpm typecheck && pnpm lint && pnpm test && pnpm build && pnpm budget`.
  Le budget du bundle initial est à ~149/150 KB gz : le surveiller.
- Rust : `RBXPORT_TEST=1 mise exec -- cargo clippy -p <crates> --all-targets -- -D warnings`
  puis `RBXPORT_TEST=1 mise exec -- cargo test -p <crate>` (l'app est `-p rbxport`).
  Clippy pédant ; `unwrap`/`expect`/`panic` interdits hors tests.
- Nouvelle commande backend : type dans `src/ipc/types.ts`, appel dans `src/ipc/client.ts`,
  enregistrement dans `src-tauri/src/lib.rs` **et** mock dans `src/ipc/backend-mock.ts`.
- Changement visible : vérifier dans le navigateur avec `pnpm dev:web`.

## Textes de l'interface et traductions

- Un texte anglais en dur dans le JSX est une clé de traduction ; un texte dynamique passe
  par `t("… {placeholder}", { … })`.
- `pnpm locales` régénère tout depuis rekordbox installé et produit un gros diff hors
  sujet (catalogue upstream pas à jour). Ne s'en servir que pour **repérer** les nouvelles
  chaînes, puis `git checkout -- src/i18n/ui.json public/locales`, ajouter les chaînes à
  la main dans `src/i18n/ui.json`, lancer `node scripts/fill-missing-locales.mjs` (repli
  anglais) et traduire `public/locales/fr.json`. `src/i18n/index.test.ts` vérifie la
  couverture.

## Style

- Commenter le pourquoi, avec les marqueurs de preuve d'upstream (`[OBS]`, `[ASSUME]`,
  `[REF]`) quand un comportement de rekordbox est imité.
- Gros fichiers (`Player.tsx` ~3000 lignes) : modifier au plus près, pas de refacto
  opportuniste.
