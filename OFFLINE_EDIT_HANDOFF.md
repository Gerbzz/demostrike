# Finder to offline edit handoff

Selecting a recorded Grenade Finder throw and choosing **Create offline edit plan**
asks the native host to build the existing CSthrowback capture request. That
builder reopens the imported match read-only, confirms a native parser source,
checks the event and projectile identity, verifies the actual demo bytes against
the saved SHA-256, and requires a complete pre-detonation flight path. An event
that cannot satisfy those checks produces an actionable error and no handoff.

The host creates a new local folder under its app data directory. It contains:

- `capture-request.json`: the unchanged CSthrowback v1 request for the three
  planned camera takes. The observer slot remains a candidate until a real
  recording is reviewed.
- `capture-edit-plan.json`: source ticks and editorial spans for the main POV,
  third-person reference, and projectile. The Finder's editable pause is 1.2
  seconds by default (250–5000 ms allowed). Main and third-person both hold the
  same source lineup tick for that duration at the same timeline position. Their
  release and end cues remain synchronized. The projectile begins at the
  release cue. The common timeline origin is the earliest planned take tick.
  Plan version 2 also stores each span's exact 60 fps editor frames. Timeline
  milliseconds are rounded display values; source ticks remain exact. A pause
  requested between frames is placed at the nearest frame, within 8.4 ms.
- `offline-edit.cstrike`: a native ClipStrike project with three named empty
  tracks and editable lineup, release, and detonation markers.
- `offline-edit.fcproj`: a schema-versioned FilmCraft project with an empty
  three-track sequence, the same four frame-aligned cues, and a note pointing
  to the exact source-bound plan. The initial timeline zoom keeps the cues in
  view without inventing clips. This file is produced by the FilmCraft-enabled
  Tauri host.

The plan is written last. An incomplete folder is left visible for inspection;
no existing handoff is overwritten. There are no media assets, recorded clips,
audio, capture receipts, or QC approvals in either editor project. Actual media
must still pass the existing recording, independent review, and accepted
package import boundaries before it can become an editable captured timeline.

The hosted public-demo smoke reads all four files from the real native host,
checks source binding and shared pause arithmetic, checks that both editor
projects have empty media lanes, and captures the actual WebView2 state. The
separate editor runtime acceptance opens those exact projects in the native
FilmCraft editor embedded in DemoStrike, places explicitly synthetic stills on
the seven planned spans, saves separate copies, reopens them, and captures the
actual editor window. It does not run CS2, HLAE, or a camera pilot.

The project files are editable document handoffs, not playable edits. Native
FilmCraft opens the `.fcproj` directly and converts the `.cstrike` cue document
in memory, leaving its original bytes unchanged. The standalone legacy
ClipStrike app still derives its ruler end from placed clips; the empty original
may show markers in its list but a zero-length ruler. Actual recorded media and
capture quality review remain separate gates.
