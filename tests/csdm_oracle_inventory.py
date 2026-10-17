"""Inventory a pinned CSDM analyzer export beside DemoStrike's public import.

This discovery step does not approve or regenerate expected values. A later
comparison must select shared facts and review every mismatch explicitly.
"""

import json
import os
from pathlib import Path
import sqlite3
from collections import Counter


root = Path(os.environ.get("ORACLE_RUN_ROOT") or os.environ["RUNNER_TEMP"])
exports = list((root / "csdm-oracle").rglob("*.json"))
if len(exports) != 1:
    raise RuntimeError(f"Expected one analyzer JSON export, found {len(exports)}")
oracle_file = exports[0]
if oracle_file.stat().st_size > 20_000_000:
    raise RuntimeError("Analyzer JSON exceeded the 20 MB discovery cap")
oracle = json.loads(oracle_file.read_text(encoding="utf-8"))


def shape(value):
    if isinstance(value, dict):
        return {"type": "object", "keys": sorted(value), "fields": {
            key: shape(child) for key, child in value.items()
            if isinstance(child, (list, dict))
        }}
    if isinstance(value, list):
        return {"type": "array", "length": len(value), "first": shape(value[0]) if value else None}
    return {"type": type(value).__name__}


database = root / "oracle-demostrike.sqlite"
with sqlite3.connect(f"file:{database.as_posix()}?mode=ro", uri=True) as connection:
    counts = {
        table: connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
        for table in (
            "matches", "rounds", "players", "player_round_combat",
            "raw_combat_hurts", "raw_combat_deaths", "utility_events",
        )
    }
    rounds = [
        dict(zip(("number", "winner", "start", "freezeEnd", "end", "reason", "teamASide", "teamBSide"), row))
        for row in connection.execute(
            "SELECT round_number, winner, start_tick, freeze_end_tick, end_tick, end_reason, team_a_side, team_b_side "
            "FROM rounds ORDER BY round_number"
        )
    ]
    native_hurts = [
        dict(zip(("roundNumber", "tick", "attackerSteamId", "victimSteamId", "healthDamage", "effectiveHealthDamage"), row))
        for row in connection.execute(
            "SELECT round_number, tick, attacker_steamid, victim_steamid, raw_dmg_health, effective_health_damage "
            "FROM raw_combat_hurts ORDER BY source_index"
        )
    ]
    native_deaths = [
        dict(zip(("roundNumber", "tick", "killerSteamId", "victimSteamId", "assisterSteamId"), row))
        for row in connection.execute(
            "SELECT round_number, tick, attacker_steamid, victim_steamid, assister_steamid "
            "FROM raw_combat_deaths ORDER BY source_index"
        )
    ]
    native_bombs = [
        {"kind": kind, "roundNumber": number, "tick": tick,
         "actorSteamId": steamid, "siteId": site_id, "x": x, "y": y, "z": z}
        for kind, number, tick, steamid, site_id, x, y, z in connection.execute(
            "SELECT kind,round_number,tick,actor_steamid,site_id,x,y,z "
            "FROM viewer_bomb_events ORDER BY tick,source_index"
        )
    ]
    native_detonations = [
        {"kind": kind, "roundNumber": number, "tick": tick,
         "x": x, "y": y, "z": z}
        for kind, number, tick, x, y, z in connection.execute(
            "SELECT utility_type,round_number,detonate_tick,detonate_x,detonate_y,detonate_z "
            "FROM utility_events WHERE detonate_tick IS NOT NULL "
            "ORDER BY detonate_tick,id"
        )
    ]
    native_teams = {}
    for steamid, team in connection.execute(
        "SELECT steamid, stable_team FROM player_round_combat ORDER BY steamid, round_number"
    ):
        native_teams.setdefault(steamid, set()).add(team)
    native_players = {
        steamid: dict(zip(("enemyKills", "enemyAssists", "deaths", "enemyHealthDamage", "effectiveHealthDamage", "unknownEffectiveRows"), row[1:]))
        for row in connection.execute(
            "SELECT steamid, SUM(enemy_kills), SUM(enemy_assists), SUM(deaths), "
            "SUM(enemy_health_damage), SUM(effective_enemy_health_damage), "
            "SUM(CASE WHEN effective_enemy_health_damage IS NULL THEN 1 ELSE 0 END) "
            "FROM player_round_combat GROUP BY steamid"
        ) for steamid in [row[0]]
    }


