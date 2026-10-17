//! Domain models shared over IPC. Mirrored by `src/types/models.ts` — keep in sync.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParserCapabilities {
    #[serde(rename = "reference-raw-facts")]
    ReferenceRawFacts,
    #[serde(rename = "grenade-only-v1")]
    GrenadeAnalysisOnlyV1,
    #[serde(rename = "native-combat-flash-v1")]
    NativeCombatFlashV1,
    #[serde(rename = "native-combat-full-v2")]
    NativeCombatFullV2,
    #[serde(rename = "test-synthetic-combat-v1")]
    TestSyntheticCombatV1,
}

impl ParserCapabilities {
    pub fn key(self) -> &'static str {
        match self { Self::ReferenceRawFacts => "reference-raw-facts", Self::GrenadeAnalysisOnlyV1 => "grenade-only-v1", Self::NativeCombatFlashV1 => "native-combat-flash-v1", Self::NativeCombatFullV2 => "native-combat-full-v2", Self::TestSyntheticCombatV1 => "test-synthetic-combat-v1" }
    }
    pub fn from_key(key: &str) -> Option<Self> {
        match key { "reference-raw-facts" => Some(Self::ReferenceRawFacts), "grenade-only-v1" => Some(Self::GrenadeAnalysisOnlyV1), "native-combat-flash-v1" => Some(Self::NativeCombatFlashV1), "native-combat-full-v2" => Some(Self::NativeCombatFullV2), "test-synthetic-combat-v1" => Some(Self::TestSyntheticCombatV1), _ => None }
    }
    pub fn supports_combat(self) -> bool { matches!(self, Self::ReferenceRawFacts | Self::NativeCombatFlashV1 | Self::NativeCombatFullV2 | Self::TestSyntheticCombatV1) }
    pub fn supports_economy(self) -> bool { matches!(self, Self::ReferenceRawFacts) }
    pub fn is_native(self) -> bool { !matches!(self, Self::ReferenceRawFacts | Self::TestSyntheticCombatV1) }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchSummary {
    pub id: i64,
    pub file_name: String,
    pub file_path: String,
    pub map_name: String,
    pub tick_rate: f64,
    pub rounds_count: i64,
    pub score_ct: i64,
    pub score_t: i64,
    pub imported_at: String,
    pub parser_capability: ParserCapabilities,
    pub event_count: i64,
    pub effect_fact_count: i64,
    pub ambiguous_effect_count: i64,
    pub unresolved_effect_count: i64,
    pub effect_attribution_version: Option<i64>,
    pub players: Vec<PlayerRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerRef {
    pub steamid: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UtilityType {
    Smoke,
    Flash,
    He,
    Molotov,
    Incendiary,
    Decoy,
}

impl UtilityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            UtilityType::Smoke => "smoke",
            UtilityType::Flash => "flash",
            UtilityType::He => "he",
            UtilityType::Molotov => "molotov",
            UtilityType::Incendiary => "incendiary",
            UtilityType::Decoy => "decoy",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "smoke" => Some(UtilityType::Smoke),
            "flash" => Some(UtilityType::Flash),
            "he" => Some(UtilityType::He),
            "molotov" => Some(UtilityType::Molotov),
            "incendiary" => Some(UtilityType::Incendiary),
            "decoy" => Some(UtilityType::Decoy),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// One trajectory sample: `[tick, x, y, z]`.
pub type TrajPoint = [f64; 4];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NearbyPlayer {
    pub steamid: String,
    pub name: String,
    pub team: String,
    pub distance: f64,
}

/// Thin row for lists and map markers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UtilityEventSummary {
    pub id: i64,
    pub match_id: i64,
    pub parser_capability: ParserCapabilities,
    pub map_name: String,
    pub round_number: i64,
    pub utility_type: UtilityType,
    pub thrower_steamid: String,
    pub thrower_name: String,
    pub thrower_team: String,
    pub throw_tick: i64,
    pub throw_time: f64,
    pub throw_pos: Vec3,
    pub detonate_pos: Option<Vec3>,
    pub blind_enemy_duration: Option<f64>,
    pub damage_enemy: Option<i64>,
    pub kills: Option<i64>,
    /// complete, unavailable, unsupported, or legacy-unverified.
    pub effect_attribution_status: String,
    pub favorite: bool,
    pub round_winner: Option<String>,
    pub has_trajectory: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UtilityPathCoverage {
    pub utility_type: UtilityType,
    pub throw_count: usize,
    pub path_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub throw_count: usize,
    pub path_count: usize,
    pub trajectory_count: usize,
    pub total_rows: usize,
    pub valid_rows: usize,
    pub skipped_rows: usize,
    pub coverage_by_utility: Vec<UtilityPathCoverage>,
}

/// Full detail for the inspector.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UtilityEventDetail {
    #[serde(flatten)]
    pub summary: UtilityEventSummary,
    pub detonate_tick: Option<i64>,
    pub land_pos: Option<Vec3>,
    pub trajectory: Vec<TrajPoint>,
    pub bounces: Vec<TrajPoint>,
    pub blind_team_duration: Option<f64>,
    pub enemies_blinded: Option<i64>,
    pub teammates_blinded: Option<i64>,
    pub damage_team: Option<i64>,
    pub flash_assists: Option<i64>,
    pub nearby_players: Vec<NearbyPlayer>,
    pub round_end_reason: Option<String>,
    pub ct_equip_value: Option<i64>,
    pub t_equip_value: Option<i64>,
    pub tick_rate: f64,
    pub demo_path: String,
    pub observer_slot: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawEffectFact {
    pub kind: String,
    pub source_index: i64,
    pub raw_json: serde_json::Value,
    pub resolution: String,
    pub assigned_event_id: Option<i64>,
    pub candidate_event_ids: Vec<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EventFilter {
    pub match_ids: Option<Vec<i64>>,
    pub maps: Option<Vec<String>>,
    pub utility_types: Option<Vec<UtilityType>>,
    pub thrower_steamids: Option<Vec<String>>,
    pub teams: Option<Vec<String>>,
    pub rounds: Option<Vec<i64>>,
    pub tag_ids: Option<Vec<i64>>,
    pub favorites_only: Option<bool>,
    pub collection_id: Option<i64>,
    pub search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub event_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RenderSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Seconds of context before the throw and after the detonation.
    pub lead_in: f64,
    pub lead_out: f64,
    /// Renderer implementation. "cs2" records the real in-game POV.
    pub renderer: String,
    /// Merge nearby throws by the same player into a single utility-set clip.
    pub group_utility_sets: bool,
    /// Maximum gap, in seconds, between consecutive throws in one set.
    pub utility_set_window: f64,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 60,
            lead_in: 2.0,
            lead_out: 2.0,
            renderer: "cs2".into(),
            group_utility_sets: true,
            utility_set_window: 12.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderJob {
    pub id: i64,
    pub status: String,
    pub progress: f64,
    pub settings: RenderSettings,
    pub event_ids: Vec<i64>,
    pub output_dir: String,
    pub error: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub file_path: String,
    pub stage: String, // queued | parsing | analyzing | saving | done | failed
    pub message: Option<String>,
    pub match_id: Option<i64>,
    pub summary: Option<ImportSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapInfo {
    pub name: String,
    pub radar_path: Option<String>,
    pub maps_dir: String,
}
