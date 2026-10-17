use demostrike_rust_candidate::library::{
    ScoreRelation, get_match_detail, get_viewer_frames, get_viewer_round, list_matches,
    score_relation,
};
use rusqlite::Connection;
use tempfile::tempdir;

fn fixture() -> (tempfile::TempDir, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "PRAGMA user_version = 6;
         CREATE TABLE matches (id INTEGER PRIMARY KEY, file_name TEXT, map_name TEXT, score_ct INTEGER, score_t INTEGER, imported_at TEXT, parser_capability TEXT, effect_attribution_version INTEGER);
         CREATE TABLE players (match_id INTEGER, steamid TEXT, name TEXT);
         CREATE TABLE rounds (match_id INTEGER, round_number INTEGER, winner TEXT, start_tick INTEGER, end_tick INTEGER);
         CREATE TABLE utility_events (id INTEGER PRIMARY KEY, match_id INTEGER, round_number INTEGER, utility_type TEXT, thrower_name TEXT, thrower_team TEXT, throw_tick INTEGER, enemies_blinded INTEGER, damage_enemy INTEGER, blind_attribution_complete INTEGER NOT NULL DEFAULT 0, damage_attribution_complete INTEGER NOT NULL DEFAULT 0);
         CREATE VIEW utility_events_supported AS
         SELECT e.id, e.match_id, m.effect_attribution_version, e.blind_attribution_complete, e.damage_attribution_complete,
                CASE WHEN m.parser_capability = 'grenade-only-v1' OR m.effect_attribution_version IS NOT 2 OR e.blind_attribution_complete != 1 THEN NULL ELSE e.enemies_blinded END AS enemies_blinded,
                CASE WHEN m.parser_capability = 'grenade-only-v1' OR m.effect_attribution_version IS NOT 2 OR e.damage_attribution_complete != 1 THEN NULL ELSE e.damage_enemy END AS damage_enemy
         FROM utility_events e JOIN matches m ON m.id = e.match_id;
         INSERT INTO matches VALUES (1, 'cache.dem', 'de_cache', 10, 5, '2026-10-17T00:00:00Z', 'native-combat-flash-v1', NULL);
         INSERT INTO players VALUES (1, '7656', 'Tester');
         INSERT INTO rounds VALUES (1, 1, 'CT', 100, 200);
         INSERT INTO utility_events (id, match_id, round_number, utility_type, thrower_name, thrower_team, throw_tick, enemies_blinded, damage_enemy)
         VALUES (1, 1, 1, 'smoke', 'Tester', 'CT', 140, 8, 34);",
    )
    .unwrap();
    (dir, path.to_string_lossy().into_owned())
}

