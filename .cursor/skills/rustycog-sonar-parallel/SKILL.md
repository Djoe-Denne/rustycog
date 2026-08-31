---
name: rustycog-sonar-parallel
description: >-
  Isolates parallel Sonar/Clippy lots on the rustycog SDK so agents do not
  collide on files, wipe sources via Serena ENOSPC, change public APIs, or
  fight the Cargo target lock. Use when fixing Sonar issues on project
  Djoe-Denne_rustycog, running parallel Clippy/Sonar campaigns, editing
  rustycog-*/src, assigning lots, or when the user mentions unused_async,
  must_use, .auto-sonar-patcher, Serena wipe, or cargo lock target/.
---

# rustycog — lots Sonar / Clippy en parallèle

Lis ce skill **avant** toute campagne Sonar, tout lot Clippy parallèle, ou toute
édition de `rustycog-*/src` pour fermer des issues. N’édite pas le code métier
en suivant ce skill : il dicte **comment** travailler, pas quoi patcher.

## Checklist au démarrage

```
- [ ] Serena : initial_instructions puis activate_project rustycog
- [ ] Espace disque C: ≥ 2 Go
- [ ] Tes fichiers assignés : aucun autre agent dessus
- [ ] Lot = 12–15 uniques (file+règle+ligne+message), refill dans la même file
- [ ] Aucun cargo tant qu’un autre agent édite
```

## Contexte campagne

projet Sonar `Djoe-Denne_rustycog`, `sonar-project.properties`, crate unifié `#[path]` vers `rustycog-*/src`. Issues souvent **dupliquées** (même file+règle+ligne+message, 2 clés). Unité de conflit = **fichier**, pas le crate Cargo.

Le crate publié est `rustycog-framework` (`src/lib.rs`). Chaque feature réexporte
un fichier historique :

```rust
#[cfg(feature = "http")]
#[path = "../rustycog-http/src/lib.rs"]
pub mod http;
```

Deux agents sur `rustycog-http` **et** `rustycog-command` peuvent se croiser
s’ils touchent le **même chemin de fichier**. Découper par **chemin**, pas par
feature Cargo.

`sonar.rust.clippy.enabled=false` : Clippy arrive via
`sonar.rust.clippyReport.reportPaths=target/sonar/clippy.json`. Les clés Sonar
Cloud sont souvent doublées (même finding, deux `key`). Fermer un unique ferme
les deux au scan suivant — ne pas « traiter » deux fois.

## Isolation

un fichier = un agent à la fois. Gros fichier (`rustycog-config/src/lib.rs`) : tranches de lignes **séquentielles même agent**. Lots ~12–15 uniques. Refill dans la **même file**.

- Ne pas prendre `rustycog-config/src/lib.rs` à deux, même sur des plages
  disjointes, sauf consigne explicite de l’opérateur.
- Après un lot : refill **uniquement** des uniques encore ouverts dans tes
  fichiers déjà lockés. Ne pas « aider » un autre agent sur son fichier.
- Si un fichier t’est retiré ou déjà dirty hors lot : STOP, ne pas merger à la
  main.

## API

INTERDIT de changer une signature publique sans skill/migration. Préférer docs `# Errors`/`# Panics`, réécriture locale, `expect` documenté, `#[allow(clippy::…)]` justifié, `#[must_use]`. `unused_async` public → allow, pas retirer `async`.

Est **public** tout `pub fn` / `pub async fn` / trait / type / champ `pub`
réexporté par `rustycog-framework` (modules `command`, `http`, `events`,
`config`, `db`, `permission`, `outbox`, `logger`, `testing`, `server`).

Correctifs sûrs (dans l’ordre) :

1. Docs rustdoc `# Errors` / `# Panics` (Clippy `missing_errors_doc` /
   `missing_panics_doc`).
2. Réécriture **locale** (helpers privés, `map_or_else`, `strip_prefix`).
3. `expect("raison")` + `# Panics` si le panic existait déjà (`unwrap`).
4. `#[allow(clippy::…)]` **une ligne au-dessus**, commentaire **pourquoi**.
5. `#[must_use]` sur un builder fluent qui retourne `Self`.

Interdit sans skill de migration **et** accord opérateur :

- changer args / type de retour / `async` ↔ sync d’une API publique
- retirer `async` pour `unused_async` (même si le corps n’await plus)
- changer un champ `pub` (ex. `version: i32` → `u32`)
- passer `Result` → panic ou l’inverse sur une `pub fn`

Les helpers **privés** (`fn foo`, pas `pub`) peuvent perdre `async` ou changer
de signature. `create_base_test_config` est privé.

## Disque / Serena

`replace_content` Serena + ENOSPC **vide le fichier**. Avant d’éditer : vérifier l’espace C: (≥ 2 Go). Si wipe : restaurer `git checkout -- <file>` ou blob HEAD, STOP, ne pas réécrire de tête. Edits ciblés, pas de rewrite entier.

