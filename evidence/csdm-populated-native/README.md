# Connected original CS Demo Manager route: native evidence

[Hosted Windows run 39021825918](https://github.com/Gerbzz/DemoStrike/actions/runs/39021825918)
tested source head `f8e55d5e847b6c9b93f4ad76dcded4ec73df00af`.
`run37-proof.json` records the PR merge commit and hashes for the native
executable, synthetic SQLite fixture, two generated demo files, and eight
app-window captures. All eight PNG hashes were checked against the proof
after downloading the hosted artifact. The captures were visually inspected
at 1024 x 768. They come from the actual Tauri WebView2 window, not mockups.

- `run37-original-matches-empty.png`: original CSDM LeftBar, router,
  Matches action bar, title bar, status bar, and the fresh-library state.
- `run37-ported-populated-matches.png`: original CSDM table engine with
  **ported columns** limited to fields recorded in a synthetic DemoStrike
  SQLite library. CT/T scores are side scores; import time is not match time.
- `run37-ported-match-overview.png`, `run37-ported-match-rounds.png`,
  `run37-ported-match-players.png`, and `run37-ported-match-utility.png`:
  original nested route/tab foundations with **ported, data-limited
  content**. Unknown statistics are unavailable. Utility blind/damage
  values appear only under the verified v6 attribution contract.
- `run37-ported-match-unsupported.png`: direct link to a deeper unported
  analysis route explains its missing data instead of showing fabricated
  metrics.
- `run37-ported-demo-folder.png`: read-only direct `.dem` folder browser
  with generated synthetic files; import, parsing, and playback remain open.

The exact field mapping and missing full-stat upstream views are described
in `CONTRACT.md`. The original pinned CSDM source snapshot remains intact.
This public hosted fixture contains no private user demo, library, or match
data. The OS file/folder dialog itself was not exercised by this smoke.

