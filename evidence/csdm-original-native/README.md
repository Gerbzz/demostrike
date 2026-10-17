# CS Demo Manager native renderer evidence

These are **unaltered app-only screenshots** from the hosted Windows Tauri
WebView2 smoke run [39018711048](https://github.com/Gerbzz/DemoStrike/actions/runs/39018711048).
The run's `upstream-renderer`, `verify`, and `original-native` jobs passed.

The source branch head at the time was
`3d54745dd2e880523036bc054f6e4af3d04c9ea8`. GitHub Actions built
the PR merge commit `3bf23aa2c57cc989261270466a63b37e286f2892`.
`proof.json` records the executable, fixture, and screenshot SHA-256 hashes.

- `original-matches-empty.png`: the actual original CS Demo Manager LeftBar,
  routed Matches action bar, empty state, and status bar in Tauri. The
  temporary local CSDM SQLite database had no matches.
- `original-local-library.png`: the same original app shell with a separate
  DemoStrike SQLite library opened in a **ported read-only adapter**. The
  match table and Overview/Rounds/Players/Utility detail tabs in this capture
  are our adapter, **not the upstream CS Demo Manager populated Matches or
  match-detail screens**. The library contains sanitized synthetic data made
  by the CI job; the test verified its file hash was unchanged after viewing.

These captures prove native renderer startup, the original LeftBar and Matches
route, and read-only synthetic DemoStrike library inspection. They do not
prove demo import, upstream populated match screens, other route backends,
real user demo handling, video tooling, or game integration. The native
file-picker command compiled; the OS picker interaction was not exercised in
this run. No user data or private demos were uploaded.
