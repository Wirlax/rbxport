# Prise en main du fork Wirlax/rbxport

Ce dépôt est un fork de [chrisle/rbxport](https://github.com/chrisle/rbxport), un
gestionnaire de bibliothèque rekordbox (Tauri 2, React/TypeScript, crates Rust `rbl-*`).
Upstream vise « moins de features » ; ce fork ajoute les nôtres, pensées pour préparer des
sets DJ avec Claude. Ce guide explique comment installer cette version et se servir de
chaque ajout. Pour travailler sur le code, la suite est dans [CLAUDE.md](CLAUDE.md) et dans
la [documentation d'upstream](docs/development/getting-started.md).

## Ce que le fork ajoute

| Feature | En deux mots |
| --- | --- |
| Memory cues toutes les 16 mesures | Un bouton (ou `V`) pose des memory cues toutes les 16 mesures, de la tête de lecture jusqu'au début. |
| Organize Library | Range chaque fichier dans `<dossier>/<Artiste>/<Album>/`, avec aperçu, sauvegarde et annulation. |
| App installée sans mise à jour | `pnpm app:install` remplace `/Applications/rbxport.app` par ce fork ; la mise à jour automatique est coupée. |
| Mini-sets | Des blocs de 2 à 4 morceaux séparés par des pistes « séparateur », que l'on mixe comme un tout. |
| Serveur MCP `rbxport` | Claude (Desktop ou Code) cherche dans la bibliothèque, propose des mini-sets, les écrit après validation, les modifie ou les retire. |
| Liens à télécharger | Pour une liste de morceaux : ceux déjà dans la bibliothèque, les liens Deezer sûrs, les versions douteuses, et SoundCloud pour le reste. |

## Avant de commencer : à lire

- **C'est votre vraie bibliothèque rekordbox.** rbxport lit et écrit le `master.db` que
  rekordbox utilise. Faites une sauvegarde de la bibliothèque avant d'essayer les écritures.
- **rekordbox (et son agent) doit être fermé pour écrire.** Toute écriture est refusée
  sinon.
- **Library Protection est active par défaut** et bloque les modifications faites à la main
  dans l'app. Seule l'écriture des mini-sets par le serveur MCP passe outre, et seulement
  après validation dans la conversation.
- **L'installation remplace l'app officielle.** Le fork a le même identifiant
  (`com.rbxport.app`) que la version officielle ; l'ancienne app part à la Corbeille. La
  vérification des mises à jour refuse volontairement : le build officiel écraserait les
  features du fork.
- **macOS seulement** pour l'app installée et le serveur MCP : les écritures passent par
  AppleScript. Testé sur Apple Silicon.

## Prérequis

- macOS avec les Xcode Command Line Tools (`xcode-select --install`).
- Node 24 et pnpm : `corepack enable` installe le pnpm 10.17.1 épinglé dans `package.json`.
- Rust par [rustup](https://rustup.rs) (ou [mise](https://mise.jdx.dev)) : `rust-toolchain.toml`
  impose la version 1.99, installée automatiquement au premier build.
- rekordbox 7 installé, avec sa bibliothèque : rbxport la trouve tout seul.
- **Au moins 20 Go de libre** : le dossier `target/` des builds dépasse vite 15 Go.
- Pour le serveur MCP : [Claude Desktop](https://claude.ai/download) et/ou
  [Claude Code](https://docs.claude.com/en/docs/claude-code).

## Installation

```bash
git clone https://github.com/Wirlax/rbxport.git
cd rbxport
pnpm install
pnpm app:install
```

`pnpm app:install` construit l'app et le serveur MCP, puis :

- remplace `/Applications/rbxport.app` (l'ancienne part à la Corbeille) ;
- installe le serveur dans `~/.local/bin/rbxport-mcp`.

Le premier build prend une dizaine de minutes, les suivants 3 à 4 minutes. rbxport doit
être fermé pendant l'installation. Trois variantes :

| Commande | Usage |
| --- | --- |
| `pnpm app:install` | Le cas courant : profil Cargo `fork` (LTO thin), rapide. |
| `pnpm app:install --release` | Pour une vraie release : toutes les optimisations, 8 à 9 minutes. |
| `pnpm mcp:install` | Seulement le serveur MCP, une vingtaine de secondes ; rbxport peut rester ouvert. |

Relancez `pnpm app:install` après chaque `git pull`.

### Brancher le serveur MCP sur Claude

Claude Code :

```bash
claude mcp add --scope user rbxport -- ~/.local/bin/rbxport-mcp
```

Claude Desktop : ajoutez ceci à
`~/Library/Application Support/Claude/claude_desktop_config.json`, en gardant le reste du
fichier, puis redémarrez Claude Desktop. Remplacez `<vous>` par votre nom d'utilisateur
macOS :

```json
{
  "mcpServers": {
    "rbxport": { "command": "/Users/<vous>/.local/bin/rbxport-mcp", "args": [] }
  }
}
```

À la première écriture, macOS demande d'autoriser votre client Claude (ou votre terminal) à
contrôler rbxport : acceptez. Le réglage se retrouve dans Réglages Système › Confidentialité
et sécurité › Automatisation.

### Créer les séparateurs (indispensable pour les mini-sets)

Les mini-sets reposent sur 101 pistes d'une seconde titrées `SEPARATORBREMSEN`, puis
`SEPARATORBREMSEN 001` à `SEPARATORBREMSEN 100`. Chaque séparateur est un fichier distinct,
parce qu'une playlist ne contient un morceau qu'une fois. Ce titre est fixé dans le code
(`SEPARATOR_TITLE`, dans `crates/rbl-db/src/mini_sets.rs`).

Ce script fabrique des WAV silencieux, sans rien à installer :

```bash
mkdir -p ~/Music/SEPARATORBREMSEN && cd ~/Music/SEPARATORBREMSEN && python3 - <<'EOF'
import wave
names = ["SEPARATORBREMSEN"] + [f"SEPARATORBREMSEN {n:03}" for n in range(1, 101)]
for name in names:
    with wave.open(f"{name}.wav", "wb") as w:
        w.setnchannels(2); w.setsampwidth(2); w.setframerate(44100)
        w.writeframes(b"\0" * 44100 * 4)
EOF
```

Importez ensuite le dossier dans la bibliothèque : dans rbxport, menu File › Import Folder…,
ou dans rekordbox. Les fichiers n'ont pas de tags, le titre est donc leur nom de fichier.

## Se servir des features

### Memory cues toutes les 16 mesures

Dans le lecteur, en mode 1 PLAYER, groupe MEMORY : le bouton « 16 », ou la touche `V`.
Le calcul part de la tête de lecture, calée sur le beat si Q est actif. Il compte 64 temps
sur la beatgrid jusqu'au début du morceau et remplace les memory cues simples, sans toucher
aux loops. La limite rekordbox de 10 memory cues s'applique.

### Organize Library

Préférences › Advanced › Database. Choisissez un dossier de musique : chaque fichier est
déplacé dans `<dossier>/<Artiste>/<Album>/<nom d'origine>` et la bibliothèque suit. Un
aperçu et une confirmation viennent avant, une sauvegarde est faite, et le dernier
rangement peut être annulé. Les morceaux cloud ou streaming, `~/Music/rekordbox` et
`~/Music/PioneerDJ` ne bougent pas. Le journal est dans
`~/Library/Application Support/rbxport/backups/organize/`.

### Mini-sets avec Claude

Un mini-set est un bloc de 2 à 4 morceaux qui se mixent entre eux et se jouent comme un
tout ; l'ordre des blocs n'a pas d'importance. Dans une playlist, chaque bloc suit un
séparateur. Un séparateur suivi de rien est une place réservée, remplie en premier.

Le serveur donne à Claude dix outils :

| Lecture (base lue directement, sans l'app) | Écriture (via rbxport) |
| --- | --- |
| `list_playlists`, `search_tracks`, `compatible_tracks` (tonalités compatibles sur la roue Camelot, BPM), `get_mini_sets`, `preview_mini_sets`, `preview_mini_set_change`, `find_links` | `add_mini_sets`, `change_mini_set`, `remove_mini_set` |

Exemples de demandes :

- « Fais-moi 2 mini-sets DnB autour de Boulder dans une nouvelle playlist DEMO. »
- « Dans DEMO, remplace Hybrid par un autre morceau compatible. »
- « Supprime le bloc 100 de DEMO. »

Claude regarde d'abord vos playlists de mini-sets pour apprendre ce que vous enchaînez,
puis **propose les blocs en texte** (titre, artiste, BPM, tonalité). Il **n'écrit qu'après
votre accord explicite**, en montrant un aperçu avant. Sa méthode est décrite dans les
consignes du serveur, au début de `crates/rbl-mcp/src/server.rs` : elle reflète la façon
de faire de Ronan, adaptez-la à la vôtre.

Si rbxport est fermé, le serveur l'ouvre en arrière-plan. Il le referme 10 minutes après la
dernière écriture, mais seulement s'il l'a ouvert lui-même et que l'app n'est pas au
premier plan.

### Trouver les liens des morceaux qui manquent

Donnez une liste à Claude : « Trouve-moi les liens pour : Netsky – Rio, Daft Punk – One
More Time, … ». Pour chaque morceau :

1. **déjà dans la bibliothèque** : pas de lien, et Claude signale quand seule une autre
   version y est ;
2. **sur Deezer** : quand la correspondance est sûre (même titre, même version, même
   artiste), les liens arrivent dans un seul bloc, un par ligne. Sans version précisée,
   l'Extended Mix passe en premier ;
3. **incertain** : une autre version (remix, VIP, edit, radio edit) ou un autre artiste ;
   les candidats sont listés avec ce qui diffère, à vous de choisir ;
4. **absent de Deezer** : Claude cherche la page SoundCloud du morceau avec sa recherche
   web, ou donne un lien de recherche SoundCloud.

L'API publique de Deezer s'utilise sans compte, avec un quota de 50 requêtes par
5 secondes. Le serveur respecte ce quota, et une liste de 100 morceaux au plus passe en
quelques secondes. Récupérez les morceaux par des moyens autorisés et importez-les dans
votre bibliothèque. Claude les retrouvera ensuite pour en faire des mini-sets ; l'import
automatique n'existe pas encore.

## Dépannage

| Symptôme | Que faire |
| --- | --- |
| « rekordbox is running » | Quittez rekordbox et son agent (`rekordboxAgent`). |
| Claude ne voit pas les outils `rbxport` | Ouvrez une nouvelle session Claude Code ou redémarrez Claude Desktop ; une session ouverte garde l'ancien serveur. |
| Écriture refusée côté macOS | Réglages Système › Confidentialité et sécurité › Automatisation : autorisez votre client Claude à contrôler rbxport. |
| `pnpm app:install` refuse de démarrer | Quittez rbxport : on ne remplace pas une app en cours d'exécution. |
| Mini-sets refusés, « no separator left » | Importez les séparateurs (voir plus haut). |
| « Operation not permitted » à la lecture des fichiers | Autorisez rbxport à lire le dossier concerné (Téléchargements, disque externe…). |
| Disque plein pendant un build | `rm -rf target/debug/incremental`, sans risque ; `target/release` ne sert qu'à `--release`. |

## Pour contribuer

- **[CLAUDE.md](CLAUDE.md)** décrit les règles du fork : code propre au fork marqué « This
  fork's own », tests, workflow git, traductions.
- **Aucun test d'écriture sur une vraie bibliothèque** : les tests Rust tournent avec
  `RBXPORT_TEST=1` sur une bibliothèque de test (`rbl_db::fixture::build`). Pour lancer
  l'app de dev sans risque d'écriture : `RB_LITE_TEST=1 pnpm dev`.
- **Validation avant une PR** :
  - Rust : `RBXPORT_TEST=1 cargo clippy -p <crate> --all-targets -- -D warnings`, puis
    `RBXPORT_TEST=1 cargo test -p <crate>` ;
  - front : `pnpm typecheck && pnpm lint && pnpm test && pnpm build && pnpm budget`.
- **Les PR vont sur le fork** : `gh pr create --repo Wirlax/rbxport --base main`. Sans
  `--repo`, gh vise le dépôt d'upstream.
