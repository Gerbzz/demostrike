//! Pure analysis: raw demo facts  domain utility events.
//! No IO, no DB - fully unit-testable. New analytics (executes, scoring,
//! heatmaps, lineups) belong in sibling modules of this crate path.

use crate::domain::{
    ImportSummary, NearbyPlayer, TrajPoint, UtilityPathCoverage, UtilityType, Vec3,
};
use crate::error::{AppError, AppResult};
use crate::parser::raw::*;
use std::collections::HashMap;

pub const DEFAULT_TICK_RATE: f64 = 64.0;
/// Players within this many units of a detonation are "nearby".
const NEARBY_RADIUS: f64 = 600.0;
/// Max ticks between weapon_fire and first trajectory sample to match.
const THROW_MATCH_WINDOW: i64 = 192;
/// Fire (molotov/incendiary) damage attribution window in seconds.
const FIRE_WINDOW_SECS: f64 = 8.0;
/// Flash assist attribution window in seconds.
const FLASH_ASSIST_WINDOW_SECS: f64 = 5.5;
/// Max stored trajectory points per event (downsampled beyond this).
const MAX_TRAJ_POINTS: usize = 256;
/// Minimum direction change (cosine) that counts as a bounce.
const BOUNCE_COS_THRESHOLD: f64 = 0.75;

#[derive(Debug, Clone)]
pub struct AnalyzedRound {
    pub round_number: i64,
    pub start_tick: Option<i64>,
    pub freeze_end_tick: Option<i64>,
    pub end_tick: Option<i64>,
    pub winner: Option<String>,
    pub end_reason: Option<String>,
    pub team_a_side: Option<String>,
    pub team_b_side: Option<String>,
    pub ct_equip_value: Option<i64>,
    pub t_equip_value: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AnalyzedEvent {
    pub round_number: i64,
    pub utility_type: UtilityType,
    pub thrower_steamid: String,
    pub thrower_name: String,
    pub thrower_team: String,
    pub observer_slot: Option<i64>,
    pub throw_tick: i64,
    pub throw_time: f64,
    pub throw_pos: Vec3,
    pub detonate_tick: Option<i64>,
    pub detonate_pos: Option<Vec3>,
    pub land_pos: Option<Vec3>,
    pub trajectory: Vec<TrajPoint>,
    pub bounces: Vec<TrajPoint>,
    pub projectile_entity_id: Option<i64>,
    pub blind_enemy_duration: f64,
    pub blind_team_duration: f64,
    pub enemies_blinded: i64,
    pub teammates_blinded: i64,
    pub damage_enemy: i64,
    pub damage_team: i64,
    pub kills: i64,
    pub flash_assists: i64,
    pub blind_attribution_complete: bool,
    pub damage_attribution_complete: bool,
    pub kill_attribution_complete: bool,
    pub assist_attribution_complete: bool,
    pub nearby_players: Vec<NearbyPlayer>,
}

#[derive(Debug, Clone)]
pub struct EffectAttribution {
    pub kind: &'static str,
    pub source_index: usize,
    pub raw_json: serde_json::Value,
    pub resolution: &'static str,
    pub assigned_event: Option<usize>,
    pub candidate_events: Vec<usize>,
}

/// Event-derived combat facts for one player in one round. The side is the
/// observed CT/T side, never an inferred stable CSDM team identity.
#[derive(Debug, Clone)]
pub struct AnalyzedPlayerRoundCombat {
    pub round_number: i64,
    pub steamid: String,
    pub side: Option<String>,
    pub stable_team: Option<String>,
    pub enemy_kills: Option<i64>,
    pub deaths: i64,
    pub enemy_assists: Option<i64>,
    /// Sum of raw player_hurt.dmg_health events. This can exceed available HP
    /// on lethal hits and is not CSDM's effective HealthDamageTaken value.
    pub enemy_health_damage: Option<i64>,
    /// Pinned analyzer effective HP, nullable for old or incomplete imports.
    pub effective_enemy_health_damage: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AnalyzedMatch {
    pub map_name: String,
    pub tick_rate: f64,
    pub duration_ticks: i64,
    pub players: Vec<RawPlayer>,
    pub rounds: Vec<AnalyzedRound>,
    pub events: Vec<AnalyzedEvent>,
    pub effect_facts: Vec<EffectAttribution>,
    pub player_round_combat: Vec<AnalyzedPlayerRoundCombat>,
    /// Lossless native combat facts retained for later timeline, duel, and
    /// weapon analysis without a destructive reimport.
    pub raw_combat_hurts: Vec<RawCombatHurt>,
    pub raw_combat_deaths: Vec<RawCombatDeath>,
    /// Source 2 bomb game events with nullable actor, site and position facts.
    pub bomb_events: Vec<RawBombEvent>,
    pub bomb_event_stream_available: bool,
    /// Absolute-tick frame-end positions and full projectile paths for 2D replay.
    /// A false capability keeps legacy and reference imports explicitly unavailable.
    pub position_stream_available: bool,
    pub viewer_positions: Vec<RawPosition>,
    pub viewer_trajectories: Vec<RawTrajectory>,
    pub score_ct: i64,
    pub score_t: i64,
}

pub fn build_match(raw: &RawDemo) -> AnalyzedMatch {
    let tick_rate = infer_tick_rate(&raw.header);
    let rounds = build_rounds(raw);
    let mut events = build_events(raw, &rounds, tick_rate);
    let effect_facts = attribute_effects(raw, &mut events, tick_rate);
    let player_round_combat = build_player_round_combat(raw, &rounds);
    let score_ct = rounds
        .iter()
        .filter(|r| r.winner.as_deref() == Some("CT"))
        .count() as i64;
    let score_t = rounds
        .iter()
        .filter(|r| r.winner.as_deref() == Some("T"))
        .count() as i64;

    AnalyzedMatch {
        map_name: raw.header.map_name.clone(),
        tick_rate,
        duration_ticks: raw.header.demo_ticks.unwrap_or(0),
        players: raw.players.clone(),
        rounds,
        events,
        effect_facts,
        player_round_combat,
        raw_combat_hurts: raw.combat_hurts.clone(),
        raw_combat_deaths: raw.combat_deaths.clone(),
        bomb_events: raw.bomb_events.clone(),
        bomb_event_stream_available: raw.bomb_event_stream_available,
        position_stream_available: raw.position_stream_available,
        viewer_positions: if raw.position_stream_available {
            raw.positions.clone()
        } else {
            Vec::new()
        },
        viewer_trajectories: if raw.position_stream_available {
            raw.trajectories.clone()
        } else {
            Vec::new()
        },
        score_ct,
        score_t,
    }
}

fn combat_row<'a>(
    rows: &'a mut HashMap<(i64, String), AnalyzedPlayerRoundCombat>,
    round: i64,
    steamid: &str,
    effective_available: bool,
) -> &'a mut AnalyzedPlayerRoundCombat {
    rows.entry((round, steamid.to_owned()))
        .or_insert_with(|| AnalyzedPlayerRoundCombat {
            round_number: round,
            steamid: steamid.to_owned(),
            side: None,
            stable_team: None,
            enemy_kills: Some(0),
            deaths: 0,
            enemy_assists: Some(0),
            enemy_health_damage: Some(0),
            effective_enemy_health_damage: effective_available.then_some(0),
        })
}

fn add_known(metric: &mut Option<i64>, value: i64) {
    if let Some(current) = metric {
        *current += value;
    }
}

fn observed_enemy(attacker: Option<&str>, victim: Option<&str>) -> Option<bool> {
    Some(normalize_team(attacker)? != normalize_team(victim)?)
}

// Source 2 can increment totalRoundsPlayed immediately after round_end. A
// death before the next round_start still belongs to the round just ended.
// Events after the final completed round have no match round and must not
// inflate the displayed scoreboard. Raw event rows remain lossless in v8.
fn completed_combat_round(round: i64, tick: i64, rounds: &[AnalyzedRound]) -> Option<i64> {
    let current = rounds.iter().find(|item| item.round_number == round)?;
    if current.start_tick.is_some_and(|start| tick < start) {
        return rounds
            .iter()
            .filter(|item| item.round_number < round)
            .filter(|item| item.end_tick.is_some_and(|end| end <= tick))
            .max_by_key(|item| item.end_tick)
            .map(|item| item.round_number);
    }
    Some(round)
}

