# CS Demo Manager source baseline and Rust migration boundary

## Provenance and scope

The source-only tree at `upstream/cs-demo-manager/` is copied byte for byte from
[`akiver/cs-demo-manager@7270bc95c8cab87b7f9ddd490fea13c6eda3ace7`](https://github.com/akiver/cs-demo-manager/tree/7270bc95c8cab87b7f9ddd490fea13c6eda3ace7), version 3.20.1. Its upstream `LICENSE` is included; the root license is MIT, Copyright (c) 2014-present AkiVer. `tests/check_upstream_snapshot.py` checks all 2,827 staged upstream paths and Git blob IDs against `MIGRATION_LEDGER.csv`. The ledger marks these files `source_staged_unported` unless a separately tested adaptation already exists.

The tracked native `.node` add-on is excluded. The original InterVariable font is staged unmodified with the official Inter project's SIL Open Font License 1.1 notice (`LICENSE.inter.txt`), checked by Git blob ID. `static/`, build assets, plugins, and downloaded runtimes are not included. Their provenance and integration must be reviewed before packaging. No original file in this tree has been changed to make an unsupported feature appear to work.

## The actual application to retain

- `src/ui/renderer.tsx` initializes the React router and calls the Electron preload `window.csdm` API.
- `src/ui/bootstrap/root.tsx` composes the Redux, locale, settings, WebSocket, database, dialog, and application loaders. `src/ui/bootstrap/app.tsx` renders the real `LeftBar` with the route outlet.
- `src/ui/left-bar/left-bar.tsx` is the original 60 px icon rail: pinned player, Matches, Demos, Players, Teams, Downloads, Bans, Search, Analyses, Video queue, Settings.
- `src/ui/router.tsx` is the original nested hash route tree. Matches opens Match Overview, with Rounds, Players, Heatmap, Duels, Weapons, Grenades, 2D Viewer, Video, Chat, and Economy children. Other top-level routes retain their source components in the staged tree.
- `src/ui/matches/matches.tsx` uses the actual action bar, matches table provider, table, and status bar. `src/ui/match/overview/match-overview.tsx` uses MatchInformation, two team scoreboards, and RoundsTimeline.

## Service replacement map

| Original boundary | Source path | Rust/Tauri status | Missing contract before original UI can run |
| --- | --- | --- | --- |
| Window and filesystem preload | `src/preload/preload.ts`, `src/preload/preload-result.ts` | Unported | Typed Tauri replacements for the preload methods actually used by each retained screen; no global fabricated API. |
| WebSocket request/reply and push | `src/ui/web-socket-client.ts`, `src/server/messages/`, `src/server/handlers/` | Unported | UUID-correlated requests, errors, and push messages routed to Rust commands/events with the original payload shapes. |
| Initialization | `src/ui/bootstrap/app-loader.tsx`, `src/server/handlers/renderer-process/initialize-application-handler.ts` | Unported | Settings, maps, cameras, tags, match checksums, download and video queues, and account state. The candidate's SQLite snapshot cannot substitute for this payload. |
| Database lifecycle | `src/ui/bootstrap/database-loader.tsx`, `src/node/database/` | Unported | Deliberate PostgreSQL-to-Rust persistence migration and schema parity. |
| Match library | `src/ui/matches/`, `src/node/database/matches/fetch-matches-table.ts` | Original source staged; candidate proof separately runnable | `MatchTable` has checksum, teams, dates, stats, comments, tags, sources, and filters absent from the current SQLite `MatchSummary`. Do not fill unknown values with zeros or labels. |
| Match overview | `src/ui/match/overview/`, `src/node/database/match/` | Original source staged; candidate proof separately runnable | Full `Match`, team identity, per-player scoreboard stats, round sides, timeline, actions, and map thumbnail contract. |
| Demo analysis/import | `src/ui/demos/`, `src/ui/analyses/`, `src/server/handlers/renderer-process/`, DemoStrike root `src/demo_import.rs` and `src/native_grenade_parser.rs` | Rust importer exists in DemoStrike; candidate integration unverified and parked | Actual original analyses workflow, progress/error/cancel semantics, parser capability reporting, and transactional persistence mapped to Rust. |
| Remaining sidebar routes | `src/ui/router.tsx`, `src/ui/{players,teams,downloads,ban,search,videos,settings}/` | Original source staged, services unported | Route-specific backend and asset contracts. No placeholder route is treated as functional. |

## Runnable proof and limit

The existing `rust-candidate/src-tauri/src/library.rs` and `src/bridge.ts` are a **separate runnable read-only Tauri proof**: hosted CI launched native WebView2 and verified `list_matches` and `get_match_detail` against synthetic SQLite. Its small custom renderer is not CS Demo Manager's original renderer, sidebar, or route tree. It is retained only as an adapter and test fixture while the original source is migrated. The full original UI is **not yet runnable in Tauri** because the preload, initialization, WebSocket, database, and route payloads above are not connected. A successful source-integrity check must not be described as visual or functional acceptance of the upstream app.

Hosted CI also attempts to compile the unmodified upstream React renderer with its pinned pnpm lockfile and native postinstall scripts disabled. This checks whether the original frontend dependency tree can be built separately from Electron and its server. A successful renderer compile still does not satisfy the startup contracts or make any sidebar route functional; a failed compile is a concrete dependency boundary to resolve, not a screenshot or runtime result.
