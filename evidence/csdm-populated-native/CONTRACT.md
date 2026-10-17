# Populated match route: data contract and limits

The original CS Demo Manager source snapshot stays byte-for-byte intact.
The Tauri renderer uses its router, LeftBar, table engine, ActionBar, Content,
and TabLinks foundations. Ported route components read an existing
DemoStrike SQLite library with Rust `SQLITE_OPEN_READ_ONLY`. This is a
connected, data-limited migration slice, **not** the upstream PostgreSQL
`MatchTable`, scoreboard, or analyzer output.

| On-screen field | DemoStrike source | Meaning |
| --- | --- | --- |
| Demo, map | `matches.file_name`, `map_name` | Imported demo name and map. |
| CT, T | `score_ct`, `score_t` | Side scores; no Team A/B identity is inferred. |
| Rounds, players, utility | Counts of related rows | Records present in the selected library. |
| Duration | `duration_ticks / tick_rate` | Shown only when both fields are present and valid. |
| Imported | `imported_at` | Import timestamp, never labeled match date. |
| Source checksum, demo path | Optional `source_sha256`, `file_path` | Explicitly unavailable when absent. |
| Round winner and ticks | `rounds.winner`, `start_tick`, `end_tick` | Null winner/ticks display as unavailable. |
| Player identity | `players.name`, `steamid` | No individual combat stats inferred. |
| Utility effect totals | `utility_events_supported` view | Values appear only when the v6 attribution contract verifies them; null displays as unavailable. |

The nested URL uses `local-<match id>` as an explicit local-library route
key. It does not present the database ID as a CS Demo Manager match
checksum. The imported source checksum is shown separately when recorded.

The upstream populated table and scoreboard require team identities,
combat events, ratings, economy, and other data absent from this library.
Those columns and screens are not filled with zeros or estimated values.
Direct links to unsupported match analyses show an explanatory state.

Hosted CI generates only synthetic public data and checks its file hashes
after browsing. No private demo, library, names, or screenshots were uploaded
to CI or this PR. A current-build GUI run on private data is not claimed:
local builds and executable downloads are outside this task's disk constraint.

