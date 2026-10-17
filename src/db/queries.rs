//! All SQL lives here. Commands call these; nothing else touches the DB.

use crate::analysis::{AnalyzedMatch, AnalyzedRound};
use crate::domain::*;
use crate::error::{AppError, AppResult};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};
use std::collections::{HashMap, HashSet};

/// Resolve existing source files before comparing import identities. This also
/// covers legacy rows, without changing stored paths or migrating the schema.
fn canonical_source_identity(path: &str) -> Option<String> {
    let resolved = std::fs::canonicalize(path).ok()?;
    let text = resolved.to_string_lossy().into_owned();
    #[cfg(windows)]
    {
        Some(text.to_lowercase())
    }
    #[cfg(not(windows))]
    {
        Some(text)
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

// Retain observed bomb events and projectile tails between recorded rounds
// under the preceding round, without inventing playable player frames there.
fn viewer_observation_round(rounds: &[AnalyzedRound], tick: i64) -> Option<i64> {
    if let Some(round) = rounds.iter().find(|round| {
        matches!((round.start_tick, round.end_tick), (Some(start), Some(end)) if start <= tick && tick <= end)
    }) {
        return Some(round.round_number);
    }
    rounds
        .windows(2)
        .find_map(|pair| match (pair[0].end_tick, pair[1].start_tick) {
            (Some(end), Some(next_start)) if end < tick && tick < next_start => {
                Some(pair[0].round_number)
            }
            _ => None,
        })
}

#[cfg(test)]
mod viewer_observation_round_tests {
    use super::*;

    fn round(number: i64, start: i64, end: i64) -> AnalyzedRound {
        AnalyzedRound {
            round_number: number,
            start_tick: Some(start),
            freeze_end_tick: None,
            end_tick: Some(end),
            winner: None,
            end_reason: None,
            team_a_side: None,
            team_b_side: None,
            ct_equip_value: None,
            t_equip_value: None,
        }
    }

    #[test]
    fn inter_round_bomb_event_is_retained_with_preceding_round() {
        let rounds = [round(3, 15680, 21457), round(4, 21905, 30000)];
        assert_eq!(viewer_observation_round(&rounds, 21853), Some(3));
        assert_eq!(viewer_observation_round(&rounds, 21905), Some(4));
        assert_eq!(viewer_observation_round(&rounds, 30001), None);
    }
}

// ---------------------------------------------------------------- matches

/// Paths and fingerprints for import identity checks. Old rows have no hash.
pub fn source_identities(conn: &Connection) -> AppResult<Vec<(i64, String, Option<String>)>> {
    let mut stmt = conn.prepare("SELECT id, file_path, source_sha256 FROM matches")?;
    Ok(stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<Result<_, _>>()?)
}

pub fn insert_match(conn: &mut Connection, file_path: &str, m: &AnalyzedMatch) -> AppResult<i64> {
    insert_match_with_control(conn, file_path, m, || false, || Ok(()))
}

/// Insert without replacing prior event IDs or user annotations. `begin_commit`
/// is the last cancellable boundary and is called immediately before COMMIT.
pub fn insert_match_with_control(
    conn: &mut Connection,
    file_path: &str,
    m: &AnalyzedMatch,
    should_cancel: impl Fn() -> bool,
    begin_commit: impl FnOnce() -> AppResult<()>,
) -> AppResult<i64> {
    insert_match_with_parser_capability(
        conn,
        file_path,
        m,
        "reference-raw-facts",
        should_cancel,
        begin_commit,
    )
}

pub fn insert_match_with_parser_capability(
    conn: &mut Connection,
    file_path: &str,
    m: &AnalyzedMatch,
    parser_capability: &str,
    should_cancel: impl Fn() -> bool,
    begin_commit: impl FnOnce() -> AppResult<()>,
) -> AppResult<i64> {
    insert_match_with_source_hash(
        conn,
        file_path,
        m,
        parser_capability,
        None,
        should_cancel,
        begin_commit,
    )
}

pub fn insert_match_with_source_hash(
    conn: &mut Connection,
    file_path: &str,
    m: &AnalyzedMatch,
    parser_capability: &str,
    source_sha256: Option<&str>,
    should_cancel: impl Fn() -> bool,
    begin_commit: impl FnOnce() -> AppResult<()>,
) -> AppResult<i64> {
    let check_cancel = || {
        if should_cancel() {
            Err(AppError::Invalid("demo import cancelled".into()))
        } else {
            Ok(())
        }
    };
    check_cancel()?;
    let tx = conn.transaction()?;
    let existing: Option<i64> = tx
        .query_row(
            "SELECT id FROM matches WHERE file_path = ?1",
            params![file_path],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Err(AppError::Invalid(format!(
            "demo is already imported as match {id}; ordinary reimport is blocked to preserve favorites, tags, collections and render-job references"
        )));
    }
    if let Some(identity) = canonical_source_identity(file_path) {
        let mut stmt = tx.prepare("SELECT id, file_path FROM matches")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, old_path) = row?;
            if canonical_source_identity(&old_path).as_deref() == Some(identity.as_str()) {
                return Err(AppError::Invalid(format!(
                    "demo is already imported as match {id} through an equivalent source path; ordinary reimport is blocked to preserve annotations"
                )));
            }
        }
    }
    if let Some(hash) = source_sha256 {
        if let Some(id) = tx
            .query_row(
                "SELECT id FROM matches WHERE source_sha256 = ?1",
                params![hash],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
        {
            return Err(AppError::Invalid(format!("identical demo bytes are already imported as match {id}; existing annotations were preserved")));
        }
    }

    let file_name = std::path::Path::new(file_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| file_path.to_string());

    tx.execute(
        "INSERT INTO matches (file_path, file_name, map_name, tick_rate, duration_ticks,
                              rounds_count, score_ct, score_t, imported_at, parser_capability, source_sha256,
                              effect_attribution_version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 2)",
        params![
            file_path,
            file_name,
            m.map_name,
            m.tick_rate,
            m.duration_ticks,
            m.rounds.len() as i64,
            m.score_ct,
            m.score_t,
            now_iso(),
            parser_capability,
            source_sha256,
        ],
    )?;
    let match_id = tx.last_insert_rowid();

    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO players (match_id, steamid, name) VALUES (?1, ?2, ?3)",
        )?;
        for p in &m.players {
            check_cancel()?;
            stmt.execute(params![match_id, p.steamid, p.name])?;
        }

        let mut stmt = tx.prepare(
            "INSERT INTO rounds (match_id, round_number, start_tick, freeze_end_tick, end_tick,
                                 winner, end_reason, ct_equip_value, t_equip_value, team_a_side, team_b_side)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?;
        for r in &m.rounds {
            check_cancel()?;
            stmt.execute(params![
                match_id,
                r.round_number,
                r.start_tick,
                r.freeze_end_tick,
                r.end_tick,
                r.winner,
                r.end_reason,
                r.ct_equip_value,
                r.t_equip_value,
                r.team_a_side,
                r.team_b_side
            ])?;
        }

        let mut combat = tx.prepare(
            "INSERT INTO player_round_combat
                (match_id, round_number, steamid, side, enemy_kills, deaths,
                 enemy_assists, enemy_health_damage, effective_enemy_health_damage, stable_team)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        for row in &m.player_round_combat {
            check_cancel()?;
            combat.execute(params![
                match_id,
                row.round_number,
                row.steamid,
                row.side,
                row.enemy_kills,
                row.deaths,
                row.enemy_assists,
                row.enemy_health_damage,
                row.effective_enemy_health_damage,
                row.stable_team
            ])?;
        }

        let mut hurts = tx.prepare(
            "INSERT INTO raw_combat_hurts (match_id, source_index, event_order, tick,
                round_number, weapon, attacker_steamid, attacker_side, victim_steamid,
                victim_side, raw_dmg_health, post_health, start_health, effective_health_damage)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        )?;
        for (index, row) in m.raw_combat_hurts.iter().enumerate() {
            check_cancel()?;
            hurts.execute(params![
                match_id,
                index as i64,
                row.event_order,
                row.tick,
                row.round,
                row.weapon,
                row.attacker_steamid,
                row.attacker_team,
                row.victim_steamid,
                row.victim_team,
                row.damage,
                row.post_health,
                row.start_health,
                row.effective_damage
            ])?;
        }
        let mut deaths = tx.prepare(
            "INSERT INTO raw_combat_deaths (match_id, source_index, event_order, tick,
                round_number, weapon, attacker_steamid, attacker_side, victim_steamid,
                victim_side, assister_steamid, assister_side)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        )?;
        for (index, row) in m.raw_combat_deaths.iter().enumerate() {
            check_cancel()?;
            deaths.execute(params![
                match_id,
                index as i64,
                row.event_order,
                row.tick,
                row.round,
                row.weapon,
                row.attacker_steamid,
                row.attacker_team,
                row.victim_steamid,
                row.victim_team,
                row.assister_steamid,
                row.assister_team
            ])?;
        }

        let winners: HashMap<i64, Option<String>> = m
            .rounds
            .iter()
            .map(|r| (r.round_number, r.winner.clone()))
            .collect();

        let mut stmt = tx.prepare(
            "INSERT INTO utility_events (
                match_id, round_number, utility_type, thrower_steamid, thrower_name, thrower_team,
                observer_slot, throw_tick, throw_time, throw_x, throw_y, throw_z,
                detonate_tick, detonate_x, detonate_y, detonate_z,
                land_x, land_y, land_z, trajectory, bounces,
                blind_enemy_duration, blind_team_duration, enemies_blinded, teammates_blinded,
                damage_enemy, damage_team, kills, flash_assists, nearby_players, round_winner,
                projectile_entity_id, blind_attribution_complete, damage_attribution_complete,
                kill_attribution_complete, assist_attribution_complete
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,
                       ?21,?22,?23,?24,?25,?26,?27,?28,?29,?30,?31,?32,?33,?34,?35,?36)",
        )?;
        let mut event_ids = Vec::with_capacity(m.events.len());
        for e in &m.events {
            check_cancel()?;
            stmt.execute(params![
                match_id,
                e.round_number,
                e.utility_type.as_str(),
                e.thrower_steamid,
                e.thrower_name,
                e.thrower_team,
                e.observer_slot,
                e.throw_tick,
                e.throw_time,
                e.throw_pos.x,
                e.throw_pos.y,
                e.throw_pos.z,
                e.detonate_tick,
                e.detonate_pos.map(|p| p.x),
                e.detonate_pos.map(|p| p.y),
                e.detonate_pos.map(|p| p.z),
                e.land_pos.map(|p| p.x),
                e.land_pos.map(|p| p.y),
                e.land_pos.map(|p| p.z),
                serde_json::to_string(&e.trajectory)?,
                serde_json::to_string(&e.bounces)?,
                e.blind_enemy_duration,
                e.blind_team_duration,
                e.enemies_blinded,
                e.teammates_blinded,
                e.damage_enemy,
                e.damage_team,
                e.kills,
                e.flash_assists,
                serde_json::to_string(&e.nearby_players)?,
                winners.get(&e.round_number).cloned().flatten(),
                e.projectile_entity_id,
                e.blind_attribution_complete,
                e.damage_attribution_complete,
                e.kill_attribution_complete,
                e.assist_attribution_complete,
            ])?;
            event_ids.push(tx.last_insert_rowid());
        }
        let mut facts = tx.prepare(
            "INSERT INTO raw_effect_facts (match_id, fact_kind, source_index, raw_json,
                resolution, assigned_event_id, candidate_event_ids)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for fact in &m.effect_facts {
            check_cancel()?;
            let assigned = fact.assigned_event.map(|index| event_ids[index]);
            let candidates: Vec<i64> = fact
                .candidate_events
                .iter()
                .map(|&index| event_ids[index])
                .collect();
            facts.execute(params![
                match_id,
                fact.kind,
                fact.source_index as i64,
                fact.raw_json.to_string(),
                fact.resolution,
                assigned,
                serde_json::to_string(&candidates)?
            ])?;
        }
    }

    if m.position_stream_available {
        // Assign positions from absolute demo ticks after final round-boundary
        // reconciliation. A projectile can outlive its release round; its
        // frame belongs to the round active at that tick.
        let round_at = |tick: i64| {
            m.rounds.iter().rev().find(|round| {
            matches!((round.start_tick, round.end_tick), (Some(start), Some(end)) if start <= tick && tick <= end)
        }).map(|round| round.round_number)
        };
        {
            let mut insert = tx.prepare(
                "INSERT INTO viewer_bomb_events
                 (match_id,source_index,round_number,kind,tick,actor_steamid,actor_name,site_id,x,y,z)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            )?;
            for (index, event) in m.bomb_events.iter().enumerate() {
                check_cancel()?;
                let Some(round_number) = viewer_observation_round(&m.rounds, event.tick) else {
                    continue;
                };
                insert.execute(params![
                    match_id,
                    index as i64,
                    round_number,
                    event.kind,
                    event.tick,
                    event.actor_steamid,
                    event.actor_name,
                    event.site_id,
                    event.x,
                    event.y,
                    event.z,
                ])?;
            }
        }
        if m.bomb_event_stream_available {
            tx.execute(
                "UPDATE matches SET viewer_bomb_version=1 WHERE id=?1",
                [match_id],
            )?;
        }
        let mut player_rows = 0_i64;
        {
            let mut insert = tx.prepare(
                "INSERT OR REPLACE INTO viewer_player_positions
                (match_id,round_number,tick,steamid,name,side,x,y,z,is_alive,health,yaw)
                VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            )?;
            for position in &m.viewer_positions {
                check_cancel()?;
                let (Some(tick), Some(steamid), Some(name), Some(x), Some(y), Some(z)) = (
                    position.tick,
                    position.steamid.as_deref(),
                    position.name.as_deref(),
                    position.x,
                    position.y,
                    position.z,
                ) else {
                    continue;
                };
                let Some(round_number) = round_at(tick) else {
                    continue;
                };
                insert.execute(params![
                    match_id,
                    round_number,
                    tick,
                    steamid,
                    name,
                    position.team,
                    x,
                    y,
                    z,
                    position.is_alive,
                    position.health,
                    position.yaw
                ])?;
                player_rows += 1;
            }
        }
        {
            let mut insert = tx.prepare("INSERT OR REPLACE INTO viewer_grenade_positions
                (match_id,round_number,tick,trajectory_index,entity_id,grenade_type,thrower_steamid,x,y,z)
                VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)")?;
            for (index, trajectory) in m.viewer_trajectories.iter().enumerate() {
                for point in &trajectory.points {
                    check_cancel()?;
                    let tick = point[0] as i64;
                    let Some(round_number) = viewer_observation_round(&m.rounds, tick) else {
                        continue;
                    };
                    insert.execute(params![
                        match_id,
                        round_number,
                        tick,
                        index as i64,
                        trajectory.entity_id,
                        trajectory.grenade_type,
                        trajectory.steamid,
                        point[1],
                        point[2],
                        point[3]
                    ])?;
                }
            }
        }
        if player_rows > 0 {
            tx.execute(
                "UPDATE matches SET viewer_position_version=1 WHERE id=?1",
                [match_id],
            )?;
        }
    }

    check_cancel()?;
    begin_commit()?;
    tx.commit()?;
    Ok(match_id)
}

