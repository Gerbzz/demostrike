# Native CSDM combat import capture

[Hosted Windows run 39024060815](https://github.com/Gerbzz/DemoStrike/actions/runs/39024060815)
tested source head `5ea49f2e386551082732f1d1bb7afca5e89d5488`.
The `upstream-renderer`, `verify`, and `original-native` jobs all passed.
`proof.json` records the PR merge commit, native executable hash, synthetic
input and library hashes, and hashes of all 13 PNG captures. All 13 PNGs
matched their recorded SHA-256 values after download. The captures were
visually inspected at 1024 × 768 and show an actual Tauri WebView2 app window
running the pinned original CS Demo Manager renderer.

- `original-matches-empty.png`: original shell and actionable local demo import.
- `imported-combat-matches.png`, `imported-combat-overview.png`,
  `imported-combat-rounds.png`, `imported-combat-players.png`, and
  `imported-player-detail.png`: native UI import, match navigation, bounded
  combat/observed-side facts, and player detail from public synthetic facts.
- `ported-*.png`: older read-only library, explicit unavailable values,
  unsupported analysis route, and synthetic demo-folder workflow.

The hosted smoke uses a compile-time `synthetic-import-smoke` parser seam to
feed public synthetic facts through the real import command, source snapshot,
analysis, SQLite transaction, and UI reader. Normal builds use the CS2 native
decoder. This capture proves the app workflow and screens; it does not prove
that the updated decoder can parse Neil's private demo. That demo and its
existing library were inspected only in place, read-only, and were not
uploaded. The exact field and uncertainty contract is in `../CONTRACT.md`.