def key(event, fields):
    # The original and SQLite may encode Steam IDs as number/text, but an
    # absent fact, explicit null and numeric zero are different evidence.
    return tuple("MISSING" if field not in event else
                 "NULL" if event[field] is None else
                 f"VALUE:{event[field]}" for field in fields)


assert key({"damage": None}, ("damage",)) != key({"damage": 0}, ("damage",))
assert key({}, ("damage",)) != key({"damage": None}, ("damage",))


def event_differences(oracle_events, native_events, fields):
    upstream = Counter(key(event, fields) for event in oracle_events)
    native = Counter(key(event, fields) for event in native_events)
    return {
        "upstreamCount": len(oracle_events),
        "nativeCount": len(native_events),
        "upstreamOnlyCount": sum((upstream - native).values()),
        "nativeOnlyCount": sum((native - upstream).values()),
        "upstreamOnlySamples": list((upstream - native).elements())[:12],
        "nativeOnlySamples": list((native - upstream).elements())[:12],
        "fields": fields,
    }


round_fields = (
    ("number", "number"), ("startTick", "start"), ("endTick", "end"),
)
round_comparison = []
for upstream, native in zip(oracle.get("rounds", []), rounds):
    differences = {
        original: {"upstream": upstream.get(original), "native": native.get(local)}
        for original, local in round_fields
        if str(upstream.get(original)) != str(native.get(local))
    }
    native_freeze = native["freezeEnd"]
    upstream_freeze = upstream.get("freezeTimeEndTick")
    # CSDM fills the final surrendered/end-of-demo round's absent freeze
    # boundary with its end tick. Preserve the native absence explicitly.
    if native_freeze is None:
        if upstream_freeze != upstream.get("endTick"):
            differences["freezeTimeEndTick"] = {"upstream": upstream_freeze, "native": native_freeze}
    elif upstream_freeze != native_freeze:
        differences["freezeTimeEndTick"] = {"upstream": upstream_freeze, "native": native_freeze}
    if differences:
        round_comparison.append({"round": native["number"], "differences": differences})

# Original analyzer exports Source 2 enum numbers. Keep the mapping explicit,
# sourced from the pinned analyzer and GameRules round outcome contract.
side_names = {2: "T", 3: "CT"}
reason_names = {1: "bomb_exploded", 7: "bomb_defused", 8: "t_killed",
                9: "ct_killed", 12: "time_ran_out", 17: "t_surrender",
                18: "ct_surrender"}
for upstream, native in zip(oracle.get("rounds", []), rounds):
    differences = {}
    for source, target, mapping in (("winnerSide", "winner", side_names),
                                    ("endReason", "reason", reason_names),
                                    ("teamASide", "teamASide", side_names),
                                    ("teamBSide", "teamBSide", side_names)):
        translated = mapping.get(upstream.get(source), "unmapped")
        if translated != native.get(target):
            differences[source] = {"upstream": upstream.get(source),
                                   "mapped": translated, "native": native.get(target)}
    if differences:
        item = next((item for item in round_comparison if item["round"] == native["number"]), None)
        if item is None:
            round_comparison.append({"round": native["number"], "differences": differences})
        else:
            item["differences"].update(differences)