pub fn list_matches(conn: &Connection) -> AppResult<Vec<MatchSummary>> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.file_name, m.file_path, m.map_name, m.tick_rate, m.rounds_count,
                m.score_ct, m.score_t, m.imported_at, m.parser_capability,
                (SELECT COUNT(*) FROM utility_events e WHERE e.match_id = m.id),
                (SELECT COUNT(*) FROM raw_effect_facts f WHERE f.match_id = m.id),
                (SELECT COUNT(*) FROM raw_effect_facts f WHERE f.match_id = m.id AND f.resolution = 'ambiguous'),
                (SELECT COUNT(*) FROM raw_effect_facts f WHERE f.match_id = m.id AND f.resolution = 'unresolved'),
                m.effect_attribution_version
         FROM matches m ORDER BY m.imported_at DESC",
    )?;
    let mut matches: Vec<MatchSummary> = stmt
        .query_map([], |row| {
            Ok(MatchSummary {
                id: row.get(0)?,
                file_name: row.get(1)?,
                file_path: row.get(2)?,
                map_name: row.get(3)?,
                tick_rate: row.get(4)?,
                rounds_count: row.get(5)?,
                score_ct: row.get(6)?,
                score_t: row.get(7)?,
                imported_at: row.get(8)?,
                parser_capability: ParserCapabilities::from_key(&row.get::<_, String>(9)?)
                    .ok_or(rusqlite::Error::InvalidQuery)?,
                event_count: row.get(10)?,
                effect_fact_count: row.get(11)?,
                ambiguous_effect_count: row.get(12)?,
                unresolved_effect_count: row.get(13)?,
                effect_attribution_version: row.get(14)?,
                players: Vec::new(),
            })
        })?
        .collect::<Result<_, _>>()?;

    let mut pstmt =
        conn.prepare("SELECT steamid, name FROM players WHERE match_id = ?1 ORDER BY name")?;
    for m in &mut matches {
        m.players = pstmt
            .query_map(params![m.id], |row| {
                Ok(PlayerRef {
                    steamid: row.get(0)?,
                    name: row.get(1)?,
                })
            })?
            .collect::<Result<_, _>>()?;
    }
    Ok(matches)
}

