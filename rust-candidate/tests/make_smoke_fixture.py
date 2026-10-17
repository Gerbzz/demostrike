"""Create a small, entirely synthetic DemoStrike-schema library for hosted UI checks."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import sqlite3


def main() -> None:
    runner_temp = os.environ.get("RUNNER_TEMP")
    if not runner_temp:
        raise SystemExit("RUNNER_TEMP is required; this fixture is for hosted CI")
    output_dir = Path(runner_temp) / "demostrike-smoke"
    output_dir.mkdir(parents=True, exist_ok=True)
    db_path = output_dir / "synthetic-library.sqlite"
    if db_path.exists():
        raise SystemExit(f"Refusing to replace an existing fixture: {db_path}")

    with sqlite3.connect(db_path) as db:
        db.executescript(
            """
            PRAGMA user_version = 6;
            CREATE TABLE matches (
                id INTEGER PRIMARY KEY, file_name TEXT NOT NULL,
                file_path TEXT, source_sha256 TEXT,
                map_name TEXT NOT NULL, score_ct INTEGER NOT NULL,
                score_t INTEGER NOT NULL, imported_at TEXT NOT NULL,
                parser_capability TEXT NOT NULL, effect_attribution_version INTEGER,
                tick_rate REAL, duration_ticks INTEGER
            );
            CREATE TABLE players (match_id INTEGER, steamid TEXT, name TEXT);
            CREATE TABLE rounds (
                match_id INTEGER, round_number INTEGER, winner TEXT,
                start_tick INTEGER, end_tick INTEGER
            );
            CREATE TABLE utility_events (
                id INTEGER PRIMARY KEY, match_id INTEGER, round_number INTEGER,
                utility_type TEXT, thrower_name TEXT, thrower_team TEXT,
                throw_tick INTEGER, enemies_blinded INTEGER, damage_enemy INTEGER,
                blind_attribution_complete INTEGER NOT NULL DEFAULT 0,
                damage_attribution_complete INTEGER NOT NULL DEFAULT 0
            );
            CREATE VIEW utility_events_supported AS
            SELECT e.id, e.match_id, m.effect_attribution_version,
                   e.blind_attribution_complete, e.damage_attribution_complete,
                   CASE WHEN m.parser_capability = 'grenade-only-v1' OR m.effect_attribution_version IS NOT 2 OR e.blind_attribution_complete != 1 THEN NULL ELSE e.enemies_blinded END AS enemies_blinded,
                   CASE WHEN m.parser_capability = 'grenade-only-v1' OR m.effect_attribution_version IS NOT 2 OR e.damage_attribution_complete != 1 THEN NULL ELSE e.damage_enemy END AS damage_enemy
            FROM utility_events e JOIN matches m ON m.id = e.match_id;
            INSERT INTO matches VALUES
                (101, 'synthetic-alpha.dem', NULL, NULL, 'de_mirage', 13, 9,
                 '2026-01-02T12:00:00Z', 'synthetic-ci-only', 2, 64.0, 57600),
                (102, 'synthetic-beta.dem', NULL, NULL, 'de_inferno', 6, 13,
                 '2026-01-01T12:00:00Z', 'synthetic-ci-only', NULL, NULL, NULL);
            INSERT INTO players VALUES
                (101, 'synthetic-ct-1', 'Synthetic CT'),
                (101, 'synthetic-t-1', 'Synthetic T');
            INSERT INTO rounds VALUES
                (101, 1, 'CT', 100, 200),
                (101, 2, 'T', 201, 300),
                (101, 3, NULL, 301, 400);
            INSERT INTO utility_events VALUES
                (1, 101, 1, 'smoke', 'Synthetic CT', 'CT', 130, 0, 0, 0, 0),
                (2, 101, 2, 'flash', 'Synthetic T', 'T', 240, 2, 0, 1, 0),
                (3, 101, 3, 'molotov', 'Synthetic CT', 'CT', 330, 0, 18, 0, 1);
            """
        )
        assert db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
    digest = hashlib.sha256(db_path.read_bytes()).hexdigest()
    print(f"Synthetic fixture: {db_path}; SHA-256: {digest}")
    demo_path = output_dir / "synthetic-combat.dem"
    with demo_path.open("xb") as demo:
        demo.write(b"PBDEMS2\0synthetic public import contract fixture")
    print(f"Synthetic import input: {demo_path}; SHA-256: {hashlib.sha256(demo_path.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    main()
