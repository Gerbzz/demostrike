# Partial original CSDM match views

The pinned MIT CS Demo Manager renderer remains unchanged. `ported/csdm-partial-overview.tsx` and `ported/csdm-partial-rounds.tsx` adapt its MatchInformation, Scoreboard, RoundsHistory, and RoundEntry structure to the DemoStrike SQLite read contract. These are partial ports, not full CSDM Match objects.

| Original view field | Native source | Handling |
| --- | --- | --- |
| Map, file name, source and import time | `matches` | Show recorded values; map thumbnail is unavailable. |
| Team A/B names and score | No stable team identity | Show CT/T *round side* wins only; do not relabel sides as teams. |
| Player scoreboard | `players` and `player_combat` | Show identity, K/A/D and raw enemy damage only when full combat coverage is recorded. Unknown values say Unavailable. |
| Round history | `rounds` | Show round number and recorded CT/T winner. No inferred half switch. |
| Round entry | `rounds` and `round_combat` | Show recorded boundaries, end reason, equipment values and observed player counts; missing values say Unavailable. |
| Kill feed | v8 `raw_combat_deaths` | Show actual ordered death events with tick, attacker, victim, optional weapon/assist. `null` means no verified stream; `[]` means no deaths in a round of a match with a verified stream. |
| Ratings, KAST, team economy, comments, tags, playback | Not in this contract | Explicitly unavailable, never zero-filled or synthesized. |

Synthetic CI combat facts remain visibly identified. The public fixture path uses the native decoder, while the historical SQLite fixture exercises missing-field behavior. Hosted native smoke captures are actual Tauri WebView2 screenshots. Visual review must be repeated after these changes; a renderer build alone is not GUI acceptance.