pub fn delete_match(conn: &Connection, match_id: i64) -> AppResult<()> {
    let n = conn.execute("DELETE FROM matches WHERE id = ?1", params![match_id])?;
    if n == 0 {
        return Err(AppError::NotFound(format!("match {match_id}")));
    }
    Ok(())
}

/// Lossless effect source rows, including those whose grenade is ambiguous or unresolved.
pub fn list_raw_effect_facts(conn: &Connection, match_id: i64) -> AppResult<Vec<RawEffectFact>> {
    let mut stmt = conn.prepare(
        "SELECT fact_kind, source_index, raw_json, resolution,
        assigned_event_id, candidate_event_ids FROM raw_effect_facts
        WHERE match_id = ?1 ORDER BY fact_kind, source_index",
    )?;
    let rows = stmt.query_map(params![match_id], |row| {
        let raw_json: String = row.get(2)?;
        let candidates: String = row.get(5)?;
        Ok(RawEffectFact {
            kind: row.get(0)?,
            source_index: row.get(1)?,
            raw_json: serde_json::from_str(&raw_json).map_err(|_| rusqlite::Error::InvalidQuery)?,
            resolution: row.get(3)?,
            assigned_event_id: row.get(4)?,
            candidate_event_ids: serde_json::from_str(&candidates)
                .map_err(|_| rusqlite::Error::InvalidQuery)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

// ---------------------------------------------------------------- events

const SUMMARY_COLS: &str = "e.id, e.match_id, m.map_name, e.round_number, e.utility_type,
    e.thrower_steamid, e.thrower_name, e.thrower_team, e.throw_tick, e.throw_time,
    e.throw_x, e.throw_y, e.throw_z, e.detonate_x, e.detonate_y, e.detonate_z,
    v.blind_enemy_duration, v.damage_enemy, v.kills, e.favorite, e.round_winner,
    e.trajectory, m.parser_capability, m.effect_attribution_version,
    e.blind_attribution_complete, e.damage_attribution_complete,
    e.kill_attribution_complete, e.assist_attribution_complete";

fn summary_from_row(row: &Row) -> rusqlite::Result<UtilityEventSummary> {
    let dx: Option<f64> = row.get(13)?;
    let dy: Option<f64> = row.get(14)?;
    let dz: Option<f64> = row.get(15)?;
    let utility: String = row.get(4)?;
    let trajectory_json: String = row.get(21)?;
    let has_trajectory = serde_json::from_str::<Vec<TrajPoint>>(&trajectory_json)
        .map(|points| points.len() >= 2)
        .unwrap_or(false);
    let parser_capability = ParserCapabilities::from_key(&row.get::<_, String>(22)?)
        .ok_or(rusqlite::Error::InvalidQuery)?;
    let attribution_version: Option<i64> = row.get(23)?;
    let complete = (24..=27)
        .map(|index| row.get::<_, i64>(index))
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .all(|value| value != 0);
    let effect_attribution_status = if !parser_capability.supports_combat() {
        "unsupported"
    } else if attribution_version != Some(2) {
        "legacy-unverified"
    } else if !complete {
        "unavailable"
    } else {
        "complete"
    };
    Ok(UtilityEventSummary {
        id: row.get(0)?,
        match_id: row.get(1)?,
        parser_capability,
        map_name: row.get(2)?,
        round_number: row.get(3)?,
        utility_type: UtilityType::from_str(&utility).unwrap_or(UtilityType::Smoke),
        thrower_steamid: row.get(5)?,
        thrower_name: row.get(6)?,
        thrower_team: row.get(7)?,
        throw_tick: row.get(8)?,
        throw_time: row.get(9)?,
        throw_pos: Vec3 {
            x: row.get(10)?,
            y: row.get(11)?,
            z: row.get(12)?,
        },
        detonate_pos: match (dx, dy) {
            (Some(x), Some(y)) => Some(Vec3 {
                x,
                y,
                z: dz.unwrap_or(0.0),
            }),
            _ => None,
        },
        blind_enemy_duration: row.get(16)?,
        damage_enemy: row.get(17)?,
        kills: row.get(18)?,
        effect_attribution_status: effect_attribution_status.into(),
        favorite: row.get::<_, i64>(19)? != 0,
        round_winner: row.get(20)?,
        has_trajectory,
        tags: Vec::new(),
    })
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}

pub fn query_events(conn: &Connection, f: &EventFilter) -> AppResult<Vec<UtilityEventSummary>> {
    let mut sql =
        format!("SELECT {SUMMARY_COLS} FROM utility_events e JOIN matches m ON m.id = e.match_id JOIN utility_events_supported v ON v.id = e.id");
    let mut wheres: Vec<String> = Vec::new();
    let mut args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(cid) = f.collection_id {
        sql.push_str(" JOIN collection_events ce ON ce.event_id = e.id");
        wheres.push("ce.collection_id = ?".into());
        args.push(Box::new(cid));
    }
    if let Some(ids) = &f.match_ids {
        if !ids.is_empty() {
            wheres.push(format!("e.match_id IN ({})", placeholders(ids.len())));
            for id in ids {
                args.push(Box::new(*id));
            }
        }
    }
    if let Some(maps) = &f.maps {
        if !maps.is_empty() {
            wheres.push(format!("m.map_name IN ({})", placeholders(maps.len())));
            for m in maps {
                args.push(Box::new(m.clone()));
            }
        }
    }
    if let Some(types) = &f.utility_types {
        if !types.is_empty() {
            wheres.push(format!("e.utility_type IN ({})", placeholders(types.len())));
            for t in types {
                args.push(Box::new(t.as_str().to_string()));
            }
        }
    }
    if let Some(ids) = &f.thrower_steamids {
        if !ids.is_empty() {
            wheres.push(format!(
                "e.thrower_steamid IN ({})",
                placeholders(ids.len())
            ));
            for id in ids {
                args.push(Box::new(id.clone()));
            }
        }
    }
    if let Some(teams) = &f.teams {
        if !teams.is_empty() {
            wheres.push(format!("e.thrower_team IN ({})", placeholders(teams.len())));
            for t in teams {
                args.push(Box::new(t.clone()));
            }
        }
    }
    if let Some(rounds) = &f.rounds {
        if !rounds.is_empty() {
            wheres.push(format!(
                "e.round_number IN ({})",
                placeholders(rounds.len())
            ));
            for r in rounds {
                args.push(Box::new(*r));
            }
        }
    }
    if let Some(tag_ids) = &f.tag_ids {
        if !tag_ids.is_empty() {
            wheres.push(format!(
                "e.id IN (SELECT event_id FROM event_tags WHERE tag_id IN ({}))",
                placeholders(tag_ids.len())
            ));
            for id in tag_ids {
                args.push(Box::new(*id));
            }
        }
    }
    if f.favorites_only == Some(true) {
        wheres.push("e.favorite = 1".into());
    }
    if let Some(search) = &f.search {
        let s = search.trim();
        if !s.is_empty() {
            wheres.push(
                "(e.thrower_name LIKE ? OR m.map_name LIKE ? OR e.utility_type LIKE ? OR e.id IN \
                 (SELECT et.event_id FROM event_tags et JOIN tags t ON t.id = et.tag_id WHERE t.name LIKE ?))"
                    .into(),
            );
            let like = format!("%{s}%");
            for _ in 0..4 {
                args.push(Box::new(like.clone()));
            }
        }
    }

    if !wheres.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&wheres.join(" AND "));
    }
    sql.push_str(" ORDER BY e.match_id DESC, e.round_number, e.throw_tick");
    sql.push_str(&format!(
        " LIMIT {}",
        f.limit.unwrap_or(5000).clamp(1, 50_000)
    ));
    if let Some(offset) = f.offset {
        sql.push_str(&format!(" OFFSET {}", offset.max(0)));
    }

    let mut stmt = conn.prepare(&sql)?;
    let mut events: Vec<UtilityEventSummary> = stmt
        .query_map(
            params_from_iter(args.iter().map(|a| a.as_ref())),
            summary_from_row,
        )?
        .collect::<Result<_, _>>()?;

    attach_tags(conn, &mut events)?;
    Ok(events)
}

fn attach_tags(conn: &Connection, events: &mut [UtilityEventSummary]) -> AppResult<()> {
    if events.is_empty() {
        return Ok(());
    }
    let mut stmt = conn
        .prepare("SELECT et.event_id, t.name FROM event_tags et JOIN tags t ON t.id = et.tag_id")?;
    let mut by_event: HashMap<i64, Vec<String>> = HashMap::new();
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (id, name) = row?;
        by_event.entry(id).or_default().push(name);
    }
    for e in events.iter_mut() {
        if let Some(tags) = by_event.remove(&e.id) {
            e.tags = tags;
        }
    }
    Ok(())
}

pub fn get_event_detail(conn: &Connection, event_id: i64) -> AppResult<UtilityEventDetail> {
    let sql = format!(
        "SELECT {SUMMARY_COLS}, e.detonate_tick, e.land_x, e.land_y, e.land_z,
                e.trajectory, e.bounces, v.blind_team_duration, v.enemies_blinded,
                v.teammates_blinded, v.damage_team, v.flash_assists, e.nearby_players,
                r.end_reason, r.ct_equip_value, r.t_equip_value, m.tick_rate,
                m.file_path, e.observer_slot
         FROM utility_events e
         JOIN matches m ON m.id = e.match_id
         JOIN utility_events_supported v ON v.id = e.id
         LEFT JOIN rounds r ON r.match_id = e.match_id AND r.round_number = e.round_number
         WHERE e.id = ?1"
    );
    let mut detail = conn
        .query_row(&sql, params![event_id], |row| {
            let summary = summary_from_row(row)?;
            let economy = summary.parser_capability.supports_economy();
            let lx: Option<f64> = row.get(29)?;
            let ly: Option<f64> = row.get(30)?;
            let lz: Option<f64> = row.get(31)?;
            let trajectory: String = row.get(32)?;
            let bounces: String = row.get(33)?;
            let nearby: String = row.get(39)?;
            Ok(UtilityEventDetail {
                summary,
                detonate_tick: row.get(28)?,
                land_pos: match (lx, ly) {
                    (Some(x), Some(y)) => Some(Vec3 {
                        x,
                        y,
                        z: lz.unwrap_or(0.0),
                    }),
                    _ => None,
                },
                trajectory: serde_json::from_str(&trajectory).unwrap_or_default(),
                bounces: serde_json::from_str(&bounces).unwrap_or_default(),
                blind_team_duration: row.get(34)?,
                enemies_blinded: row.get(35)?,
                teammates_blinded: row.get(36)?,
                damage_team: row.get(37)?,
                flash_assists: row.get(38)?,
                nearby_players: serde_json::from_str(&nearby).unwrap_or_default(),
                round_end_reason: row.get(40)?,
                ct_equip_value: if economy { row.get(41)? } else { None },
                t_equip_value: if economy { row.get(42)? } else { None },
                tick_rate: row.get(43)?,
                demo_path: row.get(44)?,
                observer_slot: row.get(45)?,
            })
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::NotFound(format!("event {event_id}")),
            other => AppError::Db(other),
        })?;

    let mut events = vec![detail.summary.clone()];
    attach_tags(conn, &mut events)?;
    detail.summary = events.pop().unwrap();
    Ok(detail)
}

pub fn set_favorite(conn: &Connection, event_id: i64, favorite: bool) -> AppResult<()> {
    let n = conn.execute(
        "UPDATE utility_events SET favorite = ?2 WHERE id = ?1",
        params![event_id, favorite as i64],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!("event {event_id}")));
    }
    Ok(())
}

