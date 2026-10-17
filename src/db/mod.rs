//! SQLite persistence. Versioned migrations via PRAGMA user_version.

pub mod queries;

use crate::error::{AppError, AppResult};
use rusqlite::Connection;
use std::path::Path;

const MIGRATIONS: &[&str] = &[
    // v1 — initial schema
    r#"
    CREATE TABLE matches (
        id INTEGER PRIMARY KEY,
        file_path TEXT NOT NULL UNIQUE,
        file_name TEXT NOT NULL,
        map_name TEXT NOT NULL,
        tick_rate REAL NOT NULL,
        duration_ticks INTEGER NOT NULL DEFAULT 0,
        rounds_count INTEGER NOT NULL DEFAULT 0,
        score_ct INTEGER NOT NULL DEFAULT 0,
        score_t INTEGER NOT NULL DEFAULT 0,
        imported_at TEXT NOT NULL
    );

    CREATE TABLE players (
        match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
        steamid TEXT NOT NULL,
        name TEXT NOT NULL,
        PRIMARY KEY (match_id, steamid)
    );

    CREATE TABLE rounds (
        match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
        round_number INTEGER NOT NULL,
        start_tick INTEGER,
        freeze_end_tick INTEGER,
        end_tick INTEGER,
        winner TEXT,
        end_reason TEXT,
        ct_equip_value INTEGER,
        t_equip_value INTEGER,
        PRIMARY KEY (match_id, round_number)
    );

    CREATE TABLE utility_events (
        id INTEGER PRIMARY KEY,
        match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
        round_number INTEGER NOT NULL,
        utility_type TEXT NOT NULL,
        thrower_steamid TEXT NOT NULL,
        thrower_name TEXT NOT NULL,
        thrower_team TEXT NOT NULL,
        throw_tick INTEGER NOT NULL,
        throw_time REAL NOT NULL DEFAULT 0,
        throw_x REAL NOT NULL, throw_y REAL NOT NULL, throw_z REAL NOT NULL,
        detonate_tick INTEGER,
        detonate_x REAL, detonate_y REAL, detonate_z REAL,
        land_x REAL, land_y REAL, land_z REAL,
        trajectory TEXT NOT NULL DEFAULT '[]',
        bounces TEXT NOT NULL DEFAULT '[]',
        blind_enemy_duration REAL NOT NULL DEFAULT 0,
        blind_team_duration REAL NOT NULL DEFAULT 0,
        enemies_blinded INTEGER NOT NULL DEFAULT 0,
        teammates_blinded INTEGER NOT NULL DEFAULT 0,
        damage_enemy INTEGER NOT NULL DEFAULT 0,
        damage_team INTEGER NOT NULL DEFAULT 0,
        kills INTEGER NOT NULL DEFAULT 0,
        flash_assists INTEGER NOT NULL DEFAULT 0,
        nearby_players TEXT NOT NULL DEFAULT '[]',
        round_winner TEXT,
        favorite INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX idx_events_match ON utility_events(match_id);
    CREATE INDEX idx_events_type ON utility_events(utility_type);
    CREATE INDEX idx_events_thrower ON utility_events(thrower_steamid);
    CREATE INDEX idx_events_round ON utility_events(round_number);
    CREATE INDEX idx_events_favorite ON utility_events(favorite);

    CREATE TABLE tags (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL UNIQUE COLLATE NOCASE
    );
    CREATE TABLE event_tags (
        event_id INTEGER NOT NULL REFERENCES utility_events(id) ON DELETE CASCADE,
        tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
        PRIMARY KEY (event_id, tag_id)
    );

    CREATE TABLE collections (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE TABLE collection_events (
        collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
        event_id INTEGER NOT NULL REFERENCES utility_events(id) ON DELETE CASCADE,
        position INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (collection_id, event_id)
    );

    CREATE TABLE render_jobs (
        id INTEGER PRIMARY KEY,
        status TEXT NOT NULL DEFAULT 'queued',
        progress REAL NOT NULL DEFAULT 0,
        settings TEXT NOT NULL,
        output_dir TEXT NOT NULL,
        error TEXT,
        created_at TEXT NOT NULL
    );
    CREATE TABLE render_job_events (
        job_id INTEGER NOT NULL REFERENCES render_jobs(id) ON DELETE CASCADE,
        event_id INTEGER NOT NULL REFERENCES utility_events(id) ON DELETE CASCADE,
        position INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (job_id, event_id)
    );
    "#,
    // v2 — real CS2 POV capture needs the spectator slot from the demo.
    r#"
    ALTER TABLE utility_events ADD COLUMN observer_slot INTEGER;
    "#,
    // v3 - a grenade-only import must never masquerade as full combat/economy facts.
    r#"
    ALTER TABLE matches ADD COLUMN parser_capability TEXT NOT NULL DEFAULT 'reference-raw-facts';
    "#,
    // v4 - source bytes identify copies at different paths without replacing annotations.
    // Legacy imports have NULL until explicitly reimported; their paths remain intact.
    r#"
    ALTER TABLE matches ADD COLUMN source_sha256 TEXT;
    CREATE UNIQUE INDEX matches_source_sha256_unique ON matches(source_sha256)
        WHERE source_sha256 IS NOT NULL;
    "#,
    // v5 - public SQL read surface: unavailable native combat and flash facts
    // are NULL, including rows written by older builds with storage defaults.
    r#"
    CREATE VIEW utility_events_supported AS
    SELECT e.id, e.match_id, m.parser_capability, e.round_number, e.utility_type,
           e.thrower_steamid, e.thrower_name, e.thrower_team, e.observer_slot,
           e.throw_tick, e.throw_time, e.throw_x, e.throw_y, e.throw_z,
           e.detonate_tick, e.detonate_x, e.detonate_y, e.detonate_z,
           e.land_x, e.land_y, e.land_z, e.trajectory, e.bounces,
           CASE WHEN m.parser_capability = 'grenade-only-v1' THEN NULL ELSE e.blind_enemy_duration END AS blind_enemy_duration,
           CASE WHEN m.parser_capability = 'grenade-only-v1' THEN NULL ELSE e.blind_team_duration END AS blind_team_duration,
           CASE WHEN m.parser_capability = 'grenade-only-v1' THEN NULL ELSE e.enemies_blinded END AS enemies_blinded,
           CASE WHEN m.parser_capability = 'grenade-only-v1' THEN NULL ELSE e.teammates_blinded END AS teammates_blinded,
           CASE WHEN m.parser_capability = 'grenade-only-v1' THEN NULL ELSE e.damage_enemy END AS damage_enemy,
           CASE WHEN m.parser_capability = 'grenade-only-v1' THEN NULL ELSE e.damage_team END AS damage_team,
           CASE WHEN m.parser_capability = 'grenade-only-v1' THEN NULL ELSE e.kills END AS kills,
           CASE WHEN m.parser_capability = 'grenade-only-v1' THEN NULL ELSE e.flash_assists END AS flash_assists,
           e.nearby_players, e.round_winner, e.favorite
    FROM utility_events e JOIN matches m ON m.id = e.match_id;
    "#,
    // v6 - old independently attributed totals are not trustworthy. Keep
    // storage and annotations, but mask them until a versioned global pass
    // records complete or ambiguous attribution per metric category.
    r#"
    ALTER TABLE matches ADD COLUMN effect_attribution_version INTEGER;
    ALTER TABLE utility_events ADD COLUMN projectile_entity_id INTEGER;
    ALTER TABLE utility_events ADD COLUMN blind_attribution_complete INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE utility_events ADD COLUMN damage_attribution_complete INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE utility_events ADD COLUMN kill_attribution_complete INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE utility_events ADD COLUMN assist_attribution_complete INTEGER NOT NULL DEFAULT 0;
    CREATE TABLE raw_effect_facts (
        id INTEGER PRIMARY KEY,
        match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
        fact_kind TEXT NOT NULL,
        source_index INTEGER NOT NULL,
        raw_json TEXT NOT NULL,
        resolution TEXT NOT NULL CHECK (resolution IN ('matched','ambiguous','unresolved')),
        assigned_event_id INTEGER REFERENCES utility_events(id) ON DELETE SET NULL,
        candidate_event_ids TEXT NOT NULL,
        UNIQUE(match_id, fact_kind, source_index)
    );
    CREATE INDEX raw_effect_facts_match_resolution ON raw_effect_facts(match_id, resolution);
    DROP VIEW utility_events_supported;
    CREATE VIEW utility_events_supported AS
    SELECT e.id, e.match_id, m.parser_capability, m.effect_attribution_version,
           e.round_number, e.utility_type, e.thrower_steamid, e.thrower_name, e.thrower_team,
           e.observer_slot, e.throw_tick, e.throw_time, e.throw_x, e.throw_y, e.throw_z,
           e.detonate_tick, e.detonate_x, e.detonate_y, e.detonate_z,
           e.land_x, e.land_y, e.land_z, e.trajectory, e.bounces, e.projectile_entity_id,
           e.blind_attribution_complete, e.damage_attribution_complete,
           e.kill_attribution_complete, e.assist_attribution_complete,
           CASE WHEN m.parser_capability = 'grenade-only-v1' OR m.effect_attribution_version IS NOT 2 OR e.blind_attribution_complete != 1 THEN NULL ELSE e.blind_enemy_duration END AS blind_enemy_duration,
           CASE WHEN m.parser_capability = 'grenade-only-v1' OR m.effect_attribution_version IS NOT 2 OR e.blind_attribution_complete != 1 THEN NULL ELSE e.blind_team_duration END AS blind_team_duration,
           CASE WHEN m.parser_capability = 'grenade-only-v1' OR m.effect_attribution_version IS NOT 2 OR e.blind_attribution_complete != 1 THEN NULL ELSE e.enemies_blinded END AS enemies_blinded,
           CASE WHEN m.parser_capability = 'grenade-only-v1' OR m.effect_attribution_version IS NOT 2 OR e.blind_attribution_complete != 1 THEN NULL ELSE e.teammates_blinded END AS teammates_blinded,
           CASE WHEN m.parser_capability = 'grenade-only-v1' OR m.effect_attribution_version IS NOT 2 OR e.damage_attribution_complete != 1 THEN NULL ELSE e.damage_enemy END AS damage_enemy,
           CASE WHEN m.parser_capability = 'grenade-only-v1' OR m.effect_attribution_version IS NOT 2 OR e.damage_attribution_complete != 1 THEN NULL ELSE e.damage_team END AS damage_team,
           CASE WHEN m.parser_capability = 'grenade-only-v1' OR m.effect_attribution_version IS NOT 2 OR e.kill_attribution_complete != 1 THEN NULL ELSE e.kills END AS kills,
           CASE WHEN m.parser_capability = 'grenade-only-v1' OR m.effect_attribution_version IS NOT 2 OR e.assist_attribution_complete != 1 THEN NULL ELSE e.flash_assists END AS flash_assists,
           e.nearby_players, e.round_winner, e.favorite
    FROM utility_events e JOIN matches m ON m.id = e.match_id;
    "#,
    // v7 - complete native combat is independent of utility attribution.
    // Nullable metrics preserve unknown side attribution. Older imported
    // matches remain unchanged and cannot claim this coverage version.
    r#"
    CREATE TABLE player_round_combat (
        match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
        round_number INTEGER NOT NULL,
        steamid TEXT NOT NULL,
        side TEXT CHECK (side IN ('CT','T') OR side IS NULL),
        enemy_kills INTEGER,
        deaths INTEGER NOT NULL,
        enemy_assists INTEGER,
        enemy_health_damage INTEGER,
        PRIMARY KEY (match_id, round_number, steamid)
    );
    CREATE INDEX player_round_combat_player ON player_round_combat(match_id, steamid);
    "#,
    // v8 - keep event-level combat for future original CSDM workflows.
    // Additive tables leave prior matches, utility annotations, and render
    // references untouched; old imports do not acquire invented event rows.
    r#"
    CREATE TABLE raw_combat_hurts (
        match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
        source_index INTEGER NOT NULL,
        event_order INTEGER,
        tick INTEGER NOT NULL,
        round_number INTEGER NOT NULL,
        weapon TEXT,
        attacker_steamid TEXT,
        attacker_side TEXT,
        victim_steamid TEXT NOT NULL,
        victim_side TEXT,
        raw_dmg_health INTEGER NOT NULL,
        PRIMARY KEY (match_id, source_index)
    );
    CREATE INDEX raw_combat_hurts_timeline ON raw_combat_hurts(match_id, tick, event_order);
    CREATE TABLE raw_combat_deaths (
        match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
        source_index INTEGER NOT NULL,
        event_order INTEGER,
        tick INTEGER NOT NULL,
        round_number INTEGER NOT NULL,
        weapon TEXT,
        attacker_steamid TEXT,
        attacker_side TEXT,
        victim_steamid TEXT NOT NULL,
        victim_side TEXT,
        assister_steamid TEXT,
        assister_side TEXT,
        PRIMARY KEY (match_id, source_index)
    );
    CREATE INDEX raw_combat_deaths_timeline ON raw_combat_deaths(match_id, tick, event_order);
    "#,
    // v9 - preserve effective HP alongside the raw source value. Existing
    // imports remain unknown; migration never fabricates health or team facts.
    r#"
    ALTER TABLE raw_combat_hurts ADD COLUMN post_health INTEGER;
    ALTER TABLE raw_combat_hurts ADD COLUMN start_health INTEGER;
    ALTER TABLE raw_combat_hurts ADD COLUMN effective_health_damage INTEGER
        CHECK (effective_health_damage >= 0 OR effective_health_damage IS NULL);
    ALTER TABLE player_round_combat ADD COLUMN effective_enemy_health_damage INTEGER
        CHECK (effective_enemy_health_damage >= 0 OR effective_enemy_health_damage IS NULL);
    ALTER TABLE rounds ADD COLUMN team_a_side TEXT CHECK (team_a_side IN ('CT','T') OR team_a_side IS NULL);
    ALTER TABLE rounds ADD COLUMN team_b_side TEXT CHECK (team_b_side IN ('CT','T') OR team_b_side IS NULL);
    ALTER TABLE player_round_combat ADD COLUMN stable_team TEXT
        CHECK (stable_team IN ('A','B') OR stable_team IS NULL);
    "#,
    // v10 - absolute-tick frame samples for the bounded native 2D viewer.
    // Older rows remain unavailable; migration never infers a trajectory.
    r#"
    ALTER TABLE matches ADD COLUMN viewer_position_version INTEGER;
    CREATE TABLE viewer_player_positions (
        match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
        round_number INTEGER NOT NULL,
        tick INTEGER NOT NULL,
        steamid TEXT NOT NULL,
        name TEXT NOT NULL,
        side TEXT,
        x REAL NOT NULL, y REAL NOT NULL, z REAL NOT NULL,
        is_alive INTEGER,
        health INTEGER,
        yaw REAL,
        PRIMARY KEY (match_id, round_number, tick, steamid)
    );
    CREATE INDEX viewer_player_tick ON viewer_player_positions(match_id, round_number, tick);
    CREATE TABLE viewer_grenade_positions (
        match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
        round_number INTEGER NOT NULL,
        tick INTEGER NOT NULL,
        trajectory_index INTEGER NOT NULL,
        entity_id INTEGER NOT NULL,
        grenade_type TEXT,
        thrower_steamid TEXT,
        x REAL NOT NULL, y REAL NOT NULL, z REAL NOT NULL,
        PRIMARY KEY (match_id, trajectory_index, tick)
    );
    CREATE INDEX viewer_grenade_tick ON viewer_grenade_positions(match_id, round_number, tick);
    "#,
    // v11 - exact observed bomb game events for the original viewer timeline.
    // Legacy imports retain an empty stream until reimported; no site or
    // position is inferred by migration.
    r#"
    ALTER TABLE matches ADD COLUMN viewer_bomb_version INTEGER;
    CREATE TABLE viewer_bomb_events (
        match_id INTEGER NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
        source_index INTEGER NOT NULL,
        round_number INTEGER NOT NULL,
        kind TEXT NOT NULL CHECK (kind IN ('bomb_planted','bomb_defused','bomb_exploded')),
        tick INTEGER NOT NULL,
        actor_steamid TEXT,
        actor_name TEXT,
        site_id INTEGER,
        x REAL, y REAL, z REAL,
        PRIMARY KEY (match_id, source_index)
    );
    CREATE INDEX viewer_bomb_tick ON viewer_bomb_events(match_id, round_number, tick);
    "#,
];

pub fn open(path: &Path) -> AppResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> AppResult<Connection> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> AppResult<()> {
    // journal_mode returns a row, so it can't go through execute_batch.
    let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    conn.execute_batch("PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;")?;
    Ok(())
}

fn migrate(conn: &Connection) -> AppResult<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > MIGRATIONS.len() as i64 {
        return Err(AppError::Invalid(format!(
            "DemoStrike database schema {version} is newer than this build supports"
        )));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate() {
        let target = (i + 1) as i64;
        if version < target {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", target)?;
            tx.commit()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod capability_migration_tests {
    use super::*;

    #[test]
    fn newer_database_schema_is_not_opened_as_an_older_one() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA user_version = 99;").unwrap();
        assert!(migrate(&conn).unwrap_err().to_string().contains("newer"));
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 99);
    }

    #[test]
    fn v2_match_and_annotation_survive_capability_migration() {
        let conn = Connection::open_in_memory().unwrap();
        configure(&conn).unwrap();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.execute_batch(MIGRATIONS[1]).unwrap();
        conn.execute_batch("PRAGMA user_version = 2;").unwrap();
        conn.execute("INSERT INTO matches (id,file_path,file_name,map_name,tick_rate,imported_at) VALUES (7,'C:/old.dem','old.dem','de_train',64,'2026-10-08')", []).unwrap();
        conn.execute("INSERT INTO utility_events (id,match_id,round_number,utility_type,thrower_steamid,thrower_name,thrower_team,throw_tick,throw_x,throw_y,throw_z,favorite) VALUES (9,7,1,'smoke','123','player','CT',100,1,2,3,1)", []).unwrap();
        migrate(&conn).unwrap();
        let capability: String = conn
            .query_row(
                "SELECT parser_capability FROM matches WHERE id=7",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let source_hash: Option<String> = conn
            .query_row("SELECT source_sha256 FROM matches WHERE id=7", [], |r| {
                r.get(0)
            })
            .unwrap();
        let favorite: i64 = conn
            .query_row("SELECT favorite FROM utility_events WHERE id=9", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(capability, "reference-raw-facts");
        assert_eq!(source_hash, None);
        assert_eq!(favorite, 1);
        let events = queries::query_events(&conn, &Default::default()).unwrap();
        assert_eq!(
            events[0].parser_capability,
            crate::domain::ParserCapabilities::ReferenceRawFacts
        );
        assert_eq!(events[0].damage_enemy, None);
        let legacy_damage: Option<i64> = conn
            .query_row(
                "SELECT damage_enemy FROM utility_events_supported WHERE id=9",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(legacy_damage, None);
        conn.execute(
            "UPDATE matches SET parser_capability='grenade-only-v1' WHERE id=7",
            [],
        )
        .unwrap();
        let native_damage: Option<i64> = conn
            .query_row(
                "SELECT damage_enemy FROM utility_events_supported WHERE id=9",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(native_damage, None);
        conn.execute(
            "UPDATE matches SET parser_capability='native-combat-flash-v1' WHERE id=7",
            [],
        )
        .unwrap();
        let measured_damage: Option<i64> = conn
            .query_row(
                "SELECT damage_enemy FROM utility_events_supported WHERE id=9",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(measured_damage, None);
        let detail = queries::get_event_detail(&conn, 9).unwrap();
        assert_eq!(detail.damage_team, None);
        assert_eq!(detail.ct_equip_value, None);
    }

    #[test]
    fn v8_combat_migrates_without_inventing_effective_hp_or_team() {
        let conn = Connection::open_in_memory().unwrap();
        configure(&conn).unwrap();
        for sql in &MIGRATIONS[..8] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 8).unwrap();
        conn.execute_batch("INSERT INTO matches
            (id,file_path,file_name,map_name,tick_rate,imported_at,parser_capability)
            VALUES (7,'C:/old.dem','old.dem','de_mirage',64,'2026-10-08','native-combat-full-v2');
            INSERT INTO rounds (match_id,round_number,start_tick) VALUES (7,1,100);
            INSERT INTO player_round_combat
            (match_id,round_number,steamid,side,enemy_kills,deaths,enemy_assists,enemy_health_damage)
            VALUES (7,1,'s1','CT',1,0,0,140);
            INSERT INTO raw_combat_hurts
            (match_id,source_index,tick,round_number,victim_steamid,raw_dmg_health)
            VALUES (7,0,200,1,'s2',140);").unwrap();
        migrate(&conn).unwrap();
        let damage: (i64, Option<i64>, Option<i64>) = conn.query_row(
            "SELECT raw_dmg_health, effective_health_damage, post_health FROM raw_combat_hurts WHERE match_id=7",
            [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).unwrap();
        let totals: (i64, Option<i64>, Option<String>) = conn
            .query_row(
                "SELECT enemy_health_damage, effective_enemy_health_damage, stable_team
             FROM player_round_combat WHERE match_id=7",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let team_a: Option<String> = conn
            .query_row(
                "SELECT team_a_side FROM rounds WHERE match_id=7",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let viewer_version: Option<i64> = conn
            .query_row(
                "SELECT viewer_position_version FROM matches WHERE id=7",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let viewer_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM viewer_player_positions WHERE match_id=7",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(damage, (140, None, None));
        assert_eq!(totals, (140, None, None));
        assert_eq!(team_a, None);
        assert_eq!(viewer_version, None);
        assert_eq!(viewer_rows, 0);
    }

    #[test]
    fn v6_import_migrates_without_inventing_combat_or_replacing_annotations() {
        let conn = Connection::open_in_memory().unwrap();
        configure(&conn).unwrap();
        for sql in &MIGRATIONS[..6] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 6).unwrap();
        conn.execute("INSERT INTO matches (id,file_path,file_name,map_name,tick_rate,imported_at,parser_capability)
            VALUES (7,'C:/old.dem','old.dem','de_train',64,'2026-10-08','native-combat-flash-v1')", []).unwrap();
        conn.execute(
            "INSERT INTO players (match_id,steamid,name) VALUES (7,'s1','preserved')",
            [],
        )
        .unwrap();
        conn.execute_batch(
            "INSERT INTO utility_events
            (id,match_id,round_number,utility_type,thrower_steamid,thrower_name,thrower_team,
             throw_tick,throw_x,throw_y,throw_z,favorite)
            VALUES (9,7,1,'smoke','s1','preserved','CT',100,1,2,3,1);
            INSERT INTO tags (id,name) VALUES (3,'approved');
            INSERT INTO event_tags (event_id,tag_id) VALUES (9,3);
            INSERT INTO collections (id,name,created_at) VALUES (4,'Guide','2026-10-08');
            INSERT INTO collection_events (collection_id,event_id,position) VALUES (4,9,0);
            INSERT INTO render_jobs (id,status,settings,output_dir,created_at)
              VALUES (5,'queued','{}','C:/output','2026-10-08');
            INSERT INTO render_job_events (job_id,event_id,position) VALUES (5,9,0);",
        )
        .unwrap();
        migrate(&conn).unwrap();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        let player: String = conn
            .query_row("SELECT name FROM players WHERE match_id=7", [], |row| {
                row.get(0)
            })
            .unwrap();
        let combat: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM player_round_combat WHERE match_id=7",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let raw_combat: i64 = conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM raw_combat_hurts WHERE match_id=7)
            + (SELECT COUNT(*) FROM raw_combat_deaths WHERE match_id=7)",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let bomb_capability: Option<i64> = conn
            .query_row(
                "SELECT viewer_bomb_version FROM matches WHERE id=7",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let annotations: (i64, i64, i64, i64) = conn
            .query_row(
                "SELECT e.favorite,
                (SELECT COUNT(*) FROM event_tags WHERE event_id=e.id),
                (SELECT COUNT(*) FROM collection_events WHERE event_id=e.id),
                (SELECT COUNT(*) FROM render_job_events WHERE event_id=e.id)
             FROM utility_events e WHERE e.id=9",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(version, 11);
        assert_eq!(bomb_capability, None);
        assert_eq!(player, "preserved");
        assert_eq!(combat, 0);
        assert_eq!(raw_combat, 0);
        assert_eq!(annotations, (1, 1, 1, 1));
    }
}