fn build_player_round_combat(
    raw: &RawDemo,
    rounds: &[AnalyzedRound],
) -> Vec<AnalyzedPlayerRoundCombat> {
    let mut rows = HashMap::new();
    let mut conflicting_sides = std::collections::HashSet::new();
    for assignment in &raw.round_sides {
        if assignment.round < 1
            || assignment.steamid.is_empty()
            || !rounds
                .iter()
                .any(|round| round.round_number == assignment.round)
        {
            continue;
        }
        let key = (assignment.round, assignment.steamid.clone());
        let side = normalize_team(Some(&assignment.side));
        let row = combat_row(
            &mut rows,
            assignment.round,
            &assignment.steamid,
            raw.effective_health_damage_available,
        );
        if let Some(side) = side {
            if row.side.is_some() && row.side.as_ref() != Some(&side) {
                conflicting_sides.insert(key);
            } else if !conflicting_sides.contains(&key) {
                row.side = Some(side);
            }
        }
    }
    for hurt in &raw.combat_hurts {
        if hurt.round < 1 || hurt.damage < 0 {
            continue;
        }
        let Some(round) = completed_combat_round(hurt.round, hurt.tick, rounds) else {
            continue;
        };
        combat_row(
            &mut rows,
            round,
            &hurt.victim_steamid,
            raw.effective_health_damage_available,
        );
        if let Some(attacker) = hurt.attacker_steamid.as_deref() {
            let row = combat_row(
                &mut rows,
                round,
                attacker,
                raw.effective_health_damage_available,
            );
            if attacker != hurt.victim_steamid {
                match observed_enemy(hurt.attacker_team.as_deref(), hurt.victim_team.as_deref()) {
                    Some(true) => {
                        add_known(&mut row.enemy_health_damage, hurt.damage);
                        if raw.effective_health_damage_available {
                            match hurt.effective_damage {
                                Some(value) if value >= 0 => {
                                    add_known(&mut row.effective_enemy_health_damage, value)
                                }
                                _ => row.effective_enemy_health_damage = None,
                            }
                        }
                    }
                    Some(false) => {}
                    None => {
                        row.enemy_health_damage = None;
                        row.effective_enemy_health_damage = None;
                    }
                }
            }
        }
    }
    for death in &raw.combat_deaths {
        if death.round < 1 {
            continue;
        }
        let Some(round) = completed_combat_round(death.round, death.tick, rounds) else {
            continue;
        };
        // The original CSDM player scoreboard excludes a world-weapon
        // suicide caused by client disconnection, while its kill ledger
        // retains the event. Keep the v8 raw ledger lossless too.
        let disconnected = death.weapon.as_deref() == Some("world")
            && death.attacker_steamid.as_deref() == Some(death.victim_steamid.as_str());
        if !disconnected {
            combat_row(
                &mut rows,
                round,
                &death.victim_steamid,
                raw.effective_health_damage_available,
            )
            .deaths += 1;
        }
        if let Some(attacker) = death.attacker_steamid.as_deref() {
            let row = combat_row(
                &mut rows,
                round,
                attacker,
                raw.effective_health_damage_available,
            );
            if attacker != death.victim_steamid {
                match observed_enemy(death.attacker_team.as_deref(), death.victim_team.as_deref()) {
                    Some(true) => add_known(&mut row.enemy_kills, 1),
                    Some(false) => {}
                    None => row.enemy_kills = None,
                }
            }
        }
        if let Some(assister) = death.assister_steamid.as_deref() {
            let row = combat_row(
                &mut rows,
                round,
                assister,
                raw.effective_health_damage_available,
            );
            if assister != death.victim_steamid
                && death.attacker_steamid.as_deref() != Some(assister)
            {
                match observed_enemy(death.assister_team.as_deref(), death.victim_team.as_deref()) {
                    Some(true) => add_known(&mut row.enemy_assists, 1),
                    Some(false) => {}
                    None => row.enemy_assists = None,
                }
            }
        }
    }
    for key in conflicting_sides {
        if let Some(row) = rows.get_mut(&key) {
            row.side = None;
            row.enemy_kills = None;
            row.enemy_assists = None;
            row.enemy_health_damage = None;
            row.effective_enemy_health_damage = None;
        }
    }
    for row in rows.values_mut() {
        row.stable_team = rounds
            .iter()
            .find(|round| round.round_number == row.round_number)
            .and_then(|round| {
                match (
                    row.side.as_deref(),
                    round.team_a_side.as_deref(),
                    round.team_b_side.as_deref(),
                ) {
                    (Some(side), Some(a), _) if side == a => Some("A".to_owned()),
                    (Some(side), _, Some(b)) if side == b => Some("B".to_owned()),
                    _ => None,
                }
            });
    }
    let mut result: Vec<_> = rows.into_values().collect();
    result.sort_by(|a, b| (a.round_number, &a.steamid).cmp(&(b.round_number, &b.steamid)));
    result
}

pub fn validate_import(raw: &RawDemo, analyzed: &AnalyzedMatch) -> AppResult<ImportSummary> {
    for round in &analyzed.rounds {
        if let (Some(start), Some(end)) = (round.start_tick, round.end_tick) {
            if start >= end {
                return Err(AppError::Parse(format!(
                    "round {} ends at tick {end} before its start at tick {start}",
                    round.round_number
                )));
            }
        }
    }
    let throw_count = raw.throws.len();
    let path_count = analyzed
        .events
        .iter()
        .filter(|event| event.trajectory.len() >= 2)
        .count();
    if !raw.throws.is_empty() && path_count == 0 {
        return Err(AppError::Parse(format!(
            "demo contains {} grenade throws but zero usable replay paths",
            raw.throws.len()
        )));
    }

    let mut by_utility = std::collections::BTreeMap::new();
    for event in &analyzed.events {
        let counts = by_utility
            .entry(event.utility_type.as_str().to_owned())
            .or_insert((event.utility_type, 0_usize, 0_usize));
        counts.1 += 1;
        if event.trajectory.len() >= 2 {
            counts.2 += 1;
        }
    }

    Ok(ImportSummary {
        throw_count,
        path_count,
        trajectory_count: raw.trajectories.len(),
        total_rows: raw.trajectory_stats.total_rows,
        valid_rows: raw.trajectory_stats.valid_rows,
        skipped_rows: raw.trajectory_stats.skipped_rows,
        coverage_by_utility: by_utility
            .into_values()
            .map(
                |(utility_type, throw_count, path_count)| UtilityPathCoverage {
                    utility_type,
                    throw_count,
                    path_count,
                },
            )
            .collect(),
    })
}

pub fn infer_tick_rate(header: &RawHeader) -> f64 {
    match (header.demo_ticks, header.playback_time) {
        (Some(ticks), Some(secs)) if secs > 1.0 && ticks > 0 => {
            let rate = ticks as f64 / secs;
            if (16.0..=256.0).contains(&rate) {
                rate
            } else {
                DEFAULT_TICK_RATE
            }
        }
        _ => DEFAULT_TICK_RATE,
    }
}

// The original CSDM analyzer starts Team A on CT and swaps only when the
// GameRules game phase changes to TeamSideSwitch (3) after match start.
// Missing phase history cannot be read as zero switches. Stop assigning
// Team A/B sides at the first unreadable or unsupported phase tick.
fn team_a_side_at_round(raw: &RawDemo, start_tick: Option<i64>) -> Option<&'static str> {
    let tick =
        start_tick.filter(|_| raw.observed_match_start && raw.team_phase_known_at_match_start)?;
    if raw
        .team_phase_unknown_from_tick
        .is_some_and(|unknown| unknown <= tick)
    {
        return None;
    }
    if raw
        .team_side_switch_ticks
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
    {
        return None;
    }
    let switches = raw
        .team_side_switch_ticks
        .iter()
        .filter(|&&at| at <= tick)
        .count();
    Some(if switches % 2 == 0 { "CT" } else { "T" })
}