damage_fields = ("roundNumber", "tick", "attackerSteamId", "victimSteamId")
# CSDM's pinned Go Damage struct starts AttackerSteamID64 at numeric zero for
# world damage; the native ledger uses SQL NULL. Normalize only this documented
# absent-identity sentinel, keeping all ordinary zeros distinct from NULL.
def normalize_csdm_damage(damage):
    if damage.get("attackerSteamId") == 0:
        return dict(damage, attackerSteamId=None)
    return damage


oracle_damages = [normalize_csdm_damage(damage) for damage in oracle.get("damages", [])]
assert normalize_csdm_damage({"attackerSteamId": 0})["attackerSteamId"] is None
assert normalize_csdm_damage({"attackerSteamId": 42})["attackerSteamId"] == 42
upstream_damage = {}
native_damage = {}
for target, events in ((upstream_damage, oracle_damages),
                       (native_damage, native_hurts)):
    for event in events:
        target.setdefault(key(event, damage_fields), []).append(event.get("healthDamage"))
damage_amount_differences = [
    {"event": identity, "upstreamEffective": sorted(upstream_damage[identity]),
     "nativeRaw": sorted(native_damage[identity])}
    for identity in upstream_damage.keys() & native_damage.keys()
    if sorted(upstream_damage[identity]) != sorted(native_damage[identity])
]
damage_amount_differences.sort(key=lambda item: item["event"])

# The native v8 ledger intentionally retains all raw deaths. Its displayed
# player-round totals now assign inter-round deaths to the preceding completed
# round and omit post-match events; compare that equivalent match scope.
def match_round(event):
    round_number = event["roundNumber"]
    current = next((item for item in rounds if item["number"] == round_number), None)
    if current is None:
        return None
    if current["start"] is not None and event["tick"] < current["start"]:
        earlier = [item for item in rounds if item["number"] < round_number
                   and item["end"] is not None and item["end"] <= event["tick"]]
        return max(earlier, key=lambda item: item["end"])["number"] if earlier else None
    return round_number

in_match_deaths = [dict(event, roundNumber=match_round(event))
                   for event in native_deaths if match_round(event) is not None]
in_match_hurts = [dict(event, roundNumber=match_round(event))
                  for event in native_hurts if match_round(event) is not None]
in_match_effective = [dict(event, healthDamage=event["effectiveHealthDamage"])
                      for event in in_match_hurts]

# The pinned Go Kill struct uses uint64 killer/assister fields initialized to
# zero when no player exists; DemoStrike stores that absence as SQL NULL. This is
# a source-specific, reviewed mapping, never a blanket zero/null coercion.
def normalize_csdm_kill(kill):
    normalized = dict(kill)
    for optional_id in ("killerSteamId", "assisterSteamId"):
        if normalized.get(optional_id) == 0:
            normalized[optional_id] = None
    return normalized


assert normalize_csdm_kill({"assisterSteamId": 0})["assisterSteamId"] is None
assert normalize_csdm_kill({"assisterSteamId": 42})["assisterSteamId"] == 42

player_comparison = []
for steamid, upstream in sorted(oracle.get("players", {}).items()):
    native = native_players.get(steamid, {})
    player_comparison.append({
        "steamId": steamid,
        "name": upstream.get("name"),
        "upstream": {name: upstream.get(name) for name in ("killCount", "assistCount", "deathCount", "healthDamage")},
        "native": native,
        "upstreamTeam": (upstream.get("team") or {}).get("letter"),
        "nativeTeams": sorted(native_teams.get(steamid, set()), key=lambda value: value or ""),
    })

