//! Raw extractor JSON contract, including older reference JSON files.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawDemo {
    pub version: u32,
    pub header: RawHeader,
    #[serde(default)]
    pub players: Vec<RawPlayer>,
    #[serde(default)]
    pub round_events: Vec<RawRoundEvent>,
    /// Observed Source 2 bomb game events. Missing actor, site or coordinates
    /// stay nullable; old imports have no bomb-event stream.
    #[serde(default)]
    pub bomb_events: Vec<RawBombEvent>,
    #[serde(default)]
    pub bomb_event_stream_available: bool,
    /// Initial Team A=CT / Team B=T is trustworthy only with match start.
    #[serde(default)]
    pub observed_match_start: bool,
    /// GameRules phase was readable before begin_new_match established A=CT.
    #[serde(default)]
    pub team_phase_known_at_match_start: bool,
    /// First post-start tick with missing or unsupported GameRules phase.
    /// Team sides after this tick are unavailable, even if decoding resumes.
    #[serde(default)]
    pub team_phase_unknown_from_tick: Option<i64>,
    /// Observed GameRules phase 3 transitions after match start, in tick order.
    #[serde(default)]
    pub team_side_switch_ticks: Vec<i64>,
    #[serde(default)]
    pub throws: Vec<RawThrow>,
    #[serde(default)]
    pub detonations: Vec<RawDetonation>,
    #[serde(default)]
    pub blinds: Vec<RawBlind>,
    #[serde(default)]
    pub hurts: Vec<RawHurt>,
    #[serde(default)]
    pub deaths: Vec<RawDeath>,
    /// Complete non-warmup combat stream. Older raw facts omit these arrays.
    #[serde(default)]
    pub combat_hurts: Vec<RawCombatHurt>,
    /// Native decoder observed and applied the pinned effective HP rule.
    #[serde(default)]
    pub effective_health_damage_available: bool,
    #[serde(default)]
    pub combat_deaths: Vec<RawCombatDeath>,
    #[serde(default)]
    pub round_sides: Vec<RawRoundSide>,
    #[serde(default)]
    pub trajectories: Vec<RawTrajectory>,
    #[serde(default)]
    pub trajectory_stats: RawTrajectoryStats,
    #[serde(default)]
    pub economy: Vec<RawEconomy>,
    /// Frame-end player positions. Legacy raw facts may contain detonation-only samples.
    #[serde(default)]
    pub positions: Vec<RawPosition>,
    /// Native frame-end player stream. Reference facts and older imports omit it.
    #[serde(default)]
    pub position_stream_available: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawTrajectoryStats {
    pub total_rows: usize,
    pub valid_rows: usize,
    pub skipped_rows: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawPosition {
    pub tick: Option<i64>,
    pub steamid: Option<String>,
    pub name: Option<String>,
    pub team: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
    #[serde(default)]
    pub is_alive: Option<bool>,
    #[serde(default)]
    pub health: Option<i64>,
    #[serde(default)]
    pub yaw: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawHeader {
    pub map_name: String,
    pub server_name: Option<String>,
    pub demo_ticks: Option<i64>,
    pub playback_time: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawPlayer {
    pub steamid: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawRoundEvent {
    pub kind: String, // round_start | round_freeze_end | round_end
    pub tick: Option<i64>,
    pub round: Option<i64>,
    pub winner: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawBombEvent {
    pub kind: String,
    pub tick: i64,
    pub round: Option<i64>,
    pub actor_steamid: Option<String>,
    pub actor_name: Option<String>,
    pub site_id: Option<i64>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawThrow {
    pub tick: Option<i64>,
    pub round: Option<i64>,
    pub utility: String,
    pub steamid: Option<String>,
    pub name: Option<String>,
    pub team: Option<String>,
    #[serde(default)]
    pub observer_slot: Option<i64>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawDetonation {
    pub kind: String, // smoke | flash | he | decoy | fire
    pub tick: Option<i64>,
    pub round: Option<i64>,
    pub entity_id: Option<i64>,
    pub steamid: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawBlind {
    pub tick: Option<i64>,
    pub round: Option<i64>,
    pub attacker_steamid: Option<String>,
    pub attacker_team: Option<String>,
    pub victim_steamid: Option<String>,
    pub victim_name: Option<String>,
    pub victim_team: Option<String>,
    pub duration: f64,
    pub entity_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawHurt {
    pub tick: Option<i64>,
    pub round: Option<i64>,
    pub weapon: Option<String>,
    pub attacker_steamid: Option<String>,
    pub attacker_team: Option<String>,
    pub victim_steamid: Option<String>,
    pub victim_team: Option<String>,
    pub damage: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawDeath {
    pub tick: Option<i64>,
    pub round: Option<i64>,
    pub weapon: Option<String>,
    pub attacker_steamid: Option<String>,
    pub victim_steamid: Option<String>,
    pub assister_steamid: Option<String>,
    pub assisted_flash: bool,
    pub utility_kill: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCombatHurt {
    /// Native game-event order across hurts and deaths; absent in older facts.
    #[serde(default)]
    pub event_order: Option<i64>,
    pub tick: i64,
    pub round: i64,
    pub weapon: Option<String>,
    pub attacker_steamid: Option<String>,
    pub attacker_team: Option<String>,
    pub victim_steamid: String,
    pub victim_team: Option<String>,
    pub damage: i64,
    /// Event-reported post-hit HP and pawn start-of-tick HP, when observed.
    #[serde(default)]
    pub post_health: Option<i32>,
    #[serde(default)]
    pub start_health: Option<i32>,
    /// Pinned demoinfocs HealthDamageTaken; absent for older or incomplete facts.
    #[serde(default)]
    pub effective_damage: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawCombatDeath {
    #[serde(default)]
    pub event_order: Option<i64>,
    pub tick: i64,
    pub round: i64,
    pub weapon: Option<String>,
    pub attacker_steamid: Option<String>,
    pub attacker_team: Option<String>,
    pub victim_steamid: String,
    pub victim_team: Option<String>,
    pub assister_steamid: Option<String>,
    pub assister_team: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawRoundSide {
    pub round: i64,
    pub steamid: String,
    /// Observed CT/T side at freeze end or a combat event, not a stable team.
    pub side: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawTrajectory {
    pub entity_id: i64,
    #[serde(rename = "type")]
    pub grenade_type: Option<String>,
    pub steamid: Option<String>,
    pub name: Option<String>,
    /// `[tick, x, y, z]`, sorted by tick.
    pub points: Vec<[f64; 4]>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawEconomy {
    pub tick: Option<i64>,
    pub steamid: Option<String>,
    pub name: Option<String>,
    pub team: Option<String>,
    pub equip_value: i64,
    pub balance: i64,
}

#[cfg(test)]
mod tests {
    use super::RawThrow;
    use serde_json::json;

    #[test]
    fn throw_deserializes_camel_case_observer_slot() {
        let parsed: RawThrow = serde_json::from_value(json!({
            "tick": 4675,
            "round": 1,
            "utility": "molotov",
            "steamid": "76561198000000000",
            "name": "zorte",
            "team": "T",
            "observerSlot": 10,
            "x": 634.0,
            "y": 1504.0,
            "z": 1700.0
        }))
        .expect("extractor throw JSON should deserialize");

        assert_eq!(parsed.observer_slot, Some(10));
    }
}