fn build_rounds(raw: &RawDemo) -> Vec<AnalyzedRound> {
    let mut by_round: HashMap<i64, AnalyzedRound> = HashMap::new();
    for ev in &raw.round_events {
        let Some(round) = ev.round else { continue };
        if round < 1 {
            continue;
        }
        let entry = by_round.entry(round).or_insert_with(|| AnalyzedRound {
            round_number: round,
            start_tick: None,
            freeze_end_tick: None,
            end_tick: None,
            winner: None,
            end_reason: None,
            team_a_side: None,
            team_b_side: None,
            ct_equip_value: None,
            t_equip_value: None,
        });
        match ev.kind.as_str() {
            "round_start" => entry.start_tick = ev.tick.or(entry.start_tick),
            "round_freeze_end" => entry.freeze_end_tick = ev.tick.or(entry.freeze_end_tick),
            "round_end" => {
                entry.end_tick = ev.tick.or(entry.end_tick);
                if ev.winner.is_some() {
                    entry.winner = normalize_team(ev.winner.as_deref());
                }
                if ev.reason.is_some() {
                    entry.end_reason = ev.reason.clone();
                }
            }
            _ => {}
        }
    }

    // Economy: sum equip values per team at each round's freeze-end tick.
    let mut econ_by_tick: HashMap<i64, (i64, i64)> = HashMap::new();
    for e in &raw.economy {
        let (Some(tick), Some(team)) = (e.tick, normalize_team(e.team.as_deref())) else {
            continue;
        };
        let slot = econ_by_tick.entry(tick).or_insert((0, 0));
        match team.as_str() {
            "CT" => slot.0 += e.equip_value,
            "T" => slot.1 += e.equip_value,
            _ => {}
        }
    }
    for round in by_round.values_mut() {
        if let Some(ft) = round.freeze_end_tick {
            if let Some(&(ct, t)) = econ_by_tick.get(&ft) {
                round.ct_equip_value = Some(ct);
                round.t_equip_value = Some(t);
            }
        }
    }

    let mut rounds: Vec<_> = by_round.into_values().collect();
    rounds.sort_by_key(|r| r.round_number);
    for round in &mut rounds {
        if let Some(side) = team_a_side_at_round(raw, round.start_tick) {
            round.team_a_side = Some(side.into());
            round.team_b_side = Some(if side == "CT" { "T" } else { "CT" }.into());
        }
    }
    rounds
}

/// Map demoparser team names to CT/T.
pub fn normalize_team(team: Option<&str>) -> Option<String> {
    match team {
        Some("CT") | Some("ct") | Some("CounterTerrorist") => Some("CT".into()),
        Some("T") | Some("t") | Some("TERRORIST") | Some("Terrorist") => Some("T".into()),
        _ => None,
    }
}

/// Normalize demoparser grenade_type values onto UtilityType.
fn normalize_grenade_type(t: Option<&str>) -> Option<UtilityType> {
    let t = t?.to_ascii_lowercase();
    if t.contains("smoke") {
        Some(UtilityType::Smoke)
    } else if t.contains("flash") {
        Some(UtilityType::Flash)
    } else if t.contains("he") && !t.contains("hex") {
        Some(UtilityType::He)
    } else if t.contains("molotov") {
        Some(UtilityType::Molotov)
    } else if t.contains("inc") {
        Some(UtilityType::Incendiary)
    } else if t.contains("decoy") {
        Some(UtilityType::Decoy)
    } else if t.contains("fire") {
        Some(UtilityType::Molotov)
    } else {
        None
    }
}

/// Does a detonation `kind` from the extractor match a utility type?
fn detonation_matches(kind: &str, utility: UtilityType) -> bool {
    match kind {
        "smoke" => utility == UtilityType::Smoke,
        "flash" => utility == UtilityType::Flash,
        "he" => utility == UtilityType::He,
        "decoy" => utility == UtilityType::Decoy,
        "fire" => utility == UtilityType::Molotov || utility == UtilityType::Incendiary,
        _ => false,
    }
}

fn dist3(a: Vec3, b: Vec3) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

/// Detect bounces: significant direction changes between consecutive segments.
pub fn detect_bounces(points: &[TrajPoint]) -> Vec<TrajPoint> {
    let mut bounces = Vec::new();
    if points.len() < 3 {
        return bounces;
    }
    for w in points.windows(3) {
        let (a, b, c) = (w[0], w[1], w[2]);
        let v1 = [b[1] - a[1], b[2] - a[2], b[3] - a[3]];
        let v2 = [c[1] - b[1], c[2] - b[2], c[3] - b[3]];
        let n1 = (v1[0].powi(2) + v1[1].powi(2) + v1[2].powi(2)).sqrt();
        let n2 = (v2[0].powi(2) + v2[1].powi(2) + v2[2].powi(2)).sqrt();
        if n1 < 1.0 || n2 < 1.0 {
            continue; // stationary - not a bounce
        }
        let cos = (v1[0] * v2[0] + v1[1] * v2[1] + v1[2] * v2[2]) / (n1 * n2);
        if cos < BOUNCE_COS_THRESHOLD {
            bounces.push(b);
        }
    }
    bounces
}

/// Downsample a trajectory, always keeping first and last points.
pub fn downsample(points: &[TrajPoint], max: usize) -> Vec<TrajPoint> {
    if points.len() <= max || max < 2 {
        return points.to_vec();
    }
    let step = (points.len() - 1) as f64 / (max - 1) as f64;
    let mut out = Vec::with_capacity(max);
    for i in 0..max {
        out.push(points[(i as f64 * step).round() as usize]);
    }
    *out.last_mut().unwrap() = *points.last().unwrap();
    out
}