comparison = {
    "status": "pinned independent comparison with exact shared-fact regression gates",
    "roundFieldDifferences": round_comparison,
    "roundTimeline": [
        {"number": native["number"], "upstreamStart": upstream.get("startTick"),
         "nativeStart": native["start"], "nativeFreezeEnd": native["freezeEnd"],
         "upstreamEnd": upstream.get("endTick"),
         "nativeEnd": native["end"], "upstreamFreezeEnd": upstream.get("freezeTimeEndTick"),
         "upstreamTeamASide": upstream.get("teamASide"),
         "upstreamTeamBSide": upstream.get("teamBSide")}
        for upstream, native in zip(oracle.get("rounds", []), rounds)
    ],
    "damageEventIdentityWithinMatchWithoutAmount": event_differences(
        oracle_damages, in_match_hurts,
        damage_fields,
    ),
    "damageAmountSemantics": {"upstream": "effective health removed",
                              "native": "raw dmg_health including overkill",
                              "differentEventCount": len(damage_amount_differences),
                              "nativeHigherEventCount": sum(sum(item["nativeRaw"]) > sum(item["upstreamEffective"])
                                                            for item in damage_amount_differences),
                              "nativeLowerEventCount": sum(sum(item["nativeRaw"]) < sum(item["upstreamEffective"])
                                                           for item in damage_amount_differences),
                              "samples": damage_amount_differences[:12]},
    "effectiveDamageEventAmounts": event_differences(
        oracle_damages, in_match_effective,
        damage_fields + ("healthDamage",),
    ),
    "killVsInMatchDeathIdentity": event_differences(
        [normalize_csdm_kill(kill) for kill in oracle.get("kills", [])], in_match_deaths,
        ("roundNumber", "tick", "killerSteamId", "victimSteamId", "assisterSteamId"),
    ),
    "retainedPostMatchRawDeaths": len(native_deaths) - len(in_match_deaths),
    "playerTotals": player_comparison,
    "bombTickRoundDiscovery": {
        kind: event_differences(
            oracle.get(source) or [],
            [event for event in native_bombs if event["kind"] == kind],
            ("roundNumber", "tick"),
        )
        for kind, source in (
            ("bomb_planted", "bombsPlanted"),
            ("bomb_defused", "bombsDefused"),
            ("bomb_exploded", "bombsExploded"),
        )
    },
    "grenadeDetonationTickRoundDiscovery": {
        kind: event_differences(
            oracle.get(source) or [],
            [event for event in native_detonations if event["kind"] == kind],
            ("roundNumber", "tick"),
        )
        for kind, source in (
            ("smoke", "smokesStart"),
            ("flash", "flashbangsExplode"),
            ("he", "heGrenadesExplode"),
            ("decoy", "decoysStart"),
        )
    },
}

