//! Hosted-only integration test. PUBLIC_CS2_DEMO points to the pinned public
//! fixture downloaded and verified by public-cs2-decoder-probe.yml.
use demostrike::demo_import::{ImportControl, import_demo};
use demostrike::native_grenade_parser::NativeGrenadeParser;
use rusqlite::Connection;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn count(connection: &Connection, table: &str) -> i64 {
    connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0)).unwrap()
}

#[test]
#[ignore = "requires the bounded public CS2 demo downloaded by hosted CI"]
fn public_demo_import_is_transactional_and_persistent() {
    let demo = PathBuf::from(std::env::var_os("PUBLIC_CS2_DEMO").expect("PUBLIC_CS2_DEMO is required"));
    assert!(demo.is_file());
    let work = tempfile::tempdir().unwrap();
    let db = work.path().join("matches.sqlite");
    let parser = NativeGrenadeParser;

    let imported = import_demo(&demo, &db, &parser, &ImportControl::new(), |_| {}).unwrap();
    assert_eq!(imported.capabilities.key(), "native-combat-full-v2");
    // Frozen expectations from the pinned public recording, measured by the
    // real native probe rather than the synthetic JSON fixture.
    assert_eq!(imported.analyzed.events.len(), 150);
    assert_eq!(imported.analyzed.raw_combat_hurts.len(), 264);
    assert_eq!(imported.analyzed.raw_combat_deaths.len(), 73);
    assert!(imported.analyzed.rounds.len() >= 10);
    for round in &imported.analyzed.rounds {
        if let (Some(start), Some(end)) = (round.start_tick, round.end_tick) {
            assert!(start < end, "round {} starts at {start} after ending at {end}", round.round_number);
        }
    }
    drop(imported);

    // A separate connection represents application restart, and checks that
    // both aggregates and ordered event facts survived the transaction.
    let reopened = demostrike::db::open(&db).unwrap();
    assert_eq!(count(&reopened, "matches"), 1);
    assert!(count(&reopened, "rounds") > 1);
    assert!(count(&reopened, "player_round_combat") > 0);
    assert!(count(&reopened, "raw_combat_hurts") > 0);
    assert!(count(&reopened, "raw_combat_deaths") > 0);
    let before = (count(&reopened, "matches"), count(&reopened, "raw_combat_hurts"));
    drop(reopened);

    let duplicate = import_demo(&demo, &db, &parser, &ImportControl::new(), |_| {}).unwrap_err();
    assert!(duplicate.contains("already imported"), "{duplicate}");
    let same_bytes = work.path().join("same-bytes.dem");
    std::fs::copy(&demo, &same_bytes).unwrap();
    let duplicate_hash = import_demo(&same_bytes, &db, &parser, &ImportControl::new(), |_| {}).unwrap_err();
    assert!(duplicate_hash.contains("identical demo bytes"), "{duplicate_hash}");

    let corrupt = work.path().join("corrupt.dem");
    std::fs::write(&corrupt, b"PBDEMS2\0invalid").unwrap();
    assert!(import_demo(&corrupt, &db, &parser, &ImportControl::new(), |_| {}).is_err());
    let reopened = demostrike::db::open(&db).unwrap();
    assert_eq!((count(&reopened, "matches"), count(&reopened, "raw_combat_hurts")), before);
    drop(reopened);

    // Cancel after the production decoder reports actual tick progress.
    let cancel_db = work.path().join("cancelled.sqlite");
    let control = ImportControl::new();
    let worker_control = control.clone();
    let worker_demo = demo.clone();
    let worker_db = cancel_db.clone();
    let worker = std::thread::spawn(move || import_demo(
        &worker_demo, &worker_db, &NativeGrenadeParser, &worker_control, |_| {}
    ));
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut observed = false;
    while Instant::now() < deadline {
        if control.parsing_progress().is_some_and(|(tick, _)| tick >= 300) {
            observed = true;
            assert!(control.request_cancel());
            break;
        }
        if worker.is_finished() { break; }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(observed, "decoder did not expose progress before completion");
    let error = worker.join().unwrap().unwrap_err();
    assert!(error.contains("cancelled"), "{error}");
    let reopened = demostrike::db::open(&cancel_db).unwrap();
    assert_eq!(count(&reopened, "matches"), 0);
    assert_eq!(count(&reopened, "raw_combat_hurts"), 0);
}