fn build_events(raw: &RawDemo, rounds: &[AnalyzedRound], tick_rate: f64) -> Vec<AnalyzedEvent> {
    let freeze_by_round: HashMap<i64, i64> = rounds
        .iter()
        .filter_map(|r| r.freeze_end_tick.map(|t| (r.round_number, t)))
        .collect();

    // Utility attribution needs only its detonation ticks. Keep the full frame
    // stream out of this index to avoid duplicating every replay sample.
    let detonation_ticks: std::collections::HashSet<i64> =
        raw.detonations.iter().filter_map(|d| d.tick).collect();
    let mut pos_by_tick: HashMap<i64, Vec<&RawPosition>> = HashMap::new();
    for p in &raw.positions {
        if let Some(t) = p.tick {
            if detonation_ticks.contains(&t) && p.is_alive != Some(false) {
                pos_by_tick.entry(t).or_default().push(p);
            }
        }
    }

    let mut used_traj: Vec<bool> = vec![false; raw.trajectories.len()];
    let mut used_det: Vec<bool> = vec![false; raw.detonations.len()];
    let mut events = Vec::with_capacity(raw.throws.len());

    let mut throws: Vec<&RawThrow> = raw.throws.iter().collect();
    throws.sort_by_key(|t| t.tick.unwrap_or(0));

    for throw in throws {
        let (Some(tick), Some(steamid)) = (throw.tick, throw.steamid.as_deref()) else {
            continue;
        };
        let Some(utility) = UtilityType::from_str(&throw.utility) else {
            continue;
        };
        let round = throw.round.unwrap_or(0).max(1);

        // ---- match a trajectory: same thrower, compatible type, starts near throw tick
        let mut best: Option<(usize, i64)> = None;
        for (i, traj) in raw.trajectories.iter().enumerate() {
            if used_traj[i] || traj.points.is_empty() {
                continue;
            }
            if traj.steamid.as_deref() != Some(steamid) {
                continue;
            }
            if let Some(tt) = normalize_grenade_type(traj.grenade_type.as_deref()) {
                let compatible = tt == utility
                    || (matches!(tt, UtilityType::Molotov | UtilityType::Incendiary)
                        && matches!(utility, UtilityType::Molotov | UtilityType::Incendiary));
                if !compatible {
                    continue;
                }
            }
            let first_tick = traj.points[0][0] as i64;
            let dt = first_tick - tick;
            if (-32..=THROW_MATCH_WINDOW).contains(&dt) {
                if best.map_or(true, |(_, bd)| dt.abs() < bd.abs()) {
                    best = Some((i, dt));
                }
            }
        }

        let matched_trajectory = best.map(|(i, _)| i);
        let (trajectory, bounces, land_pos) = match best {
            Some((i, _)) => {
                used_traj[i] = true;
                let traj = &raw.trajectories[i];
                let last = traj.points.last().unwrap();
                (
                    downsample(&traj.points, MAX_TRAJ_POINTS),
                    detect_bounces(&traj.points),
                    Some(Vec3 {
                        x: last[1],
                        y: last[2],
                        z: last[3],
                    }),
                )
            }
            None => (Vec::new(), Vec::new(), None),
        };

        // Entity indices are recycled. Ordinary effects use the projectile
        // entity during its lifetime; fire creates a separate inferno entity
        // at the projectile's end. Both require owner, round, time and space.
        let mut best_det: Option<(usize, f64)> = None;
        for (i, d) in raw.detonations.iter().enumerate() {
            if used_det[i] || !detonation_matches(&d.kind, utility) {
                continue;
            }
            if d.steamid.as_deref() != Some(steamid) {
                continue;
            }
            // A projectile can ignite just after the next round starts.
            // The lifetime and position checks below must still prove the link.
            if d.round
                .is_some_and(|value| value != round && value != round + 1)
            {
                continue;
            }
            let Some(dtick) = d.tick else {
                continue;
            };
            if dtick < tick || dtick - tick > (tick_rate * 10.0) as i64 {
                continue;
            }
            let Some(traj_idx) = matched_trajectory else {
                continue;
            };
            let traj = &raw.trajectories[traj_idx];
            let (Some(x), Some(y), Some(z)) = (d.x, d.y, d.z) else {
                continue;
            };
            let fire = matches!(utility, UtilityType::Molotov | UtilityType::Incendiary);
            let reference_point = if fire {
                let last = traj.points.last().unwrap();
                if (dtick - last[0] as i64).abs() > 2 {
                    continue;
                }
                last
            } else {
                if d.entity_id != Some(traj.entity_id) {
                    continue;
                }
                let first = traj.points.first().unwrap()[0] as i64;
                let last = traj.points.last().unwrap()[0] as i64;
                if dtick < first - 8 || dtick > last + 16 {
                    continue;
                }
                traj.points
                    .iter()
                    .min_by_key(|p| (p[0] as i64 - dtick).abs())
                    .unwrap()
            };
            let distance = dist3(
                Vec3 { x, y, z },
                Vec3 {
                    x: reference_point[1],
                    y: reference_point[2],
                    z: reference_point[3],
                },
            );
            if distance > if fire { 256.0 } else { 32.0 } {
                continue;
            }
            if best_det.is_none_or(|(_, best)| distance < best) {
                best_det = Some((i, distance));
            }
        }
        let det_idx = best_det.map(|(i, _)| i);

        let (detonate_tick, detonate_pos) = match det_idx {
            Some(i) => {
                used_det[i] = true;
                let d = &raw.detonations[i];
                let pos = match (d.x, d.y) {
                    (Some(x), Some(y)) => Some(Vec3 {
                        x,
                        y,
                        z: d.z.unwrap_or(0.0),
                    }),
                    _ => land_pos,
                };
                (d.tick, pos)
            }
            None => (None, land_pos),
        };

        let thrower_team = normalize_team(throw.team.as_deref()).unwrap_or_default();
        let effect_pos = detonate_pos.or(land_pos);

        // ---- nearby players at detonation
        let mut nearby_players = Vec::new();
        if let (Some(dtick), Some(center)) = (detonate_tick, effect_pos) {
            if let Some(rows) = pos_by_tick.get(&dtick) {
                for p in rows {
                    let (Some(x), Some(y)) = (p.x, p.y) else {
                        continue;
                    };
                    let ppos = Vec3 {
                        x,
                        y,
                        z: p.z.unwrap_or(center.z),
                    };
                    let d = dist3(ppos, center);
                    if d <= NEARBY_RADIUS {
                        nearby_players.push(NearbyPlayer {
                            steamid: p.steamid.clone().unwrap_or_default(),
                            name: p.name.clone().unwrap_or_default(),
                            team: normalize_team(p.team.as_deref()).unwrap_or_default(),
                            distance: (d * 10.0).round() / 10.0,
                        });
                    }
                }
                nearby_players.sort_by(|a, b| a.distance.total_cmp(&b.distance));
            }
        }

        let throw_time = freeze_by_round
            .get(&round)
            .map(|ft| (tick - ft) as f64 / tick_rate)
            .unwrap_or(0.0);

        events.push(AnalyzedEvent {
            round_number: round,
            utility_type: utility,
            thrower_steamid: steamid.to_string(),
            thrower_name: throw.name.clone().unwrap_or_else(|| "unknown".into()),
            thrower_team,
            observer_slot: throw.observer_slot,
            throw_tick: tick,
            throw_time: (throw_time * 100.0).round() / 100.0,
            throw_pos: Vec3 {
                x: throw.x.unwrap_or(0.0),
                y: throw.y.unwrap_or(0.0),
                z: throw.z.unwrap_or(0.0),
            },
            detonate_tick,
            detonate_pos,
            land_pos,
            trajectory,
            bounces,
            projectile_entity_id: matched_trajectory.map(|i| raw.trajectories[i].entity_id),
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
            nearby_players,
        });
    }

    events
}

fn weapon_matches_utility(weapon: Option<&str>, utility: UtilityType) -> bool {
    match weapon {
        Some("hegrenade") => utility == UtilityType::He,
        Some("inferno") | Some("molotov") | Some("incgrenade") => {
            matches!(utility, UtilityType::Molotov | UtilityType::Incendiary)
        }
        Some("decoy") => utility == UtilityType::Decoy,
        Some("flashbang") => utility == UtilityType::Flash,
        Some("smokegrenade") => utility == UtilityType::Smoke,
        _ => false,
    }
}

#[derive(Clone, Copy)]
enum EffectCategory {
    Blind,
    Damage,
    Kill,
    Assist,
}

fn mark_unavailable(events: &mut [AnalyzedEvent], candidates: &[usize], category: EffectCategory) {
    for &index in candidates {
        let event = &mut events[index];
        match category {
            EffectCategory::Blind => event.blind_attribution_complete = false,
            EffectCategory::Damage => event.damage_attribution_complete = false,
            EffectCategory::Kill => event.kill_attribution_complete = false,
            EffectCategory::Assist => event.assist_attribution_complete = false,
        }
    }
}

fn record_effect(
    events: &mut [AnalyzedEvent],
    ledger: &mut Vec<EffectAttribution>,
    kind: &'static str,
    source_index: usize,
    raw_json: serde_json::Value,
    candidates: Vec<usize>,
    category: EffectCategory,
    apply: impl FnOnce(&mut AnalyzedEvent),
) {
    let assigned_event = if candidates.len() == 1 {
        let index = candidates[0];
        apply(&mut events[index]);
        Some(index)
    } else {
        None
    };
    let resolution = if assigned_event.is_some() {
        "matched"
    } else if candidates.is_empty() {
        "unresolved"
    } else {
        "ambiguous"
    };
    if candidates.len() > 1 {
        mark_unavailable(events, &candidates, category);
    }
    ledger.push(EffectAttribution {
        kind,
        source_index,
        raw_json,
        resolution,
        assigned_event,
        candidate_events: candidates,
    });
}

fn record_unresolved(
    events: &mut [AnalyzedEvent],
    ledger: &mut Vec<EffectAttribution>,
    kind: &'static str,
    source_index: usize,
    raw_json: serde_json::Value,
    candidates: Vec<usize>,
    category: EffectCategory,
) {
    mark_unavailable(events, &candidates, category);
    ledger.push(EffectAttribution {
        kind,
        source_index,
        raw_json,
        resolution: "unresolved",
        assigned_event: None,
        candidate_events: candidates,
    });
}

fn effect_candidates(
    events: &[AnalyzedEvent],
    owner: Option<&str>,
    round: Option<i64>,
    tick: Option<i64>,
    compatible: impl Fn(&AnalyzedEvent, i64, i64) -> bool,
) -> Vec<usize> {
    let (Some(owner), Some(round), Some(tick)) = (owner, round, tick) else {
        return Vec::new();
    };
    events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            let detonation = event.detonate_tick?;
            if event.thrower_steamid != owner
                || !(round == event.round_number || round == event.round_number + 1)
            {
                return None;
            }
            compatible(event, tick, detonation).then_some(index)
        })
        .collect()
}