#[test]
fn lists_real_schema_rows_without_writing() {
    let (_dir, path) = fixture();
    let before = std::fs::read(&path).unwrap();
    let snapshot = list_matches(&path).unwrap();
    assert_eq!(snapshot.matches.len(), 1);
    assert_eq!(snapshot.matches[0].utility_count, 1);
    assert_eq!(snapshot.matches[0].player_count, 1);
    assert_eq!(snapshot.matches[0].source_sha256, None);
    assert_eq!(snapshot.matches[0].tick_rate, None);
    assert_eq!(
        snapshot.matches[0].score_ct_relation,
        ScoreRelation::Leading
    );
    assert_eq!(
        snapshot.matches[0].score_t_relation,
        ScoreRelation::Trailing
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn preserves_extended_import_metadata_without_guessing_missing_fields() {
    let (_dir, path) = fixture();
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "ALTER TABLE matches ADD COLUMN file_path TEXT;
         ALTER TABLE matches ADD COLUMN source_sha256 TEXT;
         ALTER TABLE matches ADD COLUMN tick_rate REAL;
         ALTER TABLE matches ADD COLUMN duration_ticks INTEGER;
         UPDATE matches SET file_path = 'C:/synthetic/cache.dem',
           source_sha256 = 'synthetic-checksum', tick_rate = 64.0,
           duration_ticks = 57600;",
    )
    .unwrap();
    let before = std::fs::read(&path).unwrap();
    let summary = &list_matches(&path).unwrap().matches[0];
    assert_eq!(
        summary.demo_file_path.as_deref(),
        Some("C:/synthetic/cache.dem")
    );
    assert_eq!(summary.source_sha256.as_deref(), Some("synthetic-checksum"));
    assert_eq!(summary.tick_rate, Some(64.0));
    assert_eq!(summary.duration_ticks, Some(57600));
    assert_eq!(std::fs::read(path).unwrap(), before);
}

#[test]
fn preserves_upstream_team_score_relation_for_all_three_cases() {
    assert_eq!(score_relation(5, 5), ScoreRelation::Tied);
    assert_eq!(score_relation(10, 5), ScoreRelation::Leading);
    assert_eq!(score_relation(5, 10), ScoreRelation::Trailing);
}

#[test]
fn loads_rounds_players_and_utility_for_selected_match() {
    let (_dir, path) = fixture();
    let detail = get_match_detail(&path, 1).unwrap();
    assert_eq!(detail.summary.map_name, "de_cache");
    assert_eq!(detail.rounds.len(), 1);
    assert_eq!(detail.players[0].name, "Tester");
    assert!(!detail.full_combat_coverage);
    assert!(detail.players[0].combat.is_none());
    assert!(detail.rounds[0].combat.is_none());
    assert_eq!(detail.utility_events[0].utility_type, "smoke");
    assert_eq!(detail.utility_events[0].enemies_blinded, None);
    assert_eq!(detail.utility_events[0].damage_enemy, None);
    assert_eq!(detail.utility_breakdown[0].count, 1);
}

#[test]
fn viewer_reads_absolute_tick_frames_but_never_invents_legacy_positions() {
    let (_dir, path) = fixture();
    let before = std::fs::read(&path).unwrap();
    let old = get_viewer_round(&path, 1, 1).unwrap();
    assert!(!old.positions_available);
    assert_eq!(old.player_sample_count, 0);
    assert!(get_viewer_frames(&path, 1, 1, 100, 110).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);

    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA user_version=10;
        ALTER TABLE matches ADD COLUMN viewer_position_version INTEGER;
        ALTER TABLE matches ADD COLUMN tick_rate REAL;
        UPDATE matches SET viewer_position_version=1,tick_rate=64 WHERE id=1;
        CREATE TABLE viewer_player_positions (match_id INTEGER,round_number INTEGER,tick INTEGER,
          steamid TEXT,name TEXT,side TEXT,x REAL,y REAL,z REAL,is_alive INTEGER,health INTEGER,yaw REAL);
        CREATE TABLE viewer_grenade_positions (match_id INTEGER,round_number INTEGER,tick INTEGER,
          trajectory_index INTEGER,entity_id INTEGER,grenade_type TEXT,thrower_steamid TEXT,x REAL,y REAL,z REAL);
        INSERT INTO viewer_player_positions VALUES
          (1,1,100,'7656','Tester','CT',10,20,30,1,87,45),
          (1,1,101,'7656','Tester','CT',11,21,31,1,87,46),
          (1,1,102,'7656','Tester','CT',12,22,32,0,0,47);
        INSERT INTO viewer_grenade_positions VALUES
          (1,1,101,3,44,'CSmokeGrenadeProjectile','7656',40,50,60);").unwrap();
    let meta = get_viewer_round(&path, 1, 1).unwrap();
    assert!(meta.positions_available);
    assert!(meta.utility_events.is_empty());
    assert_eq!(meta.tick_rate, Some(64.0));
    assert_eq!(
        (meta.player_sample_count, meta.grenade_sample_count),
        (3, 1)
    );
    let chunk = get_viewer_frames(&path, 1, 1, 101, 102).unwrap();
    assert_eq!(
        chunk.players.iter().map(|row| row.tick).collect::<Vec<_>>(),
        vec![101, 102]
    );
    assert_eq!(chunk.players[1].is_alive, Some(false));
    assert_eq!(chunk.grenades[0].entity_id, 44);
    assert!(get_viewer_frames(&path, 1, 1, 100, 201).is_err());
    assert!(get_viewer_frames(&path, 1, 1, 102, 101).is_err());
    db.execute_batch(
        "ALTER TABLE utility_events ADD COLUMN thrower_steamid TEXT;
         ALTER TABLE utility_events ADD COLUMN detonate_tick INTEGER;
         ALTER TABLE utility_events ADD COLUMN detonate_x REAL;
         ALTER TABLE utility_events ADD COLUMN detonate_y REAL;
         ALTER TABLE utility_events ADD COLUMN detonate_z REAL;
         ALTER TABLE utility_events ADD COLUMN projectile_entity_id INTEGER;
         UPDATE utility_events SET thrower_steamid='7656',detonate_tick=150,
           detonate_x=4,detonate_y=5,detonate_z=6,projectile_entity_id=44;
         UPDATE viewer_player_positions SET is_alive=NULL WHERE tick=101;",
    )
    .unwrap();
    let meta = get_viewer_round(&path, 1, 1).unwrap();
    assert_eq!(meta.utility_events.len(), 1);
    assert_eq!(meta.utility_events[0].detonate_tick, Some(150));
    assert_eq!(meta.utility_events[0].projectile_entity_id, Some(44));
    assert_eq!(
        get_viewer_frames(&path, 1, 1, 101, 101).unwrap().players[0].is_alive,
        None
    );
    db.execute_batch(
        "PRAGMA user_version=11;
         ALTER TABLE matches ADD COLUMN viewer_bomb_version INTEGER;
         CREATE TABLE viewer_bomb_events
           (match_id INTEGER,source_index INTEGER,round_number INTEGER,kind TEXT,tick INTEGER,
            actor_steamid TEXT,actor_name TEXT,site_id INTEGER,x REAL,y REAL,z REAL);
         INSERT INTO viewer_bomb_events VALUES
           (1,0,1,'bomb_planted',160,'7656','Tester',NULL,10,20,30),
           (1,1,1,'bomb_exploded',190,NULL,NULL,NULL,NULL,NULL,NULL);",
    )
    .unwrap();
    assert!(!get_viewer_round(&path, 1, 1).unwrap().bomb_events_available);
    db.execute("UPDATE matches SET viewer_bomb_version=1 WHERE id=1", [])
        .unwrap();
    let meta = get_viewer_round(&path, 1, 1).unwrap();
    assert!(meta.bomb_events_available);
    assert_eq!(meta.bomb_events.len(), 2);
    assert_eq!(meta.bomb_events[0].site_id, None);
    assert_eq!(meta.bomb_events[1].x, None);
}

#[test]
fn full_combat_is_read_only_and_gated_by_schema_and_parser_provenance() {
    let (_dir, path) = fixture();
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "PRAGMA user_version = 7;
         CREATE TABLE player_round_combat (
           match_id INTEGER, round_number INTEGER, steamid TEXT, side TEXT,
           enemy_kills INTEGER, deaths INTEGER, enemy_assists INTEGER,
           enemy_health_damage INTEGER);
         INSERT INTO player_round_combat VALUES (1,1,'7656','CT',2,1,1,88);
         UPDATE matches SET parser_capability = 'native-combat-full-v2';",
    )
    .unwrap();
    let before = std::fs::read(&path).unwrap();
    let detail = get_match_detail(&path, 1).unwrap();
    assert!(detail.full_combat_coverage);
    let combat = detail.players[0].combat.as_ref().unwrap();
    assert_eq!(combat.enemy_kills, Some(2));
    assert_eq!(combat.deaths, 1);
    assert_eq!(combat.enemy_assists, Some(1));
    assert_eq!(combat.enemy_health_damage, Some(88));
    assert_eq!(detail.rounds[0].combat.as_ref().unwrap().ct_players, 1);
    assert_eq!(std::fs::read(&path).unwrap(), before);

    db.execute(
        "UPDATE matches SET parser_capability = 'test-synthetic-combat-v1'",
        [],
    )
    .unwrap();
    let synthetic = get_match_detail(&path, 1).unwrap();
    assert!(synthetic.full_combat_coverage);
    assert_eq!(
        synthetic.summary.parser_capability,
        "test-synthetic-combat-v1"
    );

    db.execute(
        "UPDATE matches SET parser_capability = 'native-combat-flash-v1'",
        [],
    )
    .unwrap();
    assert!(
        get_match_detail(&path, 1).unwrap().players[0]
            .combat
            .is_none()
    );
}