// ---------------------------------------------------------------- tags

pub fn list_tags(conn: &Connection) -> AppResult<Vec<Tag>> {
    let mut stmt = conn.prepare("SELECT id, name FROM tags ORDER BY name")?;
    let tags = stmt
        .query_map([], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(tags)
}

pub fn tag_event(conn: &Connection, event_id: i64, tag_name: &str) -> AppResult<Tag> {
    let name = tag_name.trim();
    if name.is_empty() || name.len() > 64 {
        return Err(AppError::Invalid("tag must be 1–64 characters".into()));
    }
    conn.execute(
        "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
        params![name],
    )?;
    let tag = conn.query_row(
        "SELECT id, name FROM tags WHERE name = ?1 COLLATE NOCASE",
        params![name],
        |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        },
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO event_tags (event_id, tag_id) VALUES (?1, ?2)",
        params![event_id, tag.id],
    )?;
    Ok(tag)
}

pub fn untag_event(conn: &Connection, event_id: i64, tag_id: i64) -> AppResult<()> {
    conn.execute(
        "DELETE FROM event_tags WHERE event_id = ?1 AND tag_id = ?2",
        params![event_id, tag_id],
    )?;
    // Prune orphan tags so the tag list stays tidy.
    conn.execute(
        "DELETE FROM tags WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM event_tags WHERE tag_id = ?1)",
        params![tag_id],
    )?;
    Ok(())
}

// ---------------------------------------------------------------- collections