```powershell
Get-PSDrive C | Select-Object Used,Free
```

- Préférer `replace_symbol_body` / `insert_before_symbol` / `insert_after_symbol`
  ou un `replace_content` **étroit**. Jamais renvoyer le fichier entier.
- Symptôme wipe : fichier 0 octet, ou plus que le preamble / un seul `{`.
- Après restore : **STOP**. Dire à l’opérateur. Ne pas reconstruire de mémoire.

## Build

5 agents en parallèle = **ne pas** lancer `cargo` (lock `target/`). Check une fois les files closes. `cargo check --all-features` peut échouer sur `rdkafka-sys` (clone) : ce n’est pas le code.

Ne pas lancer `cargo test`, `clippy`, ni écrire sous `target/` pendant que
d’autres lots sont ouverts.

## Interdit

`.auto-sonar-patcher/` (patches cassés). Ne pas `change_sonar_issue_status` sauf vrai faux positif. Ne pas se fier aux numéros de lignes Sonar après insertions de docs.

- Ne pas générer / appliquer des patchs depuis `.auto-sonar-patcher/`.
- Relire le fichier **actuel** (Serena / git) ; les `textRange` Sonar datent
  du scan.

## MCP

`.cursor/mcp.json` projet avec `CONTEXT_MODE_PROJECT_DIR` = racine rustycog. Serena `activate_project` rustycog (un seul projet actif).

- Ne pas mettre `CONTEXT_MODE_PROJECT_DIR` dans `~/.cursor/mcp.json`.
- Si Serena pointe ailleurs : `activate_project` sur
  `C:\Users\djden\source\repos\rustycog` avant tout symbole / edit.

## Après lots

`cargo check` (features défaut) + `cargo test --lib` ; scan Sonar suivant pour fermer les clés.

Un seul agent (ou l’opérateur) lance le check **après** que toutes les files
sont closes. Features défaut, pas `--all-features` (rdkafka-sys).

## Migration : pas nécessaire, voici pourquoi

Audit `git diff 1c10714 6b7f5fd -- '*.rs'`.
**Aucune rupture compile-time** pour un crate consommateur. Pas de skill
`rustycog-api-migration`.

Après le skill (`47ca8dc`, 2026-08-30 15:56) :
- `a46d22a` — fmt
- `6b7f5fd` — rewrite locale `rustycog-http/src/tracing_middleware.rs`
  (E0502 : cloner `x-correlation-id` avant insert). Pas d’API publique,
  pas de nouvelle famille Clippy.

- Aucune ligne `pub fn` / `pub async fn` / `pub struct` / `pub trait` / champ
  `pub` n’a changé de signature.
- `unused_async` public : `#[allow]`, `async` conservé
  (`OutboxDispatcher::stop`, `DbTestUtils::cleanup_test_data`, fixtures
  testing, constructeurs Kafka internes appelés par `new().await`).
- `version: i32` → `u32` uniquement sur le struct **privé**
  `GenericDomainEvent` (`rustycog-events/src/sqs.rs`). `StoredOutboxEvent`
  garde `i32` en base ; `DomainEvent::version` fait `u32::try_from`.
- `serialize_event` / `map_jwt_error` / `parse_message` / cleanups
  testcontainer : **privés**.
- Reste : rustdoc `# Errors` / `# Panics`, `#[allow]`, `unwrap` → `expect`
  ou `PoisonError::into_inner`, helpers locaux.

Ne pas inventer une migration « pour la forme ».

### Warnings possibles (`#[must_use]`)

Ajouts de cette campagne — **warnings** rustc/Clippy chez le service si le
builder n’est pas chaîné jusqu’à un sink (`into_router` / `build` /
enregistrement consommé) :

| Item | Crate / fichier |
|------|-----------------|
| `CommandRegistryBuilder::register` | `rustycog-command/src/registry.rs` |
| `RouteBuilder::{get,post,put,delete,patch}` | `rustycog-http/src/builder.rs` |
| `RouteBuilder::health_check` | idem |
| `RouteBuilder::with_permission_on` | idem |

`RouteBuilder::nest` avait déjà `#[must_use]` avant la campagne.

Un service qui écrit `builder.get("/x", h);` sans réutiliser la valeur peut
voir `unused must-use`. Correctif côté service : chaîner
(`.get(...).post(...).into_router()`), pas un bump d’API rustycog.

## Exemple — `unused_async` public

```rust
/// Request the dispatcher loop to exit.
///
/// # Errors
///
/// This method does not currently fail.
#[allow(clippy::unused_async)] // Public API stays async so callers can `.await` stop.
pub async fn stop(&self) -> Result<(), ServiceError> {
```

## Exemple — interdit

```rust
// NON — rupture pour tous les `.await stop()`
pub fn stop(&self) -> Result<(), ServiceError> { ... }

// NON — rewrite entier via Serena replace_content
// NON — cargo check pendant que 4 autres agents éditent
```
