//! Source-bound plans for offline CS2 capture. Planning never launches a game
//! or marks media accepted. ClipStrike's capture-package v1 remains the output
//! boundary after independently reviewed recording.
use crate::db::queries;
use crate::domain::{ParserCapabilities, TrajPoint, Vec3};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const CAPTURE_REQUEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureRequest {
    pub version: u32,
    pub demo_path: PathBuf,
    pub demo_sha256: String,
    pub demo_bytes: u64,
    /// SQLite selectors are local to this database; the hash and source facts
    /// below are required when reconciling a request against another database.
    pub match_id: i64,
    pub event_id: i64,
    pub map: String,
    pub round: i64,
    pub player_steamid64: String,
    pub utility_type: String,
    pub projectile_entity_id: i64,
    /// Raw userinfo ID confirmed at import. The CS2 spectator POV is not yet
    /// verified; recording QC must compare the actual visible player.
    pub observer_slot_candidate: i64,
    pub tick_rate: f64,
    pub throw_tick: i64,
    pub detonate_tick: i64,
    pub throw_position: Vec3,
    pub detonate_position: Vec3,
    /// Only actual projectile samples at or before detonation are included.
    /// Persisted trajectories can continue long after the visible flight.
    pub flight_samples: Vec<TrajPoint>,
    pub takes: Vec<PlannedTake>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlannedTake {
    pub camera_role: String,
    pub start_tick: i64,
    pub end_tick: i64,
}

pub(crate) fn hash_source(path: &Path) -> Result<(String, u64), String> {
    let mut file = File::open(path).map_err(|e| format!("cannot open source demo {}: {e}", path.display()))?;
    let mut hash = Sha256::new();
    let mut count = 0_u64;
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|e| format!("cannot hash source demo: {e}"))?;
        if read == 0 { break; }
        count += read as u64;
        hash.update(&buffer[..read]);
    }
    Ok((format!("{:x}", hash.finalize()), count))
}

/// Builds an immutable event plan from an existing, independently imported
/// native match. The database is opened read-only and no source file is edited.
pub fn build_capture_request(database: &Path, event_id: i64) -> Result<CaptureRequest, String> {
    let conn = Connection::open_with_flags(database, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("cannot open match database read-only: {e}"))?;
    let detail = queries::get_event_detail(&conn, event_id).map_err(|e| e.to_string())?;
    if !matches!(detail.summary.parser_capability,
        ParserCapabilities::NativeCombatFlashV1 | ParserCapabilities::NativeCombatFullV2
        | ParserCapabilities::GrenadeAnalysisOnlyV1) {
        return Err("capture planning requires a natively imported demo with source-bound throw facts".into());
    }
    let (stored_hash, projectile_entity_id): (Option<String>, Option<i64>) = conn.query_row(
        "SELECT m.source_sha256, e.projectile_entity_id FROM utility_events e \
         JOIN matches m ON m.id=e.match_id WHERE e.id=?1",
        params![event_id], |row| Ok((row.get(0)?, row.get(1)?))
    ).map_err(|e| format!("cannot read event provenance: {e}"))?;
    let stored_hash = stored_hash.filter(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("saved match has no valid demo SHA-256; reimport from verified source before capture")?;
    let projectile_entity_id = projectile_entity_id.ok_or("selected event has no causal projectile identity")?;
    let source = PathBuf::from(&detail.demo_path);
    let (actual_hash, demo_bytes) = hash_source(&source)?;
    if actual_hash != stored_hash.to_ascii_lowercase() {
        return Err("source demo bytes differ from the imported match; capture request was not created".into());
    }
    let event = &detail.summary;
    let throw_tick = event.throw_tick;
    let detonate_tick = detail.detonate_tick.ok_or("selected throw has no decoded detonation")?;
    let detonate_position = event.detonate_pos.ok_or("selected throw has no decoded detonation position")?;
    if !(throw_tick >= 0 && detonate_tick > throw_tick && detail.tick_rate.is_finite()
        && (1.0..=1024.0).contains(&detail.tick_rate)) {
        return Err("selected event has unsupported capture timing".into());
    }
    let observer_slot_candidate = detail.observer_slot.ok_or("selected throw has no live userinfo observer candidate")?;
    if observer_slot_candidate < 0 { return Err("observer candidate is negative".into()); }
    let flight_samples: Vec<_> = detail.trajectory.iter().copied()
        .filter(|sample| sample[0] >= throw_tick as f64 && sample[0] <= detonate_tick as f64)
        .collect();
    if flight_samples.len() < 2 || flight_samples.iter().any(|sample| sample.iter().any(|v| !v.is_finite()))
        || flight_samples.windows(2).any(|pair| pair[0][0] >= pair[1][0])
        || detonate_tick as f64 - flight_samples.last().unwrap()[0] > 16.0 {
        return Err("selected throw lacks a complete, ordered pre-detonation flight path".into());
    }
    let pre_roll = (detail.tick_rate * 3.5).round() as i64;
    let release_tail = (detail.tick_rate * 0.5).round() as i64;
    let effect_tail = detail.tick_rate.round() as i64;
    let takes = vec![
        PlannedTake { camera_role: "player_pov".into(),
            start_tick: throw_tick.saturating_sub(pre_roll).max(0),
            end_tick: throw_tick + release_tail },
        PlannedTake { camera_role: "projectile".into(),
            start_tick: throw_tick, end_tick: detonate_tick + effect_tail },
        PlannedTake { camera_role: "third_person_reference".into(),
            start_tick: throw_tick.saturating_sub((detail.tick_rate * 2.0).round() as i64).max(0),
            end_tick: throw_tick + release_tail },
    ];
    if takes.iter().any(|take| take.start_tick >= take.end_tick) {
        return Err("capture take has an empty tick range".into());
    }
    Ok(CaptureRequest {
        version: CAPTURE_REQUEST_VERSION,
        demo_path: source,
        demo_sha256: actual_hash,
        demo_bytes,
        match_id: event.match_id,
        event_id,
        map: event.map_name.clone(),
        round: event.round_number,
        player_steamid64: event.thrower_steamid.clone(),
        utility_type: event.utility_type.as_str().into(),
        projectile_entity_id,
        observer_slot_candidate,
        tick_rate: detail.tick_rate,
        throw_tick,
        detonate_tick,
        throw_position: event.throw_pos,
        detonate_position,
        flight_samples,
        takes,
    })
}

/// Refuse to overwrite an earlier request, including one that may already
/// have a producer receipt. The caller chooses a fresh local destination.
pub fn save_request_noclobber(request: &CaptureRequest, path: &Path) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(request).map_err(|e| e.to_string())?;
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)
        .map_err(|e| format!("cannot create new capture request {}: {e}", path.display()))?;
    if let Err(error) = file.write_all(&bytes).and_then(|_| file.sync_all()) {
        return Err(format!("could not finish capture request {}: {error}; inspect the incomplete file before retrying", path.display()));
    }
    Ok(())
}