#[test]
fn ordered_deaths_and_round_fields_remain_distinct_from_missing_facts() {
    let (_dir, path) = fixture();
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "PRAGMA user_version = 8;
         ALTER TABLE rounds ADD COLUMN freeze_end_tick INTEGER;
         ALTER TABLE rounds ADD COLUMN end_reason TEXT;
         ALTER TABLE rounds ADD COLUMN ct_equip_value INTEGER;
         ALTER TABLE rounds ADD COLUMN t_equip_value INTEGER;
         UPDATE rounds SET freeze_end_tick = 120, end_reason = 'bomb_defused',
           ct_equip_value = 18000, t_equip_value = 12000;
         INSERT INTO rounds (match_id, round_number, winner, start_tick, end_tick)
           VALUES (1, 2, 'T', 201, 300);
         CREATE TABLE player_round_combat (
           match_id INTEGER, round_number INTEGER, steamid TEXT, side TEXT,
           enemy_kills INTEGER, deaths INTEGER, enemy_assists INTEGER,
           enemy_health_damage INTEGER);
         INSERT INTO player_round_combat VALUES (1,1,'7656','CT',2,1,1,88);
         CREATE TABLE raw_combat_deaths (
           match_id INTEGER, source_index INTEGER, event_order INTEGER, tick INTEGER,
           round_number INTEGER, weapon TEXT, attacker_steamid TEXT,
           attacker_side TEXT, victim_steamid TEXT, victim_side TEXT,
           assister_steamid TEXT);
         CREATE TABLE raw_combat_hurts (
           match_id INTEGER, source_index INTEGER, event_order INTEGER, tick INTEGER,
           round_number INTEGER, weapon TEXT, attacker_steamid TEXT,
           attacker_side TEXT, victim_steamid TEXT, victim_side TEXT,
           raw_dmg_health INTEGER);
         UPDATE matches SET parser_capability = 'native-combat-full-v2';",
    )
    .unwrap();
    let before = std::fs::read(&path).unwrap();
    let missing = get_match_detail(&path, 1).unwrap();
    assert!(missing.rounds[0].deaths.is_none());
    assert!(missing.rounds[0].hurts.is_none());
    assert_eq!(missing.rounds[0].freeze_end_tick, Some(120));
    assert_eq!(
        missing.rounds[0].end_reason.as_deref(),
        Some("bomb_defused")
    );
    assert_eq!(missing.rounds[0].ct_equip_value, Some(18000));
    assert_eq!(missing.rounds[0].t_equip_value, Some(12000));
    assert_eq!(missing.rounds[1].freeze_end_tick, None);
    assert_eq!(std::fs::read(&path).unwrap(), before);

    db.execute_batch(
        "INSERT INTO raw_combat_deaths VALUES
           (1, 0, 2, 175, 1, 'ak47', '7656', 'CT', 'opponent', 'T', NULL),
           (1, 1, 1, 150, 1, 'm4a1', 'opponent', 'TERRORIST', '7656', 'CT', NULL),
           (1, 2, 3, 200, 2, 'ak47', '7656', 'CT', 'opponent', 'T', NULL),
           (1, 3, 4, 301, 3, 'world', '7656', 'CT', '7656', 'CT', NULL);
         INSERT INTO raw_combat_hurts VALUES
           (1, 0, 1, 180, 1, 'ak47', '7656', 'CT', 'opponent', 'TERRORIST', 42),
           (1, 1, 2, 200, 2, 'ak47', '7656', 'CT', 'opponent', 'T', 17),
           (1, 2, 3, 301, 3, 'world', '7656', 'CT', '7656', 'CT', 99);",
    )
    .unwrap();
    let before = std::fs::read(&path).unwrap();
    let observed = get_match_detail(&path, 1).unwrap();
    let deaths = observed.rounds[0].deaths.as_ref().unwrap();
    assert_eq!(
        deaths.iter().map(|event| event.tick).collect::<Vec<_>>(),
        vec![150, 175, 200]
    );
    assert_eq!(deaths[0].weapon.as_deref(), Some("m4a1"));
    assert_eq!(deaths[0].attacker_side.as_deref(), Some("T"));
    assert_eq!(observed.rounds[1].deaths.as_ref().unwrap().len(), 0);
    let hurts = observed.rounds[0].hurts.as_ref().unwrap();
    assert_eq!(
        hurts.iter().map(|event| event.tick).collect::<Vec<_>>(),
        vec![180, 200]
    );
    assert_eq!(hurts[1].raw_damage, 17);
    assert_eq!(hurts[0].victim_side.as_deref(), Some("T"));
    assert!(observed.rounds[1].hurts.as_ref().unwrap().is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), before);

    db.execute(
        "UPDATE matches SET parser_capability = 'test-synthetic-combat-v1'",
        [],
    )
    .unwrap();
    let synthetic = get_match_detail(&path, 1).unwrap();
    assert!(synthetic.rounds[0].deaths.is_none());
    assert!(synthetic.rounds[0].hurts.is_none());
}

