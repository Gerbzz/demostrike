"""Compare sampled native viewer coordinates with pinned CSDM CSV positions.

The upstream export is kept only on the hosted runner. This report contains
counts and coordinate differences, never source demo data or a bulk CSV copy.
"""
import csv
import json
import os
from pathlib import Path
import sqlite3


root = Path(os.environ.get("ORACLE_RUN_ROOT", os.environ["RUNNER_TEMP"]))
database = sqlite3.connect(root / "oracle-demostrike.sqlite")
database.row_factory = sqlite3.Row
rows = database.execute("""SELECT tick, steamid, x, y, z, is_alive, health, yaw FROM viewer_player_positions
                           ORDER BY tick, steamid""").fetchall()
assert len(rows) > 1000, "Native frame stream is too sparse for viewer acceptance"
stride = max(1, len(rows) // 600)
native_rows = {(row["tick"], row["steamid"]): row for row in rows[::stride]}
native = {key: tuple(row[axis] for axis in "xyz") for key, row in native_rows.items()}
assert len(native) >= 100

oracle_dir = root / "csdm-position-oracle"
position_files = list(oracle_dir.rglob("*_positions.csv"))
def header(file):
    with file.open(newline="", encoding="utf-8") as handle:
        return set(next(csv.reader(handle)))

player_files = [file for file in position_files if {"steamid", "is alive", "health"} <= header(file)]
assert len(player_files) == 1, [str(file) for file in position_files]
matched = {}
sampled_ticks = {tick for tick, _ in native}
upstream_players_at_sampled_ticks = {}
player_attribute_differences = {"alive": [], "health": [], "yaw": []}
player_attribute_samples = {"yaw": [], "yawDirectOnly": [], "yawNeither": []}
player_attribute_compared = {"alive": 0, "health": 0, "yaw": 0}
yaw_rotated_matches = 0
yaw_direct_matches = 0
yaw_viewer_matches = 0
with player_files[0].open(newline="", encoding="utf-8") as file:
    for row in csv.DictReader(file):
        key = (int(row["tick"]), row["steamid"])
        if key[0] in sampled_ticks:
            upstream_players_at_sampled_ticks.setdefault(key[0], set()).add(key[1])
        if key in native:
            expected = native[key]
            observed = tuple(float(row[axis]) for axis in "xyz")
            matched[key] = max(abs(a - b) for a, b in zip(expected, observed))
            local = native_rows[key]
            alive = row["is alive"].lower()
            if local["is_alive"] is not None and alive in ("true", "false", "1", "0"):
                player_attribute_compared["alive"] += 1
                if bool(local["is_alive"]) != (alive in ("true", "1")):
                    player_attribute_differences["alive"].append(key)
            if local["health"] is not None and row["health"]:
                player_attribute_compared["health"] += 1
                if local["health"] != int(float(row["health"])):
                    player_attribute_differences["health"].append(key)
            if local["yaw"] is not None and row["yaw"]:
                player_attribute_compared["yaw"] += 1
                original = float(row["yaw"])
                difference = abs(local["yaw"] - original) % 360
                direct = min(difference, 360 - difference) <= 0.1
                rotated = ((local["yaw"] % 360) - 180)
                rotated_difference = abs(rotated - original) % 360
                rotated_match = min(rotated_difference, 360 - rotated_difference) <= 0.1
                yaw_direct_matches += direct
                yaw_rotated_matches += rotated_match
                yaw_viewer_matches += direct if local["yaw"] == 0 else rotated_match
                if not direct:
                    player_attribute_differences["yaw"].append(key)
                    if len(player_attribute_samples["yaw"]) < 8:
                        player_attribute_samples["yaw"].append({"tick": key[0],
                            "nativeYaw": local["yaw"], "upstreamYaw": original})
                if direct and not rotated_match and len(player_attribute_samples["yawDirectOnly"]) < 8:
                    player_attribute_samples["yawDirectOnly"].append({"tick": key[0],
                        "nativeYaw": local["yaw"], "upstreamYaw": original,
                        "nativeAlive": local["is_alive"], "upstreamAlive": row["is alive"],
                        "nativeHealth": local["health"]})
                if not direct and not rotated_match and len(player_attribute_samples["yawNeither"]) < 8:
                    player_attribute_samples["yawNeither"].append({"tick": key[0],
                        "nativeYaw": local["yaw"], "upstreamYaw": original})

grenade_rows = database.execute("""SELECT round_number,tick,trajectory_index,entity_id,
                                   grenade_type,thrower_steamid,x,y,z FROM viewer_grenade_positions
                                   ORDER BY round_number,tick,trajectory_index""").fetchall()
assert grenade_rows, "Native projectile stream is absent"
grenade_ticks = {row["tick"] for row in grenade_rows[::max(1, len(grenade_rows) // 300)]}
grenade_file = [file for file in position_files if {"grenade id", "projectile id"} <= header(file)]
assert len(grenade_file) == 1, [str(file) for file in position_files]
oracle_grenades = {}
native_paths = {}
oracle_paths = {}
def kind(value):
    text = (value or "").lower()
    for name in ("smoke", "flash", "hegrenade", "he grenade", "high explosive", "molotov", "incendiary", "decoy"):
        if name in text:
            if name in ("molotov", "incendiary"):
                return "fire"
            return "he" if name in ("hegrenade", "he grenade", "high explosive") else name
    return None

for row in grenade_rows:
    key = (row["round_number"], row["trajectory_index"])
    path = native_paths.setdefault(key, {"kind": kind(row["grenade_type"]),
        "label": row["grenade_type"], "thrower": row["thrower_steamid"],
        "entity": row["entity_id"], "ticks": {}})
    path["ticks"][row["tick"]] = tuple(row[axis] for axis in "xyz")
with grenade_file[0].open(newline="", encoding="utf-8") as file:
    for row in csv.DictReader(file):
        tick = int(row["tick"])
        path_key = (int(row["round"]), row["projectile id"])
        path = oracle_paths.setdefault(path_key, {"kind": kind(row["grenade name"]),
            "label": row["grenade name"], "thrower": row["thrower steamid"], "ticks": {}})
        path["ticks"][tick] = tuple(float(row[axis]) for axis in "xyz")
        if tick in grenade_ticks:
            oracle_grenades.setdefault(tick, []).append(tuple(float(row[axis]) for axis in "xyz"))
grenade_matches = []
for row in grenade_rows:
    if row["tick"] not in oracle_grenades:
        continue
    xyz = tuple(row[axis] for axis in "xyz")
    grenade_matches.append(min(max(abs(a - b) for a, b in zip(xyz, observed))
                               for observed in oracle_grenades[row["tick"]]))

# Pair entire trajectories one-to-one. A same-tick nearest position alone can
# silently match a different projectile in a crowded utility execute.
candidate_pairs = []
for native_key, native_path in native_paths.items():
    for oracle_key, oracle_path in oracle_paths.items():
        if native_key[0] != oracle_key[0]:
            continue
        if native_path["thrower"] and oracle_path["thrower"] and native_path["thrower"] != oracle_path["thrower"]:
            continue
        if native_path["kind"] and oracle_path["kind"] and native_path["kind"] != oracle_path["kind"]:
            continue
        shared = native_path["ticks"].keys() & oracle_path["ticks"].keys()
        if not shared:
            continue
        max_delta = max(max(abs(a - b) for a, b in zip(native_path["ticks"][tick],
                          oracle_path["ticks"][tick])) for tick in shared)
        if max_delta > 16:
            continue
        candidate_pairs.append((len(shared), -max_delta, native_key, oracle_key))
candidate_pairs.sort(reverse=True)
paired_native = set()
paired_oracle = set()
path_pairs = []
for count, neg_delta, native_key, oracle_key in candidate_pairs:
    if native_key in paired_native or oracle_key in paired_oracle:
        continue
    paired_native.add(native_key)
    paired_oracle.add(oracle_key)
    native_ticks = native_paths[native_key]["ticks"]
    oracle_ticks = oracle_paths[oracle_key]["ticks"]
    releases = database.execute("""SELECT utility_type,throw_tick,detonate_tick,thrower_steamid,trajectory
        FROM utility_events WHERE projectile_entity_id=? AND thrower_steamid=?
        ORDER BY throw_tick""", (native_paths[native_key]["entity"],
                            native_paths[native_key]["thrower"])).fetchall()
    release_evidence = []
    for row in releases:
        points = json.loads(row["trajectory"])
        release_evidence.append({"utility_type": row["utility_type"],
            "throw_tick": row["throw_tick"], "detonate_tick": row["detonate_tick"],
            "thrower_steamid": row["thrower_steamid"],
            "analyzedTrajectoryEndTick": points[-1][0] if points else None})
    path_pairs.append({"sharedTicks": count, "maxAxisDelta": -neg_delta,
        "nativeTrajectory": native_key, "upstreamProjectile": oracle_key,
        "nativeEntity": native_paths[native_key]["entity"],
        "kind": native_paths[native_key]["kind"],
        "nativeLabel": native_paths[native_key]["label"],
        "upstreamLabel": oracle_paths[oracle_key]["label"],
        "nativeStartTick": min(native_ticks), "nativeEndTick": max(native_ticks),
        "upstreamStartTick": min(oracle_ticks), "upstreamEndTick": max(oracle_ticks),
        "releaseCandidates": release_evidence,
        "startTickDelta": min(native_ticks) - min(oracle_ticks),
        "endTickDelta": max(native_ticks) - max(oracle_ticks)})

unmatched_player_samples = [{"tick": tick, "steamid": steamid,
    "nativeAlive": native_rows[(tick, steamid)]["is_alive"],
    "nativeHealth": native_rows[(tick, steamid)]["health"],
    "nativeXYZ": native[(tick, steamid)],
    "upstreamPlayersAtTick": sorted(upstream_players_at_sampled_ticks.get(tick, set()))}
    for tick, steamid in sorted(set(native) - set(matched))]

report = {
    "nativePlayerRowsAllRounds": len(rows),
    "sampledPlayerKeys": len(native),
    "sharedPlayerKeys": len(matched),
    "playerMaxAxisDifference": max(matched.values(), default=None),
    "playerWithin001Units": sum(value <= 0.001 for value in matched.values()),
    "unmatchedPlayerSamples": unmatched_player_samples[:8],
    "nativeGrenadeRowsAllRounds": len(grenade_rows),
    "sharedGrenadeTicks": len(oracle_grenades),
    "grenadeComparisons": len(grenade_matches),
    "grenadeWithin16Units": sum(value <= 16 for value in grenade_matches),
    "grenadeMinAxisDifference": min(grenade_matches, default=None),
    "grenadeMaxAxisDifference": max(grenade_matches, default=None),
    "playerAttributeDiagnostics": {field: {"mismatches": len(keys), "sampleKeys": keys[:8]}
        for field, keys in player_attribute_differences.items()},
    "playerAttributeCompared": player_attribute_compared,
    "playerAttributeSamples": player_attribute_samples,
    "yawDirectMatches": yaw_direct_matches,
    "yawRotatedMatches": yaw_rotated_matches,
    "yawViewerConventionMatches": yaw_viewer_matches,
    "projectilePathIdentityDiagnostics": {
        "nativePaths": len(native_paths), "oraclePaths": len(oracle_paths),
        "oneToOnePairs": len(path_pairs),
        "sharedPositionRows": sum(pair["sharedTicks"] for pair in path_pairs),
        "maxStartTickDifference": max((abs(pair["startTickDelta"]) for pair in path_pairs), default=None),
        "maxEndTickDifference": max((abs(pair["endTickDelta"]) for pair in path_pairs), default=None),
        "lifetimeDifferenceCount": sum(pair["endTickDelta"] != 0 for pair in path_pairs),
        "lifetimeDifferenceSamples": [pair for pair in path_pairs if pair["endTickDelta"] != 0][:8],
        "fireSubtypeSamples": [pair for pair in path_pairs if pair["kind"] == "fire"],
        "unmatchedNativeSamples": list(map(str, set(native_paths) - paired_native))[:8],
        "unmatchedNativeDetails": [{"round": key[0], "trajectory": key[1],
            "kind": native_paths[key]["kind"], "thrower": native_paths[key]["thrower"],
            "startTick": min(native_paths[key]["ticks"]),
            "endTick": max(native_paths[key]["ticks"]),
            "rows": len(native_paths[key]["ticks"])}
            for key in sorted(set(native_paths) - paired_native)[:8]],
        "unmatchedOracleDetails": [{"round": key[0], "projectile": key[1],
            "kind": oracle_paths[key]["kind"], "thrower": oracle_paths[key]["thrower"],
            "startTick": min(oracle_paths[key]["ticks"]),
            "endTick": max(oracle_paths[key]["ticks"]),
            "rows": len(oracle_paths[key]["ticks"])}
            for key in sorted(set(oracle_paths) - paired_oracle)[:8]],
    },
}
(root / "csdm-position-comparison.json").write_text(json.dumps(report, indent=2) + "\n")
print("CSDM_POSITION_COMPARISON", json.dumps(report))
# The pinned CSDM exporter iterates Participants().Playing(), whereas the
# native stream observes all valid controllers with pawns. Ancient has two
# sampled observations of one dead, zero-HP controller absent from CSDM's
# playing set. Do not delete those source observations or hide the difference.
if root.name == "ancient":
    assert len(unmatched_player_samples) == 2 and all(
        sample["nativeAlive"] == 0 and sample["nativeHealth"] == 0
        and sample["steamid"] not in sample["upstreamPlayersAtTick"]
        for sample in unmatched_player_samples
    ), "Ancient player-set difference is not the reviewed dead-controller case"
else:
    assert not unmatched_player_samples, "Native player tick/identity samples missing from CSDM"
assert report["playerWithin001Units"] == len(matched), "Shared player world coordinates disagree with CSDM"
assert len(grenade_matches) >= 5, "Too few overlapping native and CSDM projectile ticks"
assert report["grenadeWithin16Units"] == len(grenade_matches), "Projectile world coordinates disagree with CSDM"
assert player_attribute_compared["alive"] > 0 and not player_attribute_differences["alive"], "Observed alive state disagrees with CSDM"
assert player_attribute_compared["health"] > 0 and not player_attribute_differences["health"], "Observed health disagrees with CSDM"
assert yaw_viewer_matches == player_attribute_compared["yaw"], "Viewer yaw convention disagrees with CSDM"
assert len(path_pairs) == len(native_paths) == len(oracle_paths), "Projectile identities do not pair one-to-one"
assert sum(pair["sharedTicks"] for pair in path_pairs) == len(grenade_rows), "Projectile trajectory samples do not pair one-to-one"
assert all(pair["startTickDelta"] == 0 for pair in path_pairs), "Projectile release/start ticks disagree with CSDM"
assert all(pair["endTickDelta"] == 0 for pair in path_pairs), "Projectile lifetime/end ticks disagree with CSDM"
fire_paths = [pair for pair in path_pairs if pair["kind"] == "fire"]
assert fire_paths, "No observed fire paths to verify subtype mapping"
for pair in fire_paths:
    releases = [event for event in pair["releaseCandidates"]
        if event["throw_tick"] <= pair["nativeStartTick"] <= event["throw_tick"] + 32
        and (event["detonate_tick"] is None or
            pair["nativeEndTick"] <= event["detonate_tick"] <= pair["nativeEndTick"] + 16)]
    assert len(releases) == 1, "Fire trajectory lacks one observed subtype release"
    assert releases[0]["utility_type"] in pair["upstreamLabel"].lower(), \
        "Observed fire release subtype disagrees with pinned CSDM weapon instance"
