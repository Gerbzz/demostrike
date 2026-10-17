# Native CSDM shell and read-only demo-folder evidence

[Hosted Windows run 39019533555](https://github.com/Gerbzz/DemoStrike/actions/runs/39019533555)
built PR source head `080ee0690c254de583285241940723739ba6be76`
at PR merge commit `ad2180782833c5bd111a6940973a77b41bd6b24c`.
The `upstream-renderer` and `original-native` jobs passed. The separate
candidate `verify` job is recorded in the linked run. `proof.json` records
the native executable, fixture, generated demo, and PNG hashes. All three
PNGs were downloaded from the native job artifact, hash checked, and
visually inspected. They are direct 1024×768 Tauri WebView2 app-window
captures, not mockups.

- `run31-original-matches-empty.png`: preserved original CS Demo Manager
  LeftBar, title bar, routed Matches action bar, empty state, and status bar.
- `run31-local-library.png`: preserved original shell around a separate
  **ported read-only DemoStrike SQLite adapter**. The populated match list
  and Overview/Rounds/Players/Utility tabs are **not upstream CS Demo
  Manager populated match screens**. The data is sanitized synthetic CI data.
- `run31-ported-demo-folder.png`: preserved original LeftBar and `/demos`
  route with a **ported read-only folder adapter**, showing two generated
  synthetic `.dem` files. The browser filters, searches, and refreshes direct
  demo files. It does not parse, import, analyze, delete, or play them.

The native smoke verified the SQLite fixture and both generated demo files
kept their original SHA-256 hashes after viewing. The proof file's short
`scope` sentence predates the demo-folder addition and omits this third
view; the `demoFolderScreenshotSha256`, `syntheticDemoHashes`, and
`runtimeUrl` fields cover it. The native OS folder/file chooser interaction
was not exercised. Other route backends, real demo handling, and game/video
integration remain open. No private user demo data was uploaded.