inventory = {
    "status": "pinned fixture inventory; no expected values regenerated",
    "oracleSource": "akiver/cs-demo-analyzer@61a1208e71a00255846a7e0e829405052c11c7cd (v1.11.0)",
    "oracleExportBytes": oracle_file.stat().st_size,
    "oracleShape": shape(oracle),
    "demoStrikeCounts": counts,
    "demoStrikeRounds": rounds,
}
output = root / "csdm-oracle-inventory.json"
output.write_text(json.dumps(inventory, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
comparison_output = root / "csdm-oracle-comparison.json"
comparison_output.write_text(json.dumps(comparison, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
print(json.dumps({"inventory": {
    "upstreamSource": oracle.get("source"),
    "upstreamType": oracle.get("type"),
    "upstreamRoundCount": len(oracle.get("rounds", [])),
    "upstreamKillCount": len(oracle.get("kills", [])),
    "upstreamDamageCount": len(oracle.get("damages", [])),
    "demoStrikeCounts": counts,
}, "comparison": comparison}, indent=2, ensure_ascii=False))

# Keep the source bytes and analyzer revision pinned in the workflow. Assert
# only facts both implementations actually expose with the same semantics.
# Raw dmg_health versus effective HP damage and the final absent freeze tick
# remain explicit differences, never coerced into apparent parity.
fixture = "Ancient" if root.name == "ancient" else "Mirage"
expected = {
    "Mirage": {"rounds": 10, "kills": 64, "damages": 264, "first": 67},
    "Ancient": {"rounds": 8, "kills": 43, "damages": 173, "first": 66,
                "timeoutStarts": {4: 22327, 5: 29901, 6: 41797}},
}[fixture]
failures = []
if oracle.get("source") != "valve" or oracle.get("type") != "GOTV":
    failures.append(f"Unexpected original analyzer source/type: {oracle.get('source')}/{oracle.get('type')}")
if len(oracle.get("rounds", [])) != expected["rounds"] or len(rounds) != expected["rounds"]:
    failures.append(f"Round count changed: upstream={len(oracle.get('rounds', []))} native={len(rounds)}")
if len(oracle.get("kills", [])) != expected["kills"] or len(oracle.get("damages", [])) != expected["damages"]:
    failures.append("Pinned original kill/damage counts changed")
if round_comparison:
    failures.append(f"Shared round fields differ: {round_comparison}")
if rounds and rounds[0]["start"] != expected["first"]:
    failures.append(f"First round starts at {rounds[0]['start']}, expected {expected['first']}")
for number, start in expected.get("timeoutStarts", {}).items():
    if next((row["start"] for row in rounds if row["number"] == number), None) != start:
        failures.append(f"Timeout round {number} start no longer equals observed zero tick {start}")
for field in ("killVsInMatchDeathIdentity", "damageEventIdentityWithinMatchWithoutAmount",
              "effectiveDamageEventAmounts"):
    result = comparison[field]
    if result["upstreamCount"] != result["nativeCount"] or result["upstreamOnlyCount"] or result["nativeOnlyCount"]:
        failures.append(f"{field} differs: {result}")
for result in comparison["bombTickRoundDiscovery"].values():
    if result["upstreamCount"] != result["nativeCount"] or result["upstreamOnlyCount"] or result["nativeOnlyCount"]:
        failures.append(f"Observed bomb tick/round identity differs: {result}")
for kind in ("smoke", "he", "decoy"):
    result = comparison["grenadeDetonationTickRoundDiscovery"][kind]
    if result["upstreamCount"] != result["nativeCount"] or result["upstreamOnlyCount"] or result["nativeOnlyCount"]:
        failures.append(f"{kind} detonation tick/round identity differs: {result}")
# The pinned analyzer emits two FlashExplode records per observed native
# detonation on both public fixtures. Preserve its multiplicity as an explicit
# source difference while checking every distinct tick/round identity.
flash_fields = ("roundNumber", "tick")
upstream_flashes = Counter(key(event, flash_fields) for event in oracle.get("flashbangsExplode") or [])
native_flashes = Counter(key(event, flash_fields) for event in native_detonations if event["kind"] == "flash")
if set(upstream_flashes) != set(native_flashes) or any(
    upstream_flashes[identity] != 2 * count for identity, count in native_flashes.items()
):
    failures.append("Flash detonation identities or pinned analyzer 2x export multiplicity changed")
if len(player_comparison) != 10 or set(native_players) != set(oracle.get("players", {})):
    failures.append("Pinned player identity set changed")
for player in player_comparison:
    upstream, native = player["upstream"], player["native"]
    if native.get("unknownEffectiveRows"):
        failures.append(f"{player['steamId']} has unavailable effective damage rows")
    if player["nativeTeams"] != [player["upstreamTeam"]]:
        failures.append(f"{player['steamId']} stable team: upstream={player['upstreamTeam']} native={player['nativeTeams']}")
    for source, target in (("killCount", "enemyKills"), ("assistCount", "enemyAssists"),
                           ("deathCount", "deaths"), ("healthDamage", "effectiveHealthDamage")):
        if upstream[source] != native.get(target):
            failures.append(f"{player['steamId']} {source}: upstream={upstream[source]} native={native.get(target)}")
if failures:
    raise AssertionError(f"{fixture} independent analyzer regression failed:\\n" + "\\n".join(failures))
print(f"PASS: {fixture} pinned analyzer agrees on shared rounds/team sides, event identities/effective HP, and ten player K/A/D/effective damage totals")
