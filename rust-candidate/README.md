# DemoStrike Rust candidate

An isolated Tauri 2 and React migration candidate for CS Demo Manager 3.20.1. The source baseline is [`akiver/cs-demo-manager` at `7270bc95c8cab87b7f9ddd490fea13c6eda3ace7`](https://github.com/akiver/cs-demo-manager/tree/7270bc95c8cab87b7f9ddd490fea13c6eda3ace7). The candidate does not replace the current DemoStrike, CSthrowback, ClipStrike, or FilmCraft applications.

The separately runnable proof reads an existing DemoStrike SQLite import and shows a small match library, overview, rounds, players, and utility events. It never imports a demo, writes to the database, launches Counter-Strike, or captures video. Effect metrics honor DemoStrike's versioned supported view; unverified raw values appear as Unavailable. This proof is not the original CS Demo Manager UI or a completed rewrite.

The exact pinned upstream **application source** now lives under `upstream/cs-demo-manager/`, including its real React renderer, sidebar, route tree, screens, preload and server source. See `docs/UPSTREAM_BASELINE.md` for excluded payloads, source-integrity test, and the service contracts still needed to run those original screens in Tauri. The source is staged, not claimed as a working migrated application.

## Intended development run

Install dependencies only in an environment with sufficient disk space, then run `npm install` and `npm run tauri dev`. Use Browse or the path field to open an existing DemoStrike `.sqlite` file. Local preparation did not install packages, compile Rust, run Tauri, or produce a native screenshot because the Office disk floor prohibits local builds and executable downloads. Hosted CI later built the source, but no release binary is provided.

## Source map

- `upstream/cs-demo-manager/src/`: original pinned UI and service source; not yet connected to Tauri.
- `src-tauri/src/library.rs`: read-only, validated SQLite proof adapter.
- `src/upstream/`: the first bounded, pinned upstream match-score component and helper, rendered in the library and overview. Provenance and acceptance gaps are in `docs/UPSTREAM_ADAPTATIONS.md`.
- `src-tauri/src/capture_provider.rs`: pure request/provider contract for the future HLAE-to-Rust parity track; no runtime provider exists yet.
- `src-tauri/src/lib.rs`: typed Tauri commands for match list and detail.
- `src/bridge.ts`: typed frontend command adapter; future upstream `window.csdm` and WebSocket messages belong here.
- `src/App.tsx`, `src/components/`: match library and overview UI.
- `docs/MIGRATION_LEDGER.csv`: exhaustive upstream file ledger, marking staged source separately from verified ports.
- `docs/UPSTREAM_BASELINE.md`: exact original UI, routes, service replacement map, exclusions and proof limits.
- `docs/REUSE_AUDIT.md`: exact upstream influence, copied notice, license gates and source scan.
- `docs/INTEGRATION_PARITY.md`: integration boundaries and negative tests.
- `docs/SCHEMA_CONTRACT.md`: required read-only SQLite tables and columns, plus fixture limits.
- `docs/NATIVE_SMOKE.md`: real hosted WebView2 launch, IPC assertions, and synthetic capture provenance.
- `docs/DEPENDENCIES.md`: direct dependency license register and unresolved distribution gates.

The repository-root [GitHub Actions workflow](../.github/workflows/rust-candidate-verify.yml) passed on [draft PR #3](https://github.com/Gerbzz/DemoStrike/pull/3) at `035e625` on [hosted Windows](https://github.com/Gerbzz/DemoStrike/actions/runs/39014587522): frontend build, 11 Rust tests, native Tauri compilation, and real WebView2 IPC smoke with three synthetic-data screenshots. This validates the small read-only proof, not the staged original CS Demo Manager renderer. The next run also checks the original upstream files byte for byte. The workflow has read-only repository permissions and uploads no executable. Transitive notices and packaging remain open.

The current Rust service contract is deliberately narrower than the upstream PostgreSQL, analyzer, Game Coordinator, and plugin contracts. Their gaps are tracked in the ledger and parity document.