#[test]
fn effect_totals_follow_supported_view_provenance() {
    let (_dir, path) = fixture();
    let db = Connection::open(&path).unwrap();
    db.execute(
        "UPDATE utility_events SET blind_attribution_complete = 1, damage_attribution_complete = 1",
        [],
    )
    .unwrap();
    // Versionless old imports retain stored numbers, but cannot prove attribution.
    let detail = get_match_detail(&path, 1).unwrap();
    assert_eq!(detail.utility_events[0].enemies_blinded, None);
    assert_eq!(detail.utility_events[0].damage_enemy, None);

    db.execute("UPDATE matches SET effect_attribution_version = 2", [])
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    let detail = get_match_detail(&path, 1).unwrap();
    assert_eq!(detail.utility_events[0].enemies_blinded, Some(8));
    assert_eq!(detail.utility_events[0].damage_enemy, Some(34));
    assert_eq!(std::fs::read(&path).unwrap(), before);

    db.execute(
        "UPDATE utility_events SET damage_attribution_complete = 0",
        [],
    )
    .unwrap();
    let detail = get_match_detail(&path, 1).unwrap();
    assert_eq!(detail.utility_events[0].enemies_blinded, Some(8));
    assert_eq!(detail.utility_events[0].damage_enemy, None);

    db.execute(
        "UPDATE matches SET parser_capability = 'grenade-only-v1'",
        [],
    )
    .unwrap();
    let detail = get_match_detail(&path, 1).unwrap();
    assert_eq!(detail.utility_events[0].enemies_blinded, None);
    assert_eq!(detail.utility_events[0].damage_enemy, None);

    db.execute(
        "UPDATE matches SET parser_capability = 'native-combat-flash-v1'",
        [],
    )
    .unwrap();
    db.execute("UPDATE utility_events SET enemies_blinded = 0, damage_enemy = 0, damage_attribution_complete = 1", []).unwrap();
    let detail = get_match_detail(&path, 1).unwrap();
    assert_eq!(detail.utility_events[0].enemies_blinded, Some(0));
    assert_eq!(detail.utility_events[0].damage_enemy, Some(0));
}