pub fn list_collections(conn: &Connection) -> AppResult<Vec<Collection>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.name, c.created_at,
                (SELECT COUNT(*) FROM collection_events ce WHERE ce.collection_id = c.id)
         FROM collections c ORDER BY c.created_at DESC",
    )?;
    let cols = stmt
        .query_map([], |row| {
            Ok(Collection {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                event_count: row.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(cols)
}

pub fn create_collection(conn: &Connection, name: &str) -> AppResult<Collection> {
    let name = name.trim();
    if name.is_empty() || name.len() > 128 {
        return Err(AppError::Invalid(
            "collection name must be 1–128 characters".into(),
        ));
    }
    conn.execute(
        "INSERT INTO collections (name, created_at) VALUES (?1, ?2)",
        params![name, now_iso()],
    )?;
    Ok(Collection {
        id: conn.last_insert_rowid(),
        name: name.to_string(),
        created_at: now_iso(),
        event_count: 0,
    })
}

pub fn delete_collection(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM collections WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn add_to_collection(
    conn: &Connection,
    collection_id: i64,
    event_ids: &[i64],
) -> AppResult<()> {
    let mut stmt = conn.prepare(
        "INSERT OR IGNORE INTO collection_events (collection_id, event_id, position)
         VALUES (?1, ?2, (SELECT COALESCE(MAX(position), 0) + 1 FROM collection_events WHERE collection_id = ?1))",
    )?;
    for id in event_ids {
        stmt.execute(params![collection_id, id])?;
    }
    Ok(())
}

pub fn remove_from_collection(
    conn: &Connection,
    collection_id: i64,
    event_id: i64,
) -> AppResult<()> {
    conn.execute(
        "DELETE FROM collection_events WHERE collection_id = ?1 AND event_id = ?2",
        params![collection_id, event_id],
    )?;
    Ok(())
}

// ---------------------------------------------------------------- render jobs

#[derive(Clone)]
struct UtilitySetRow {
    id: i64,
    match_id: i64,
    round_number: i64,
    thrower_steamid: String,
    throw_tick: i64,
    tick_rate: f64,
}

/// Expand selected throws to the complete same-player utility bursts that
/// contain them. This lets a smoke-only selection pull in a second dropped
/// smoke, flash, HE, or molotov without mixing in a teammate's utility.
pub fn expand_utility_set_event_ids(
    conn: &Connection,
    event_ids: &[i64],
    window_seconds: f64,
) -> AppResult<Vec<i64>> {
    if event_ids.is_empty() {
        return Err(AppError::Invalid(
            "select at least one event to render".into(),
        ));
    }
    let selected: HashSet<i64> = event_ids.iter().copied().collect();
    let mut statement = conn.prepare(
        "SELECT e.id, e.match_id, e.round_number, e.thrower_steamid, e.throw_tick, m.tick_rate
         FROM utility_events e
         JOIN matches m ON m.id = e.match_id
         ORDER BY e.match_id, e.round_number, e.thrower_steamid, e.throw_tick, e.id",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok(UtilitySetRow {
                id: row.get(0)?,
                match_id: row.get(1)?,
                round_number: row.get(2)?,
                thrower_steamid: row.get(3)?,
                throw_tick: row.get(4)?,
                tick_rate: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let found: HashSet<i64> = rows
        .iter()
        .filter_map(|row| selected.contains(&row.id).then_some(row.id))
        .collect();
    if let Some(missing) = selected.difference(&found).next() {
        return Err(AppError::NotFound(format!("event {missing}")));
    }

    let mut by_player_round: HashMap<(i64, i64, String), Vec<UtilitySetRow>> = HashMap::new();
    for row in rows {
        by_player_round
            .entry((row.match_id, row.round_number, row.thrower_steamid.clone()))
            .or_default()
            .push(row);
    }

    let window_seconds = window_seconds.max(0.0);
    let mut expanded = Vec::new();
    for rows in by_player_round.values() {
        let mut start = 0;
        for end in 1..=rows.len() {
            let split = end == rows.len()
                || (rows[end].throw_tick - rows[end - 1].throw_tick).max(0) as f64
                    / rows[end].tick_rate.max(1.0)
                    > window_seconds;
            if !split {
                continue;
            }
            let set = &rows[start..end];
            if set.iter().any(|row| selected.contains(&row.id)) {
                expanded.extend(set.iter().cloned());
            }
            start = end;
        }
    }
    expanded.sort_by_key(|row| (row.match_id, row.round_number, row.throw_tick, row.id));
    Ok(expanded.into_iter().map(|row| row.id).collect())
}

pub fn validate_render_events(conn: &Connection, event_ids: &[i64]) -> AppResult<()> {
    if event_ids.is_empty() {
        return Err(AppError::Invalid(
            "select at least one event to render".into(),
        ));
    }
    for event_id in event_ids {
        let observer_slot: Option<i64> = conn
            .query_row(
                "SELECT observer_slot FROM utility_events WHERE id = ?1",
                params![event_id],
                |row| row.get(0),
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => {
                    AppError::NotFound(format!("event {event_id}"))
                }
                other => AppError::Db(other),
            })?;
        if observer_slot.is_none() {
            return Err(AppError::Invalid(format!(
                "event {event_id} was imported before in-game capture support; re-import its demo once to render the player POV"
            )));
        }
    }
    Ok(())
}

pub fn create_render_job(
    conn: &mut Connection,
    event_ids: &[i64],
    settings: &RenderSettings,
    output_dir: &str,
) -> AppResult<i64> {
    validate_render_events(conn, event_ids)?;
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO render_jobs (status, progress, settings, output_dir, created_at)
         VALUES ('queued', 0, ?1, ?2, ?3)",
        params![serde_json::to_string(settings)?, output_dir, now_iso()],
    )?;
    let job_id = tx.last_insert_rowid();
    {
        let mut stmt = tx.prepare(
            "INSERT INTO render_job_events (job_id, event_id, position) VALUES (?1, ?2, ?3)",
        )?;
        for (i, id) in event_ids.iter().enumerate() {
            stmt.execute(params![job_id, id, i as i64])?;
        }
    }
    tx.commit()?;
    Ok(job_id)
}

fn query_render_jobs(conn: &Connection, query: &str) -> AppResult<Vec<RenderJob>> {
    let mut stmt = conn.prepare(query)?;
    let mut jobs: Vec<RenderJob> = stmt
        .query_map([], |row| {
            let settings: String = row.get(3)?;
            Ok(RenderJob {
                id: row.get(0)?,
                status: row.get(1)?,
                progress: row.get(2)?,
                settings: serde_json::from_str(&settings).unwrap_or_default(),
                event_ids: Vec::new(),
                output_dir: row.get(4)?,
                error: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    let mut estmt =
        conn.prepare("SELECT event_id FROM render_job_events WHERE job_id = ?1 ORDER BY position")?;
    for job in &mut jobs {
        job.event_ids = estmt
            .query_map(params![job.id], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
    }
    Ok(jobs)
}

pub fn list_render_jobs(conn: &Connection) -> AppResult<Vec<RenderJob>> {
    query_render_jobs(
        conn,
        "SELECT id, status, progress, settings, output_dir, error, created_at
         FROM render_jobs ORDER BY id DESC LIMIT 100",
    )
}

pub fn recover_render_jobs(conn: &Connection) -> AppResult<Vec<RenderJob>> {
    conn.execute(
        "UPDATE render_jobs SET status = 'queued', progress = 0, error = NULL
         WHERE status = 'running'",
        [],
    )?;
    query_render_jobs(
        conn,
        "SELECT id, status, progress, settings, output_dir, error, created_at
         FROM render_jobs WHERE status = 'queued' ORDER BY id ASC",
    )
}

pub fn update_render_job(
    conn: &Connection,
    job_id: i64,
    status: &str,
    progress: f64,
    error: Option<&str>,
) -> AppResult<()> {
    conn.execute(
        "UPDATE render_jobs SET status = ?2, progress = ?3, error = ?4 WHERE id = ?1",
        params![job_id, status, progress, error],
    )?;
    Ok(())
}

/// Atomically claim a queued render job for one worker. A duplicate queue
/// delivery (or a second app instance) must not render and overwrite a job
/// that is already running or completed.
pub fn claim_render_job(conn: &Connection, job_id: i64) -> AppResult<bool> {
    Ok(conn.execute(
        "UPDATE render_jobs
         SET status = 'running', progress = 0, error = NULL
         WHERE id = ?1 AND status = 'queued'",
        params![job_id],
    )? == 1)
}

pub fn fail_render_job(conn: &Connection, job_id: i64, error: &str) -> AppResult<f64> {
    let progress = conn
        .query_row(
            "SELECT progress FROM render_jobs WHERE id = ?1",
            params![job_id],
            |row| row.get(0),
        )
        .map_err(|db_error| match db_error {
            rusqlite::Error::QueryReturnedNoRows => {
                AppError::NotFound(format!("render job {job_id}"))
            }
            other => AppError::Db(other),
        })?;
    update_render_job(conn, job_id, "failed", progress, Some(error))?;
    Ok(progress)
}

pub fn render_job_status(conn: &Connection, job_id: i64) -> AppResult<String> {
    conn.query_row(
        "SELECT status FROM render_jobs WHERE id = ?1",
        params![job_id],
        |r| r.get(0),
    )
    .map_err(|_| AppError::NotFound(format!("render job {job_id}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{AnalyzedEvent, AnalyzedPlayerRoundCombat, AnalyzedRound};
    use crate::db::open_in_memory;
    use crate::parser::raw::{RawPlayer, RawPosition, RawTrajectory};

    fn sample_match() -> AnalyzedMatch {
        AnalyzedMatch {
            map_name: "de_mirage".into(),
            tick_rate: 64.0,
            duration_ticks: 100_000,
            players: vec![RawPlayer {
                steamid: "s1".into(),
                name: "alice".into(),
            }],
            rounds: vec![AnalyzedRound {
                round_number: 1,
                start_tick: Some(1000),
                freeze_end_tick: Some(2000),
                end_tick: Some(8000),
                winner: Some("CT".into()),
                end_reason: Some("ct_win_elimination".into()),
                team_a_side: None,
                team_b_side: None,
                ct_equip_value: Some(4000),
                t_equip_value: Some(3000),
            }],
            events: vec![AnalyzedEvent {
                round_number: 1,
                utility_type: UtilityType::Smoke,
                thrower_steamid: "s1".into(),
                thrower_name: "alice".into(),
                thrower_team: "CT".into(),
                observer_slot: Some(2),
                throw_tick: 3000,
                throw_time: 15.6,
                throw_pos: Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                detonate_tick: Some(3200),
                detonate_pos: Some(Vec3 {
                    x: 100.0,
                    y: 50.0,
                    z: 0.0,
                }),
                land_pos: Some(Vec3 {
                    x: 100.0,
                    y: 50.0,
                    z: 0.0,
                }),
                trajectory: vec![[3000.0, 0.0, 0.0, 0.0], [3200.0, 100.0, 50.0, 0.0]],
                bounces: vec![],
                projectile_entity_id: Some(1),
                blind_enemy_duration: 0.0,
                blind_team_duration: 0.0,
                enemies_blinded: 0,
                teammates_blinded: 0,
                damage_enemy: 0,
                damage_team: 0,
                kills: 0,
                flash_assists: 0,
                blind_attribution_complete: true,
                damage_attribution_complete: true,
                kill_attribution_complete: true,
                assist_attribution_complete: true,
                nearby_players: vec![],
            }],
            effect_facts: vec![],
            player_round_combat: vec![AnalyzedPlayerRoundCombat {
                round_number: 1,
                steamid: "s1".into(),
                side: Some("CT".into()),
                enemy_kills: Some(2),
                deaths: 1,
                enemy_assists: Some(1),
                enemy_health_damage: Some(87),
                effective_enemy_health_damage: None,
                stable_team: None,
            }],
            raw_combat_hurts: vec![],
            raw_combat_deaths: vec![],
            bomb_events: vec![],
            bomb_event_stream_available: false,
            position_stream_available: false,
            viewer_positions: vec![],
            viewer_trajectories: vec![],
            score_ct: 13,
            score_t: 7,
        }
    }

    #[test]
    fn insert_query_roundtrip() {
        let mut conn = open_in_memory().unwrap();
        let mut analyzed = sample_match();
        analyzed.raw_combat_hurts = serde_json::from_value(serde_json::json!([{
            "eventOrder":1,"tick":3100,"round":1,"weapon":"ak47",
            "attackerSteamid":"s1","attackerTeam":"CT","victimSteamid":"s2",
            "victimTeam":"T","damage":15
        }]))
        .unwrap();
        analyzed.raw_combat_deaths = serde_json::from_value(serde_json::json!([{
            "eventOrder":2,"tick":3100,"round":1,"weapon":"ak47",
            "attackerSteamid":"s1","attackerTeam":"CT","victimSteamid":"s2",
            "victimTeam":"T","assisterSteamid":null,"assisterTeam":null
        }]))
        .unwrap();
        let id = insert_match(&mut conn, "C:/demos/test.dem", &analyzed).unwrap();
        assert!(id > 0);

        let matches = list_matches(&conn).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].event_count, 1);
        assert_eq!(matches[0].players[0].name, "alice");
        let combat: (String, i64, i64, i64) = conn
            .query_row(
                "SELECT side, enemy_kills, enemy_assists, enemy_health_damage
             FROM player_round_combat WHERE match_id = ?1 AND steamid = 's1'",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(combat, ("CT".into(), 2, 1, 87));
        let hurt: (i64, i64, String, String, i64) = conn
            .query_row(
                "SELECT event_order, tick, weapon, victim_steamid, raw_dmg_health
             FROM raw_combat_hurts WHERE match_id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(hurt, (1, 3100, "ak47".into(), "s2".into(), 15));
        let death: (i64, i64, String) = conn.query_row(
            "SELECT event_order, tick, victim_steamid FROM raw_combat_deaths WHERE match_id = ?1",
            [id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).unwrap();
        assert_eq!(death, (2, 3100, "s2".into()));

        let events = query_events(&conn, &EventFilter::default()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].round_winner.as_deref(), Some("CT"));
        assert!(events[0].has_trajectory);

        let detail = get_event_detail(&conn, events[0].id).unwrap();
        assert_eq!(detail.trajectory.len(), 2);
        assert_eq!(detail.ct_equip_value, Some(4000));
        assert_eq!(detail.demo_path, "C:/demos/test.dem");
        assert_eq!(detail.observer_slot, Some(2));
    }

    #[test]
    fn viewer_frames_keep_absolute_ticks_and_completed_round_boundaries() {
        let mut conn = open_in_memory().unwrap();
        let mut analyzed = sample_match();
        let mut next_round = analyzed.rounds[0].clone();
        next_round.round_number = 2;
        next_round.start_tick = Some(9000);
        next_round.freeze_end_tick = Some(9500);
        next_round.end_tick = Some(16000);
        analyzed.rounds.push(next_round);
        analyzed.position_stream_available = true;
        analyzed.viewer_positions = [999, 1000, 3000, 8000, 8001]
            .into_iter()
            .map(|tick| RawPosition {
                tick: Some(tick),
                steamid: Some("s1".into()),
                name: Some("alice".into()),
                team: Some("CT".into()),
                x: Some(tick as f64),
                y: Some(20.0),
                z: Some(30.0),
                is_alive: Some(true),
                health: Some(90),
                yaw: Some(45.0),
            })
            .collect();
        analyzed.viewer_trajectories = vec![RawTrajectory {
            entity_id: 7,
            grenade_type: Some("CSmokeGrenadeProjectile".into()),
            steamid: Some("s1".into()),
            name: Some("alice".into()),
            points: vec![[3000.0, 1.0, 2.0, 3.0], [8001.0, 4.0, 5.0, 6.0]],
        }];
        let id = insert_match(&mut conn, "C:/demos/viewer.dem", &analyzed).unwrap();
        let version: Option<i64> = conn
            .query_row(
                "SELECT viewer_position_version FROM matches WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .unwrap();
        let ticks: Vec<i64> = conn
            .prepare("SELECT tick FROM viewer_player_positions WHERE match_id=?1 ORDER BY tick")
            .unwrap()
            .query_map([id], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let grenade: Vec<(i64, i64, i64, String)> = conn
            .prepare("SELECT round_number,tick,entity_id,grenade_type FROM viewer_grenade_positions WHERE match_id=?1 ORDER BY tick")
            .unwrap()
            .query_map([id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(version, Some(1));
        assert_eq!(ticks, vec![1000, 3000, 8000]);
        assert_eq!(
            grenade,
            vec![
                (1, 3000, 7, "CSmokeGrenadeProjectile".into()),
                (1, 8001, 7, "CSmokeGrenadeProjectile".into()),
            ]
        );
    }

    #[test]
    fn cancellation_rolls_back_raw_combat_and_match_together() {
        let mut conn = open_in_memory().unwrap();
        let mut analyzed = sample_match();
        analyzed.raw_combat_hurts = serde_json::from_value(serde_json::json!([{
            "eventOrder":1,"tick":3100,"round":1,"weapon":"ak47",
            "attackerSteamid":"s1","attackerTeam":"CT","victimSteamid":"s2",
            "victimTeam":"T","damage":15
        }]))
        .unwrap();
        analyzed.raw_combat_deaths = serde_json::from_value(serde_json::json!([{
            "eventOrder":2,"tick":3100,"round":1,"weapon":"ak47",
            "attackerSteamid":"s1","attackerTeam":"CT","victimSteamid":"s2",
            "victimTeam":"T","assisterSteamid":null,"assisterTeam":null
        }]))
        .unwrap();
        let checks = std::cell::Cell::new(0);
        let result = insert_match_with_source_hash(
            &mut conn,
            "C:/cancelled.dem",
            &analyzed,
            "native-combat-full-v2",
            None,
            || {
                checks.set(checks.get() + 1);
                checks.get() >= 6
            },
            || Ok(()),
        );
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        for table in [
            "matches",
            "player_round_combat",
            "raw_combat_hurts",
            "raw_combat_deaths",
        ] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table} must roll back with the match");
        }
    }

    #[test]
    fn ambiguous_effect_is_preserved_and_masked_in_public_queries() {
        let mut conn = open_in_memory().unwrap();
        let mut analyzed = sample_match();
        analyzed.events[0].damage_enemy = 40;
        analyzed.events[0].damage_attribution_complete = false;
        analyzed
            .effect_facts
            .push(crate::analysis::EffectAttribution {
                kind: "hurt",
                source_index: 0,
                raw_json: serde_json::json!({"tick": 3201, "damage": 40}),
                resolution: "ambiguous",
                assigned_event: None,
                candidate_events: vec![0],
            });
        let match_id = insert_match_with_parser_capability(
            &mut conn,
            "C:/demos/ambiguous.dem",
            &analyzed,
            "native-combat-flash-v1",
            || false,
            || Ok(()),
        )
        .unwrap();
        let match_summary = list_matches(&conn).unwrap().remove(0);
        assert_eq!(
            (
                match_summary.effect_fact_count,
                match_summary.ambiguous_effect_count,
                match_summary.unresolved_effect_count
            ),
            (1, 1, 0)
        );
        let summary = query_events(&conn, &EventFilter::default())
            .unwrap()
            .remove(0);
        assert_eq!(summary.effect_attribution_status, "unavailable");
        assert_eq!(summary.damage_enemy, None);
        assert_eq!(summary.kills, Some(0));
        let detail = get_event_detail(&conn, summary.id).unwrap();
        assert_eq!(detail.damage_team, None);
        let facts = list_raw_effect_facts(&conn, match_id).unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].resolution, "ambiguous");
        assert_eq!(facts[0].raw_json["damage"], 40);
        assert_eq!(facts[0].candidate_event_ids, [summary.id]);
    }

    #[test]
    fn summary_requires_two_decoded_trajectory_points() {
        let mut conn = open_in_memory().unwrap();
        let mut analyzed = sample_match();
        analyzed.events[0].trajectory = vec![[3000.0, 0.0, 0.0, 0.0]];
        insert_match(&mut conn, "C:/demos/single-point.dem", &analyzed).unwrap();

        let events = query_events(&conn, &EventFilter::default()).unwrap();
        assert_eq!(events.len(), 1);
        assert!(!events[0].has_trajectory);
    }

    #[test]
    fn ordinary_reimport_preserves_match_event_ids_annotations_and_render_job_links() {
        let mut conn = open_in_memory().unwrap();
        let match_id = insert_match(&mut conn, "C:/demos/test.dem", &sample_match()).unwrap();
        let event_id = query_events(&conn, &EventFilter::default()).unwrap()[0].id;
        set_favorite(&conn, event_id, true).unwrap();
        tag_event(&conn, event_id, "approved").unwrap();
        let collection = create_collection(&conn, "First cut").unwrap();
        add_to_collection(&conn, collection.id, &[event_id]).unwrap();
        conn.execute(
            "INSERT INTO render_jobs (status, settings, output_dir, created_at)
            VALUES ('queued', '{}', 'C:/output', '2026-10-09')",
            [],
        )
        .unwrap();
        let job_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO render_job_events (job_id, event_id, position) VALUES (?1, ?2, 0)",
            params![job_id, event_id],
        )
        .unwrap();

        let error = insert_match(&mut conn, "C:/demos/test.dem", &sample_match()).unwrap_err();
        assert!(error.to_string().contains("reimport is blocked"));
        assert_eq!(list_matches(&conn).unwrap().len(), 1);
        assert_eq!(list_matches(&conn).unwrap()[0].id, match_id);
        let events = query_events(&conn, &EventFilter::default()).unwrap();
        assert_eq!(events[0].id, event_id);
        assert!(events[0].favorite);
        assert_eq!(events[0].tags, ["approved"]);
        assert_eq!(
            query_events(
                &conn,
                &EventFilter {
                    collection_id: Some(collection.id),
                    ..Default::default()
                }
            )
            .unwrap()
            .len(),
            1
        );
        let links: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM render_job_events WHERE job_id = ?1 AND event_id = ?2",
                params![job_id, event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(links, 1);
    }

    #[test]
    fn alias_of_legacy_demo_path_cannot_duplicate_match_or_annotations() {
        let dir = tempfile::tempdir().unwrap();
        let demo = dir.path().join("sample.dem");
        std::fs::write(&demo, b"PBDEMS2\0fixture").unwrap();
        let alias = dir.path().join(".").join("sample.dem");
        let mut conn = open_in_memory().unwrap();
        let id = insert_match(&mut conn, &demo.to_string_lossy(), &sample_match()).unwrap();
        let event_id = query_events(&conn, &EventFilter::default()).unwrap()[0].id;
        set_favorite(&conn, event_id, true).unwrap();
        tag_event(&conn, event_id, "keep").unwrap();
        conn.execute("INSERT INTO render_jobs (status, settings, output_dir, created_at) VALUES ('queued','{}','C:/output','2026-10-09')", []).unwrap();
        let job_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO render_job_events (job_id,event_id,position) VALUES (?1,?2,0)",
            params![job_id, event_id],
        )
        .unwrap();

        let error = insert_match(&mut conn, &alias.to_string_lossy(), &sample_match()).unwrap_err();
        assert!(error.to_string().contains("equivalent source path"));
        assert_eq!(
            list_matches(&conn)
                .unwrap()
                .iter()
                .map(|m| m.id)
                .collect::<Vec<_>>(),
            vec![id]
        );
        let events = query_events(&conn, &EventFilter::default()).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, event_id);
        assert!(events[0].favorite);
        assert_eq!(events[0].tags, ["keep"]);
        let links: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM render_job_events WHERE job_id=?1 AND event_id=?2",
                params![job_id, event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(links, 1);
    }

    #[test]
    fn filters_apply() {
        let mut conn = open_in_memory().unwrap();
        insert_match(&mut conn, "C:/demos/test.dem", &sample_match()).unwrap();

        let f = EventFilter {
            utility_types: Some(vec![UtilityType::Flash]),
            ..Default::default()
        };
        assert!(query_events(&conn, &f).unwrap().is_empty());

        let f = EventFilter {
            search: Some("alice".into()),
            ..Default::default()
        };
        assert_eq!(query_events(&conn, &f).unwrap().len(), 1);
    }

    #[test]
    fn tags_and_favorites() {
        let mut conn = open_in_memory().unwrap();
        insert_match(&mut conn, "C:/demos/test.dem", &sample_match()).unwrap();
        let ev = query_events(&conn, &EventFilter::default())
            .unwrap()
            .remove(0);

        set_favorite(&conn, ev.id, true).unwrap();
        let tag = tag_event(&conn, ev.id, "one-way").unwrap();
        let events = query_events(
            &conn,
            &EventFilter {
                favorites_only: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tags, vec!["one-way".to_string()]);

        untag_event(&conn, ev.id, tag.id).unwrap();
        assert!(list_tags(&conn).unwrap().is_empty());
    }

    #[test]
    fn collections_flow() {
        let mut conn = open_in_memory().unwrap();
        insert_match(&mut conn, "C:/demos/test.dem", &sample_match()).unwrap();
        let ev = query_events(&conn, &EventFilter::default())
            .unwrap()
            .remove(0);

        let col = create_collection(&conn, "A-site executes").unwrap();
        add_to_collection(&conn, col.id, &[ev.id]).unwrap();
        let f = EventFilter {
            collection_id: Some(col.id),
            ..Default::default()
        };
        assert_eq!(query_events(&conn, &f).unwrap().len(), 1);

        remove_from_collection(&conn, col.id, ev.id).unwrap();
        assert!(query_events(&conn, &f).unwrap().is_empty());
    }

    #[test]
    fn utility_set_expansion_includes_same_player_throws_inside_window() {
        let mut conn = open_in_memory().unwrap();
        let mut sample = sample_match();

        let mut second_smoke = sample.events[0].clone();
        second_smoke.throw_tick = 3128;
        second_smoke.throw_time = 17.6;
        let mut flash = sample.events[0].clone();
        flash.utility_type = UtilityType::Flash;
        flash.throw_tick = 3320;
        flash.throw_time = 20.6;
        let mut unrelated_late_throw = sample.events[0].clone();
        unrelated_late_throw.utility_type = UtilityType::Molotov;
        unrelated_late_throw.throw_tick = 4300;
        unrelated_late_throw.throw_time = 35.9;
        let mut teammate_throw = sample.events[0].clone();
        teammate_throw.thrower_steamid = "s2".into();
        teammate_throw.thrower_name = "bob".into();
        teammate_throw.observer_slot = Some(3);
        teammate_throw.throw_tick = 3064;
        teammate_throw.throw_time = 16.6;
        sample
            .events
            .extend([second_smoke, flash, unrelated_late_throw, teammate_throw]);

        insert_match(&mut conn, "C:/demos/util-set.dem", &sample).unwrap();
        let events = query_events(&conn, &EventFilter::default()).unwrap();
        let anchor = events
            .iter()
            .find(|event| event.throw_tick == 3000)
            .unwrap()
            .id;
        let expanded = expand_utility_set_event_ids(&conn, &[anchor], 12.0).unwrap();
        let ticks = expanded
            .iter()
            .map(|event_id| {
                get_event_detail(&conn, *event_id)
                    .unwrap()
                    .summary
                    .throw_tick
            })
            .collect::<Vec<_>>();

        assert_eq!(ticks, vec![3000, 3128, 3320]);
    }

    #[test]
    fn render_job_rejects_events_without_a_pov_slot() {
        let mut conn = open_in_memory().unwrap();
        let mut sample = sample_match();
        sample.events[0].observer_slot = None;
        insert_match(&mut conn, "C:/demos/old-import.dem", &sample).unwrap();
        let event_id = query_events(&conn, &EventFilter::default()).unwrap()[0].id;
        let error = create_render_job(
            &mut conn,
            &[event_id],
            &RenderSettings::default(),
            "C:/renders/one",
        )
        .unwrap_err();
        assert!(error.to_string().contains("re-import its demo"));
    }

    #[test]
    fn recovery_requeues_interrupted_jobs_in_creation_order() {
        let mut conn = open_in_memory().unwrap();
        insert_match(&mut conn, "C:/demos/test.dem", &sample_match()).unwrap();
        let event_id = query_events(&conn, &EventFilter::default()).unwrap()[0].id;
        let first = create_render_job(&mut conn, &[event_id], &RenderSettings::default(), "C:/r/1")
            .unwrap();
        let second =
            create_render_job(&mut conn, &[event_id], &RenderSettings::default(), "C:/r/2")
                .unwrap();
        update_render_job(&conn, first, "running", 0.5, None).unwrap();
        let jobs = recover_render_jobs(&conn).unwrap();
        assert_eq!(
            jobs.iter().map(|job| job.id).collect::<Vec<_>>(),
            vec![first, second]
        );
        assert!(jobs
            .iter()
            .all(|job| job.status == "queued" && job.progress == 0.0));
    }

    #[test]
    fn recovery_returns_more_than_one_hundred_queued_jobs() {
        let mut conn = open_in_memory().unwrap();
        insert_match(&mut conn, "C:/demos/test.dem", &sample_match()).unwrap();
        let event_id = query_events(&conn, &EventFilter::default()).unwrap()[0].id;
        let expected: Vec<_> = (0..101)
            .map(|index| {
                create_render_job(
                    &mut conn,
                    &[event_id],
                    &RenderSettings::default(),
                    &format!("C:/r/{index}"),
                )
                .unwrap()
            })
            .collect();

        let jobs = recover_render_jobs(&conn).unwrap();
        assert_eq!(jobs.iter().map(|job| job.id).collect::<Vec<_>>(), expected);
        assert!(jobs.iter().all(|job| job.event_ids == vec![event_id]));
    }

    #[test]
    fn render_job_can_only_be_claimed_once() {
        let mut conn = open_in_memory().unwrap();
        insert_match(&mut conn, "C:/demos/test.dem", &sample_match()).unwrap();
        let event_id = query_events(&conn, &EventFilter::default()).unwrap()[0].id;
        let job_id = create_render_job(
            &mut conn,
            &[event_id],
            &RenderSettings::default(),
            "C:/r/claimed-once",
        )
        .unwrap();

        assert!(claim_render_job(&conn, job_id).unwrap());
        assert!(!claim_render_job(&conn, job_id).unwrap());
        let job = list_render_jobs(&conn).unwrap().remove(0);
        assert_eq!(job.status, "running");
        assert_eq!(job.progress, 0.0);
    }

    #[test]
    fn failing_render_job_preserves_current_progress() {
        let mut conn = open_in_memory().unwrap();
        insert_match(&mut conn, "C:/demos/test.dem", &sample_match()).unwrap();
        let event_id = query_events(&conn, &EventFilter::default()).unwrap()[0].id;
        let job_id = create_render_job(
            &mut conn,
            &[event_id],
            &RenderSettings::default(),
            "C:/r/failed",
        )
        .unwrap();
        update_render_job(&conn, job_id, "running", 0.625, None).unwrap();

        let progress = fail_render_job(&conn, job_id, "encoder failed").unwrap();
        let job = list_render_jobs(&conn).unwrap().remove(0);
        assert_eq!(progress, 0.625);
        assert_eq!(job.status, "failed");
        assert_eq!(job.progress, 0.625);
        assert_eq!(job.error.as_deref(), Some("encoder failed"));
    }
}