/// Each source fact is considered once. An event receives a total only when
/// the source fact has one causal candidate; ambiguity masks that category.
fn attribute_effects(
    raw: &RawDemo,
    events: &mut [AnalyzedEvent],
    tick_rate: f64,
) -> Vec<EffectAttribution> {
    let mut ledger = Vec::with_capacity(raw.blinds.len() + raw.hurts.len() + raw.deaths.len());
    for (source_index, blind) in raw.blinds.iter().enumerate() {
        let raw_json = serde_json::to_value(blind).unwrap();
        let base = effect_candidates(
            events,
            blind.attacker_steamid.as_deref(),
            blind.round,
            blind.tick,
            |event, tick, det| event.utility_type == UtilityType::Flash && (tick - det).abs() <= 16,
        );
        let candidates: Vec<_> = match blind.entity_id {
            Some(entity) => base
                .iter()
                .copied()
                .filter(|&i| events[i].projectile_entity_id == Some(entity))
                .collect(),
            None => base.clone(),
        };
        if candidates.is_empty() && !base.is_empty() {
            record_unresolved(
                events,
                &mut ledger,
                "blind",
                source_index,
                raw_json,
                base,
                EffectCategory::Blind,
            );
            continue;
        }
        if candidates.len() == 1
            && blind.victim_steamid.as_deref()
                != Some(events[candidates[0]].thrower_steamid.as_str())
            && normalize_team(blind.victim_team.as_deref()).is_none()
        {
            record_unresolved(
                events,
                &mut ledger,
                "blind",
                source_index,
                raw_json,
                candidates,
                EffectCategory::Blind,
            );
            continue;
        }
        record_effect(
            events,
            &mut ledger,
            "blind",
            source_index,
            raw_json,
            candidates,
            EffectCategory::Blind,
            |event| {
                if blind.victim_steamid.as_deref() == Some(event.thrower_steamid.as_str()) {
                    return;
                }
                if normalize_team(blind.victim_team.as_deref()).as_deref()
                    == Some(event.thrower_team.as_str())
                {
                    event.blind_team_duration += blind.duration;
                    event.teammates_blinded += 1;
                } else {
                    event.blind_enemy_duration += blind.duration;
                    event.enemies_blinded += 1;
                }
            },
        );
    }
    for (source_index, hurt) in raw.hurts.iter().enumerate() {
        let raw_json = serde_json::to_value(hurt).unwrap();
        let candidates = effect_candidates(
            events,
            hurt.attacker_steamid.as_deref(),
            hurt.round,
            hurt.tick,
            |event, tick, det| {
                let window = if event.utility_type == UtilityType::He {
                    16
                } else {
                    (FIRE_WINDOW_SECS * tick_rate) as i64
                };
                weapon_matches_utility(hurt.weapon.as_deref(), event.utility_type)
                    && tick >= det - 8
                    && tick <= det + window
            },
        );
        if candidates.len() == 1 && normalize_team(hurt.victim_team.as_deref()).is_none() {
            record_unresolved(
                events,
                &mut ledger,
                "hurt",
                source_index,
                raw_json,
                candidates,
                EffectCategory::Damage,
            );
            continue;
        }
        record_effect(
            events,
            &mut ledger,
            "hurt",
            source_index,
            raw_json,
            candidates,
            EffectCategory::Damage,
            |event| {
                if normalize_team(hurt.victim_team.as_deref()).as_deref()
                    == Some(event.thrower_team.as_str())
                {
                    event.damage_team += hurt.damage;
                } else {
                    event.damage_enemy += hurt.damage;
                }
            },
        );
    }
    for (source_index, death) in raw.deaths.iter().enumerate() {
        let raw_json = serde_json::to_value(death).unwrap();
        if death.utility_kill {
            let candidates = effect_candidates(
                events,
                death.attacker_steamid.as_deref(),
                death.round,
                death.tick,
                |event, tick, det| {
                    let window = if event.utility_type == UtilityType::He {
                        16
                    } else {
                        (FIRE_WINDOW_SECS * tick_rate) as i64
                    };
                    weapon_matches_utility(death.weapon.as_deref(), event.utility_type)
                        && tick >= det - 8
                        && tick <= det + window
                },
            );
            record_effect(
                events,
                &mut ledger,
                "kill",
                source_index,
                raw_json.clone(),
                candidates,
                EffectCategory::Kill,
                |event| event.kills += 1,
            );
        }
        if death.assisted_flash {
            let candidates = effect_candidates(
                events,
                death.assister_steamid.as_deref(),
                death.round,
                death.tick,
                |event, tick, det| {
                    event.utility_type == UtilityType::Flash
                        && tick >= det
                        && tick <= det + (FLASH_ASSIST_WINDOW_SECS * tick_rate) as i64
                },
            );
            record_effect(
                events,
                &mut ledger,
                "flash_assist",
                source_index,
                raw_json,
                candidates,
                EffectCategory::Assist,
                |event| event.flash_assists += 1,
            );
        }
    }
    for event in events {
        event.blind_enemy_duration = (event.blind_enemy_duration * 100.0).round() / 100.0;
        event.blind_team_duration = (event.blind_team_duration * 100.0).round() / 100.0;
    }
    ledger
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(tick: f64, x: f64, y: f64, z: f64) -> TrajPoint {
        [tick, x, y, z]
    }

    #[test]
    fn bounce_detected_on_direction_change() {
        let points = vec![
            pt(0.0, 0.0, 0.0, 100.0),
            pt(8.0, 100.0, 0.0, 90.0),
            pt(16.0, 200.0, 0.0, 80.0),
            // wall bounce: x reverses
            pt(24.0, 120.0, 10.0, 70.0),
            pt(32.0, 40.0, 20.0, 60.0),
        ];
        let bounces = detect_bounces(&points);
        assert_eq!(bounces.len(), 1);
        assert_eq!(bounces[0][1], 200.0);
    }

    #[test]
    fn no_bounce_on_straight_flight() {
        let points: Vec<TrajPoint> = (0..10)
            .map(|i| pt(i as f64 * 8.0, i as f64 * 50.0, 0.0, 100.0 - i as f64))
            .collect();
        assert!(detect_bounces(&points).is_empty());
    }

    #[test]
    fn downsample_keeps_endpoints() {
        let points: Vec<TrajPoint> = (0..1000)
            .map(|i| pt(i as f64, i as f64, 0.0, 0.0))
            .collect();
        let ds = downsample(&points, 100);
        assert_eq!(ds.len(), 100);
        assert_eq!(ds[0][0], 0.0);
        assert_eq!(ds[99][0], 999.0);
    }

    #[test]
    fn tick_rate_inferred_and_bounded() {
        let h = RawHeader {
            map_name: "de_mirage".into(),
            server_name: None,
            demo_ticks: Some(64_000),
            playback_time: Some(1000.0),
        };
        assert!((infer_tick_rate(&h) - 64.0).abs() < 0.01);
        let bad = RawHeader {
            demo_ticks: Some(10),
            playback_time: Some(1000.0),
            ..h
        };
        assert_eq!(infer_tick_rate(&bad), DEFAULT_TICK_RATE);
    }

    #[test]
    fn team_normalization() {
        assert_eq!(normalize_team(Some("CT")).as_deref(), Some("CT"));
        assert_eq!(normalize_team(Some("TERRORIST")).as_deref(), Some("T"));
        assert_eq!(normalize_team(Some("Spectator")), None);
    }

    fn minimal_raw() -> RawDemo {
        serde_json::from_value(serde_json::json!({
            "version": 1,
            "header": { "mapName": "de_mirage", "serverName": null, "demoTicks": 64000, "playbackTime": 1000.0 },
            "players": [{ "steamid": "s1", "name": "alice" }],
            "roundEvents": [
                { "kind": "round_start", "tick": 1000, "round": 1, "winner": null, "reason": null },
                { "kind": "round_freeze_end", "tick": 2000, "round": 1, "winner": null, "reason": null },
                { "kind": "round_end", "tick": 8000, "round": 1, "winner": "CT", "reason": "ct_win_elimination" }
            ],
            "throws": [
                { "tick": 3000, "round": 1, "utility": "flash", "steamid": "s1", "name": "alice",
                  "team": "CT", "x": 0.0, "y": 0.0, "z": 0.0 }
            ],
            "detonations": [
                { "kind": "flash", "tick": 3100, "round": 1, "entityId": 7, "steamid": "s1",
                  "x": 500.0, "y": 0.0, "z": 0.0 }
            ],
            "blinds": [
                { "tick": 3100, "round": 1, "attackerSteamid": "s1", "attackerTeam": "CT",
                  "victimSteamid": "s2", "victimName": "bob", "victimTeam": "TERRORIST",
                  "duration": 2.5, "entityId": 7 }
            ],
            "hurts": [], "deaths": [],
            "trajectories": [
                { "entityId": 7, "type": "CFlashbang", "steamid": "s1", "name": "alice",
                  "points": [[3005.0, 0.0, 0.0, 60.0], [3050.0, 250.0, 0.0, 80.0], [3100.0, 500.0, 0.0, 20.0]] }
            ],
            "economy": [
                { "tick": 2000, "steamid": "s1", "name": "alice", "team": "CT", "equipValue": 4000, "balance": 100 }
            ],
            "positions": [
                { "tick": 3100, "steamid": "s2", "name": "bob", "team": "TERRORIST", "x": 550.0, "y": 30.0, "z": 10.0 }
            ]
        }))
        .expect("valid raw json")
    }

    #[test]
    fn combat_counts_only_observed_enemy_events_and_preserves_unknown_damage() {
        let mut raw = minimal_raw();
        raw.round_sides = serde_json::from_value(serde_json::json!([
            {"round":1,"steamid":"s1","side":"CT"},
            {"round":1,"steamid":"s2","side":"T"},
            {"round":1,"steamid":"s3","side":"CT"}
        ]))
        .unwrap();
        raw.combat_deaths = serde_json::from_value(serde_json::json!([
            {"tick":3200,"round":1,"weapon":"ak47","attackerSteamid":"s1","attackerTeam":"CT",
             "victimSteamid":"s2","victimTeam":"T","assisterSteamid":"s3","assisterTeam":"CT"},
            {"tick":3300,"round":1,"weapon":"world","attackerSteamid":null,"attackerTeam":null,
             "victimSteamid":"s3","victimTeam":"CT","assisterSteamid":null,"assisterTeam":null}
        ]))
        .unwrap();
        raw.combat_hurts = serde_json::from_value(serde_json::json!([
            {"tick":3100,"round":1,"weapon":"ak47","attackerSteamid":"s1","attackerTeam":"CT",
             "victimSteamid":"s2","victimTeam":"T","damage":27},
            {"tick":3150,"round":1,"weapon":"ak47","attackerSteamid":"s1","attackerTeam":"CT",
             "victimSteamid":"s2","victimTeam":null,"damage":8}
        ]))
        .unwrap();
        let result = build_match(&raw);
        let find = |steamid: &str| {
            result
                .player_round_combat
                .iter()
                .find(|row| row.steamid == steamid)
                .unwrap()
        };
        assert_eq!(find("s1").side.as_deref(), Some("CT"));
        assert_eq!(find("s1").enemy_kills, Some(1));
        assert_eq!(find("s1").enemy_health_damage, None);
        assert_eq!(find("s2").deaths, 1);
        assert_eq!(find("s3").enemy_assists, Some(1));
        assert_eq!(find("s3").deaths, 1);

        raw.round_sides.push(
            serde_json::from_value(serde_json::json!(
                {"round":1,"steamid":"s1","side":"T"}
            ))
            .unwrap(),
        );
        let ambiguous = build_match(&raw);
        let ada = ambiguous
            .player_round_combat
            .iter()
            .find(|row| row.steamid == "s1")
            .unwrap();
        assert_eq!(ada.side, None);
        assert_eq!(ada.enemy_kills, None);
    }

    #[test]
    fn raw_enemy_damage_keeps_overkill_and_same_tick_hits_without_claiming_effective_hp() {
        let mut raw = minimal_raw();
        raw.round_sides = serde_json::from_value(serde_json::json!([
            {"round":1,"steamid":"s1","side":"CT"},
            {"round":1,"steamid":"s2","side":"T"},
            {"round":1,"steamid":"s3","side":"CT"}
        ]))
        .unwrap();
        let hurt = |tick, victim: &str, victim_team: &str, damage| {
            serde_json::json!({
                "tick":tick,"round":1,"weapon":"ak47","attackerSteamid":"s1",
                "attackerTeam":"CT","victimSteamid":victim,"victimTeam":victim_team,
                "damage":damage
            })
        };
        raw.combat_hurts = serde_json::from_value(serde_json::json!([
            hurt(3100, "s2", "T", 15),
            hurt(3100, "s2", "T", 7),
            hurt(3100, "s3", "CT", 30),
            hurt(3100, "s1", "CT", 9)
        ]))
        .unwrap();
        raw.combat_deaths = serde_json::from_value(serde_json::json!([{
            "tick":3100,"round":1,"weapon":"ak47","attackerSteamid":"s1",
            "attackerTeam":"CT","victimSteamid":"s2","victimTeam":"T",
            "assisterSteamid":null,"assisterTeam":null
        }]))
        .unwrap();
        let analyzed = build_match(&raw);
        let attacker = analyzed
            .player_round_combat
            .iter()
            .find(|row| row.steamid == "s1")
            .unwrap();
        // If the victim had 5 HP before the lethal 15-point event, effective
        // health removed would be 5. That pre-hit HP is not in these facts.
        assert_eq!(attacker.enemy_health_damage, Some(22));
        assert_eq!(attacker.enemy_kills, Some(1));
        assert_eq!(
            analyzed
                .player_round_combat
                .iter()
                .find(|row| row.steamid == "s2")
                .unwrap()
                .deaths,
            1
        );
    }

    #[test]
    fn effective_hp_and_stable_team_follow_observed_match_and_halftime() {
        let mut raw = minimal_raw();
        raw.observed_match_start = true;
        raw.team_phase_known_at_match_start = true;
        raw.effective_health_damage_available = true;
        raw.team_side_switch_ticks = vec![8400];
        raw.round_events.extend(
            serde_json::from_value::<Vec<_>>(serde_json::json!([
                {"kind":"round_start","tick":8500,"round":2,"winner":null,"reason":null},
                {"kind":"round_freeze_end","tick":9300,"round":2,"winner":null,"reason":null},
                {"kind":"round_end","tick":12000,"round":2,"winner":"T","reason":"ct_killed"}
            ]))
            .unwrap(),
        );
        raw.round_sides = serde_json::from_value(serde_json::json!([
            {"round":1,"steamid":"s1","side":"CT"},
            {"round":1,"steamid":"s2","side":"T"},
            {"round":2,"steamid":"s1","side":"T"},
            {"round":2,"steamid":"s2","side":"CT"}
        ]))
        .unwrap();
        raw.combat_hurts = serde_json::from_value(serde_json::json!([
            {"tick":3100,"round":1,"weapon":"ak47","attackerSteamid":"s1","attackerTeam":"CT",
             "victimSteamid":"s2","victimTeam":"T","damage":88,"postHealth":12,
             "startHealth":100,"effectiveDamage":88},
            {"tick":3100,"round":1,"weapon":"ak47","attackerSteamid":"s1","attackerTeam":"CT",
             "victimSteamid":"s2","victimTeam":"T","damage":119,"postHealth":0,
             "startHealth":100,"effectiveDamage":12},
            {"tick":9400,"round":2,"weapon":"ak47","attackerSteamid":"s1","attackerTeam":"T",
             "victimSteamid":"s2","victimTeam":"CT","damage":20,"postHealth":80,
             "startHealth":100,"effectiveDamage":20}
        ]))
        .unwrap();
        let analyzed = build_match(&raw);
        assert_eq!(
            analyzed
                .rounds
                .iter()
                .map(|row| row.team_a_side.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("CT"), Some("T")]
        );
        let player = |round| {
            analyzed
                .player_round_combat
                .iter()
                .find(|row| row.round_number == round && row.steamid == "s1")
                .unwrap()
        };
        assert_eq!(player(1).stable_team.as_deref(), Some("A"));
        assert_eq!(player(2).stable_team.as_deref(), Some("A"));
        assert_eq!(player(1).enemy_health_damage, Some(207));
        assert_eq!(player(1).effective_enemy_health_damage, Some(100));
        assert_eq!(player(2).effective_enemy_health_damage, Some(20));

        let mut repeated_swap = raw.clone();
        repeated_swap.team_side_switch_ticks.push(12500);
        repeated_swap.round_events.extend(
            serde_json::from_value::<Vec<_>>(serde_json::json!([
                {"kind":"round_start","tick":13000,"round":3,"winner":null,"reason":null},
                {"kind":"round_freeze_end","tick":13800,"round":3,"winner":null,"reason":null},
                {"kind":"round_end","tick":16000,"round":3,"winner":"CT","reason":"t_killed"}
            ]))
            .unwrap(),
        );
        let three = build_match(&repeated_swap);
        assert_eq!(
            three
                .rounds
                .iter()
                .map(|row| row.team_a_side.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("CT"), Some("T"), Some("CT")]
        );

        raw.team_phase_known_at_match_start = false;
        let missing_phase = build_match(&raw);
        assert!(missing_phase
            .rounds
            .iter()
            .all(|row| row.team_a_side.is_none()));
        assert!(missing_phase
            .player_round_combat
            .iter()
            .all(|row| row.stable_team.is_none()));
        assert!(missing_phase
            .player_round_combat
            .iter()
            .any(|row| row.effective_enemy_health_damage == Some(100)));
        raw.team_phase_known_at_match_start = true;
        raw.team_phase_unknown_from_tick = Some(8400);
        let phase_gap = build_match(&raw);
        assert_eq!(
            phase_gap
                .rounds
                .iter()
                .map(|row| row.team_a_side.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("CT"), None]
        );
        assert!(phase_gap
            .player_round_combat
            .iter()
            .filter(|row| row.round_number == 2)
            .all(|row| row.stable_team.is_none()));
        raw.team_phase_unknown_from_tick = None;

        raw.observed_match_start = false;
        raw.effective_health_damage_available = false;
        let old = build_match(&raw);
        assert!(old.rounds.iter().all(|row| row.team_a_side.is_none()));
        assert!(old
            .player_round_combat
            .iter()
            .all(|row| row.stable_team.is_none() && row.effective_enemy_health_damage.is_none()));
        raw.observed_match_start = true;
        raw.effective_health_damage_available = true;
        raw.combat_hurts[0].effective_damage = None;
        let incomplete = build_match(&raw);
        let first = incomplete
            .player_round_combat
            .iter()
            .find(|row| row.round_number == 1 && row.steamid == "s1")
            .unwrap();
        assert_eq!(first.effective_enemy_health_damage, None);
        assert_eq!(first.enemy_health_damage, Some(207));
    }

    #[test]
    fn combat_between_rounds_stays_in_prior_round_and_post_match_deaths_stay_raw_only() {
        let mut raw = minimal_raw();
        raw.round_events.extend(
            serde_json::from_value::<Vec<_>>(serde_json::json!([
                {"kind":"round_start","tick":8500,"round":2,"winner":null,"reason":null},
                {"kind":"round_end","tick":12000,"round":2,"winner":"T","reason":"ct_killed"}
            ]))
            .unwrap(),
        );
        raw.combat_deaths = serde_json::from_value(serde_json::json!([
            {"tick":8200,"round":2,"weapon":"ak47","attackerSteamid":"s1","attackerTeam":"CT",
             "victimSteamid":"s2","victimTeam":"T","assisterSteamid":null,"assisterTeam":null},
            {"tick":12100,"round":3,"weapon":"world","attackerSteamid":"s2","attackerTeam":"T",
             "victimSteamid":"s2","victimTeam":"T","assisterSteamid":null,"assisterTeam":null}
        ]))
        .unwrap();
        let analyzed = build_match(&raw);
        assert_eq!(analyzed.raw_combat_deaths.len(), 2);
        assert!(!analyzed
            .player_round_combat
            .iter()
            .any(|row| row.round_number == 3));
        let victim = analyzed
            .player_round_combat
            .iter()
            .find(|row| row.steamid == "s2")
            .unwrap();
        assert_eq!(victim.round_number, 1);
        assert_eq!(victim.deaths, 1);
    }

    #[test]
    fn world_self_death_stays_raw_but_does_not_count_as_scoreboard_death() {
        let mut raw = minimal_raw();
        raw.combat_deaths = serde_json::from_value(serde_json::json!([{
            "tick": 3200, "round": 1, "weapon": "world",
            "attackerSteamid": "s1", "attackerTeam": "CT",
            "victimSteamid": "s1", "victimTeam": "CT",
            "assisterSteamid": null, "assisterTeam": null
        }]))
        .unwrap();
        let analyzed = build_match(&raw);
        assert_eq!(analyzed.raw_combat_deaths.len(), 1);
        let player = analyzed
            .player_round_combat
            .iter()
            .find(|row| row.steamid == "s1")
            .unwrap();
        assert_eq!(player.deaths, 0);
    }

    #[test]
    fn full_pipeline_flash() {
        let raw = minimal_raw();
        let m = build_match(&raw);
        assert_eq!(m.map_name, "de_mirage");
        assert_eq!(m.rounds.len(), 1);
        assert_eq!(m.rounds[0].winner.as_deref(), Some("CT"));
        assert_eq!(m.rounds[0].ct_equip_value, Some(4000));
        assert_eq!(m.events.len(), 1);

        let ev = &m.events[0];
        assert_eq!(ev.utility_type, UtilityType::Flash);
        assert_eq!(ev.detonate_tick, Some(3100));
        assert_eq!(ev.detonate_pos.unwrap().x, 500.0);
        assert_eq!(ev.enemies_blinded, 1);
        assert!((ev.blind_enemy_duration - 2.5).abs() < 1e-9);
        assert_eq!(ev.teammates_blinded, 0);
        assert_eq!(ev.trajectory.len(), 3);
        assert_eq!(ev.nearby_players.len(), 1);
        assert_eq!(ev.nearby_players[0].name, "bob");
        // throw_time: (3000 - 2000) / 64 � 15.63s after freeze end
        assert!((ev.throw_time - 15.63).abs() < 0.02);
    }

    fn overlapping_events(utility: UtilityType) -> (RawDemo, Vec<AnalyzedEvent>) {
        let mut raw = minimal_raw();
        raw.blinds.clear();
        let mut first = build_match(&raw).events.remove(0);
        first.utility_type = utility;
        first.detonate_tick = Some(3100);
        first.projectile_entity_id = Some(1);
        let mut second = first.clone();
        second.detonate_tick = Some(3104);
        second.projectile_entity_id = Some(2);
        (raw, vec![first, second])
    }

    #[test]
    fn blind_entity_identity_prevents_overlap_double_count() {
        let (mut raw, mut events) = overlapping_events(UtilityType::Flash);
        raw.blinds.push(RawBlind {
            tick: Some(3105),
            round: Some(1),
            attacker_steamid: Some("s1".into()),
            attacker_team: Some("CT".into()),
            victim_steamid: Some("s2".into()),
            victim_name: Some("bob".into()),
            victim_team: Some("T".into()),
            duration: 2.5,
            entity_id: Some(2),
        });
        let facts = attribute_effects(&raw, &mut events, 64.0);
        assert_eq!(
            (events[0].enemies_blinded, events[1].enemies_blinded),
            (0, 1)
        );
        assert_eq!(facts[0].assigned_event, Some(1));
        assert!(events.iter().all(|event| event.blind_attribution_complete));

        raw.blinds[0].entity_id = Some(999);
        let (_, mut events) = overlapping_events(UtilityType::Flash);
        let facts = attribute_effects(&raw, &mut events, 64.0);
        assert_eq!(facts[0].resolution, "unresolved");
        assert!(events.iter().all(|event| !event.blind_attribution_complete));
    }

    #[test]
    fn overlapping_flash_assist_is_preserved_but_unavailable() {
        let (mut raw, mut events) = overlapping_events(UtilityType::Flash);
        raw.deaths.push(RawDeath {
            tick: Some(3110),
            round: Some(1),
            weapon: Some("ak47".into()),
            attacker_steamid: Some("s3".into()),
            victim_steamid: Some("s2".into()),
            assister_steamid: Some("s1".into()),
            assisted_flash: true,
            utility_kill: false,
        });
        let facts = attribute_effects(&raw, &mut events, 64.0);
        assert_eq!(facts[0].resolution, "ambiguous");
        assert_eq!(facts[0].candidate_events, [0, 1]);
        assert!(events
            .iter()
            .all(|event| event.flash_assists == 0 && !event.assist_attribution_complete));
    }

    #[test]
    fn overlapping_he_damage_and_kill_are_never_counted_twice() {
        let (mut raw, mut events) = overlapping_events(UtilityType::He);
        raw.hurts.push(RawHurt {
            tick: Some(3105),
            round: Some(1),
            weapon: Some("hegrenade".into()),
            attacker_steamid: Some("s1".into()),
            attacker_team: Some("CT".into()),
            victim_steamid: Some("s2".into()),
            victim_team: Some("T".into()),
            damage: 40,
        });
        raw.deaths.push(RawDeath {
            tick: Some(3106),
            round: Some(1),
            weapon: Some("hegrenade".into()),
            attacker_steamid: Some("s1".into()),
            victim_steamid: Some("s2".into()),
            assister_steamid: None,
            assisted_flash: false,
            utility_kill: true,
        });
        let facts = attribute_effects(&raw, &mut events, 64.0);
        assert_eq!(
            facts.iter().map(|f| f.resolution).collect::<Vec<_>>(),
            ["ambiguous", "ambiguous"]
        );
        assert!(events.iter().all(|event| event.damage_enemy == 0
            && event.kills == 0
            && !event.damage_attribution_complete
            && !event.kill_attribution_complete));
        let (_, mut events) = overlapping_events(UtilityType::He);
        events[0].detonate_tick = Some(3000);
        let facts = attribute_effects(&raw, &mut events, 64.0);
        assert_eq!(
            facts.iter().map(|f| f.assigned_event).collect::<Vec<_>>(),
            [Some(1), Some(1)]
        );
        assert_eq!((events[0].damage_enemy, events[1].damage_enemy), (0, 40));
        assert_eq!((events[0].kills, events[1].kills), (0, 1));
    }

    #[test]
    fn next_round_effect_needs_lifetime_and_owner_and_cannot_reach_later_rounds() {
        let (mut raw, mut events) = overlapping_events(UtilityType::He);
        events[0].round_number = 1;
        events[1].round_number = 3;
        raw.hurts.push(RawHurt {
            tick: Some(3105),
            round: Some(2),
            weapon: Some("hegrenade".into()),
            attacker_steamid: Some("s1".into()),
            attacker_team: Some("CT".into()),
            victim_steamid: Some("s2".into()),
            victim_team: Some("T".into()),
            damage: 40,
        });
        let facts = attribute_effects(&raw, &mut events, 64.0);
        assert_eq!(facts[0].assigned_event, Some(0));
        assert_eq!((events[0].damage_enemy, events[1].damage_enemy), (40, 0));
        raw.hurts[0].attacker_steamid = Some("other".into());
        let (_, mut events) = overlapping_events(UtilityType::He);
        assert_eq!(
            attribute_effects(&raw, &mut events, 64.0)[0].resolution,
            "unresolved"
        );
    }

    #[test]
    fn unresolved_assist_remains_a_raw_fact_without_a_grenade_total() {
        let (mut raw, mut events) = overlapping_events(UtilityType::Flash);
        raw.deaths.push(RawDeath {
            tick: Some(3110),
            round: Some(1),
            weapon: Some("ak47".into()),
            attacker_steamid: Some("s3".into()),
            victim_steamid: Some("s2".into()),
            assister_steamid: Some("missing-owner".into()),
            assisted_flash: true,
            utility_kill: false,
        });
        let facts = attribute_effects(&raw, &mut events, 64.0);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].kind, "flash_assist");
        assert_eq!(facts[0].resolution, "unresolved");
        assert_eq!(facts[0].assigned_event, None);
        assert_eq!(facts[0].raw_json["assisterSteamid"], "missing-owner");
        assert!(events.iter().all(|event| event.flash_assists == 0));
    }

    #[test]
    fn import_rejects_grenade_throws_when_every_path_is_missing() {
        let mut raw = minimal_raw();
        raw.trajectories.clear();
        let analyzed = build_match(&raw);
        let error = validate_import(&raw, &analyzed).unwrap_err();
        assert!(error.to_string().contains("zero usable replay paths"));
    }

    #[test]
    fn import_summary_reports_partial_path_coverage() {
        let mut raw = minimal_raw();
        raw.throws.push(RawThrow {
            tick: Some(4000),
            round: Some(1),
            utility: "smoke".into(),
            steamid: Some("s1".into()),
            name: Some("alice".into()),
            team: Some("CT".into()),
            observer_slot: Some(2),
            x: Some(0.0),
            y: Some(0.0),
            z: Some(0.0),
        });
        raw.trajectory_stats.total_rows = 12;
        raw.trajectory_stats.valid_rows = 3;
        raw.trajectory_stats.skipped_rows = 9;
        let analyzed = build_match(&raw);
        let summary = validate_import(&raw, &analyzed).unwrap();
        assert_eq!(summary.throw_count, 2);
        assert_eq!(summary.path_count, 1);
        assert_eq!(summary.trajectory_count, 1);
        assert_eq!(summary.total_rows, 12);
        assert_eq!(summary.valid_rows, 3);
        assert_eq!(summary.skipped_rows, 9);
        assert_eq!(
            summary.coverage_by_utility[0].utility_type,
            UtilityType::Flash
        );
        assert_eq!(summary.coverage_by_utility[0].throw_count, 1);
        assert_eq!(summary.coverage_by_utility[0].path_count, 1);
        assert_eq!(
            summary.coverage_by_utility[1].utility_type,
            UtilityType::Smoke
        );
        assert_eq!(summary.coverage_by_utility[1].throw_count, 1);
        assert_eq!(summary.coverage_by_utility[1].path_count, 0);
    }

    #[test]
    fn import_rejects_inverted_round_ticks() {
        let raw = minimal_raw();
        let mut analyzed = build_match(&raw);
        analyzed.rounds[0].end_tick = Some(500);
        let error = validate_import(&raw, &analyzed).unwrap_err();
        assert!(error.to_string().contains("before its start"));
    }

    #[test]
    fn acceptance_raw_demo_reports_coverage_when_requested() {
        let Some(path) = std::env::var_os("DEMOSTRIKE_ACCEPTANCE_RAW")
            .or_else(|| std::env::var_os("CSTHROWBACK_ACCEPTANCE_RAW"))
        else {
            return;
        };
        let raw: RawDemo = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let analyzed = build_match(&raw);
        let summary = validate_import(&raw, &analyzed).unwrap();
        eprintln!("real demo import summary: {summary:#?}");
        assert!(summary.throw_count > 0);
        assert!(summary.path_count > 0);
        assert!(summary.valid_rows > 0);
        assert!(!summary.coverage_by_utility.is_empty());
    }

    #[test]
    fn unmatched_throw_still_recorded() {
        let mut raw = minimal_raw();
        raw.detonations.clear();
        raw.trajectories.clear();
        let m = build_match(&raw);
        assert_eq!(m.events.len(), 1);
        assert!(m.events[0].detonate_tick.is_none());
        assert!(m.events[0].trajectory.is_empty());
    }

    #[test]
    fn reused_projectile_index_never_links_a_past_or_future_detonation() {
        let raw: RawDemo = serde_json::from_str(r#"{
            "version":1,"header":{"mapName":"de_train","serverName":null,"demoTicks":6400,"playbackTime":100},
            "throws":[
                {"tick":100,"round":1,"utility":"smoke","steamid":"123","name":"A","team":"CT","x":0,"y":0,"z":0},
                {"tick":1000,"round":2,"utility":"smoke","steamid":"123","name":"A","team":"CT","x":0,"y":0,"z":0}
            ],
            "trajectories":[
                {"entityId":5,"type":"smoke","steamid":"123","points":[[101,0,0,0],[102,1,0,0]]},
                {"entityId":5,"type":"smoke","steamid":"123","points":[[1001,0,0,0],[1002,1,0,0]]}
            ],
            "detonations":[
                {"kind":"smoke","tick":1003,"round":2,"entityId":5,"steamid":"123","x":1,"y":0,"z":0},
                {"kind":"smoke","tick":50,"round":1,"entityId":5,"steamid":"123","x":1,"y":0,"z":0},
                {"kind":"smoke","tick":103,"round":1,"entityId":5,"steamid":"123","x":1,"y":0,"z":0}
            ]
        }"#).unwrap();
        let events = build_match(&raw).events;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].detonate_tick, Some(103));
        assert_eq!(events[1].detonate_tick, Some(1003));
    }

    #[test]
    fn fire_uses_projectile_endpoint_when_inferno_has_another_entity_id() {
        let raw: RawDemo = serde_json::from_str(r#"{
            "version":1,"header":{"mapName":"de_inferno","serverName":null,"demoTicks":6400,"playbackTime":100},
            "throws":[
                {"tick":100,"round":1,"utility":"molotov","steamid":"123","name":"A","team":"T","x":0,"y":0,"z":0},
                {"tick":300,"round":1,"utility":"molotov","steamid":"123","name":"A","team":"T","x":0,"y":0,"z":0}
            ],
            "trajectories":[
                {"entityId":5,"type":"CMolotovProjectile","steamid":"123","points":[[101,0,0,0],[110,10,0,0]]},
                {"entityId":5,"type":"CMolotovProjectile","steamid":"123","points":[[301,0,0,0],[310,100,0,0]]}
            ],
            "detonations":[
                {"kind":"fire","tick":311,"round":1,"entityId":50,"steamid":"123","x":100,"y":0,"z":0},
                {"kind":"fire","tick":111,"round":2,"entityId":51,"steamid":"123","x":10,"y":0,"z":0}
            ]
        }"#).unwrap();
        let events = build_match(&raw).events;
        assert_eq!(
            events.iter().map(|e| e.detonate_tick).collect::<Vec<_>>(),
            [Some(111), Some(311)]
        );
    }
}