#[test]
fn legacy_view_does_not_promote_stored_totals_into_facts() {
    let (_dir, path) = fixture();
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "DROP VIEW utility_events_supported;
         PRAGMA user_version = 5;
         CREATE VIEW utility_events_supported AS
         SELECT id, match_id, enemies_blinded, damage_enemy FROM utility_events;",
    )
    .unwrap();
    let detail = get_match_detail(&path, 1).unwrap();
    assert_eq!(detail.utility_events[0].enemies_blinded, None);
    assert_eq!(detail.utility_events[0].damage_enemy, None);
    assert_eq!(detail.utility_events.len(), 1);
    db.execute_batch("PRAGMA user_version = 6;").unwrap();
    let detail = get_match_detail(&path, 1).unwrap();
    assert_eq!(detail.utility_events[0].enemies_blinded, None);
    assert_eq!(detail.utility_events[0].damage_enemy, None);
}

#[test]
fn rejects_missing_file_and_unknown_schema() {
    let dir = tempdir().unwrap();
    assert!(list_matches(&dir.path().join("missing.sqlite").to_string_lossy()).is_err());
    let path = dir.path().join("wrong.sqlite");
    Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE unrelated (id INTEGER);")
        .unwrap();
    assert!(list_matches(&path.to_string_lossy()).is_err());
}

#[test]
fn empty_valid_library_is_valid_and_missing_match_is_an_error() {
    let (_dir, path) = fixture();
    let db = Connection::open(&path).unwrap();
    db.execute("DELETE FROM utility_events", []).unwrap();
    db.execute("DELETE FROM rounds", []).unwrap();
    db.execute("DELETE FROM players", []).unwrap();
    db.execute("DELETE FROM matches", []).unwrap();
    assert!(list_matches(&path).unwrap().matches.is_empty());
    assert!(get_match_detail(&path, 1).is_err());
}
