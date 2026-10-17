use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

#[derive(Debug)]
pub enum LibraryError {
    MissingFile,
    InvalidSchema(String),
    InvalidRequest(String),
    MatchNotFound(i64),
    RoundNotFound(i64),
    Database(rusqlite::Error),
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFile => write!(
                f,
                "The library file was not found. Check the path and try again."
            ),
            Self::InvalidSchema(detail) => {
                write!(f, "This is not a compatible DemoStrike library: {detail}")
            }
            Self::InvalidRequest(detail) => write!(f, "Invalid viewer request: {detail}"),
            Self::MatchNotFound(id) => write!(
                f,
                "Match {id} is no longer in this library. Refresh the match list."
            ),
            Self::RoundNotFound(number) => write!(f, "Round {number} is not in this match."),
            Self::Database(error) => write!(f, "Could not read the library: {error}"),
        }
    }
}

impl From<rusqlite::Error> for LibraryError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

/// Rust equivalent of the upstream score comparison used by TeamScore.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScoreRelation {
    Tied,
    Leading,
    Trailing,
}

pub fn score_relation(score: i64, opposite_score: i64) -> ScoreRelation {
    use std::cmp::Ordering;
    match score.cmp(&opposite_score) {
        Ordering::Equal => ScoreRelation::Tied,
        Ordering::Greater => ScoreRelation::Leading,
        Ordering::Less => ScoreRelation::Trailing,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchSummary {
    pub id: i64,
    pub file_name: String,
    pub demo_file_path: Option<String>,
    pub map_name: String,
    pub score_ct: i64,
    pub score_t: i64,
    pub score_ct_relation: ScoreRelation,
    pub score_t_relation: ScoreRelation,
    pub imported_at: String,
    pub parser_capability: String,
    pub source_sha256: Option<String>,
    pub tick_rate: Option<f64>,
    pub duration_ticks: Option<i64>,
    pub player_count: i64,
    pub round_count: i64,
    pub utility_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    pub path: String,
    pub matches: Vec<MatchSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundSummary {
    pub round_number: i64,
    pub winner: Option<String>,
    pub start_tick: Option<i64>,
    pub freeze_end_tick: Option<i64>,
    pub end_tick: Option<i64>,
    pub end_reason: Option<String>,
    pub team_a_side: Option<String>,
    pub team_b_side: Option<String>,
    pub ct_equip_value: Option<i64>,
    pub t_equip_value: Option<i64>,
    pub combat: Option<RoundCombat>,
    /// None means this import has no verified ordered combat stream. An
    /// empty array means the stream exists but this round has no deaths.
    pub deaths: Option<Vec<RoundDeath>>,
    /// Raw damage events are absent for older imports; zero means a verified empty round.
    pub hurts: Option<Vec<RoundHurt>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundDeath {
    pub source_index: i64,
    pub event_order: Option<i64>,
    pub tick: i64,
    pub weapon: Option<String>,
    pub attacker_steamid: Option<String>,
    pub attacker_side: Option<String>,
    pub victim_steamid: String,
    pub victim_side: Option<String>,
    pub assister_steamid: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundHurt {
    pub source_index: i64,
    pub event_order: Option<i64>,
    pub tick: i64,
    pub weapon: Option<String>,
    pub attacker_steamid: Option<String>,
    pub attacker_side: Option<String>,
    pub victim_steamid: String,
    pub victim_side: Option<String>,
    /// Demo event raw_dmg_health, not CSDM's effective health damage.
    pub raw_damage: i64,
    /// Pinned CSDM HealthDamageTaken; unavailable for earlier imports.
    pub effective_damage: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundCombat {
    pub ct_players: i64,
    pub t_players: i64,
    pub unassigned_players: i64,
    pub enemy_kills: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSummary {
    pub steamid: String,
    pub name: String,
    pub combat: Option<PlayerCombat>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerCombat {
    pub stable_team: Option<String>,
    pub enemy_kills: Option<i64>,
    pub deaths: i64,
    pub enemy_assists: Option<i64>,
    pub enemy_health_damage: Option<i64>,
    pub effective_enemy_health_damage: Option<i64>,
    pub rounds: Vec<PlayerRoundCombat>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerRoundCombat {
    pub round_number: i64,
    pub side: Option<String>,
    pub stable_team: Option<String>,
    pub enemy_kills: Option<i64>,
    pub deaths: i64,
    pub enemy_assists: Option<i64>,
    pub enemy_health_damage: Option<i64>,
    pub effective_enemy_health_damage: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UtilityEvent {
    pub id: i64,
    pub round_number: i64,
    pub utility_type: String,
    pub thrower_name: String,
    pub thrower_team: String,
    pub throw_tick: i64,
    pub enemies_blinded: Option<i64>,
    pub damage_enemy: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UtilityCount {
    pub utility_type: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchDetail {
    pub summary: MatchSummary,
    pub rounds: Vec<RoundSummary>,
    pub players: Vec<PlayerSummary>,
    pub utility_events: Vec<UtilityEvent>,
    pub utility_breakdown: Vec<UtilityCount>,
    pub full_combat_coverage: bool,
}

/// The viewer keeps the demo's absolute tick domain. Older imports report
/// unavailable instead of interpreting absent rows as an empty replay.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerRoundMeta {
    pub round_number: i64,
    pub start_tick: Option<i64>,
    pub end_tick: Option<i64>,
    pub tick_rate: Option<f64>,
    pub map_name: String,
    pub positions_available: bool,
    pub player_sample_count: i64,
    pub grenade_sample_count: i64,
    pub utility_events: Vec<ViewerUtilityEvent>,
    pub bomb_events: Vec<ViewerBombEvent>,
    pub bomb_events_available: bool,
}

/// Detonation facts persist with the imported demo. Missing coordinates and
/// projectile identity stay nullable; the viewer must not infer an effect.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerUtilityEvent {
    pub id: i64,
    pub utility_type: String,
    pub throw_tick: i64,
    pub detonate_tick: Option<i64>,
    pub detonate_x: Option<f64>,
    pub detonate_y: Option<f64>,
    pub detonate_z: Option<f64>,
    pub projectile_entity_id: Option<i64>,
    pub thrower_steamid: String,
    pub thrower_name: String,
    pub thrower_side: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinderThrow {
    pub id: i64,
    pub round_number: i64,
    pub utility_type: String,
    pub thrower_steamid: String,
    pub thrower_name: String,
    pub thrower_side: String,
    pub throw_tick: i64,
    pub throw_position: [f64; 3],
    pub detonate_tick: Option<i64>,
    pub detonate_position: Option<[f64; 3]>,
    pub trajectory: Vec<[f64; 4]>,
}

/// A read-only native edge for the original grenade finder. Every coordinate
/// comes from the imported event; invalid or absent paths remain unavailable.
pub fn get_utility_finder(path: &str, match_id: i64) -> Result<Vec<FinderThrow>, LibraryError> {
    let connection = open_library(path)?;
    if !read_matches(&connection)?
        .iter()
        .any(|item| item.id == match_id)
    {
        return Err(LibraryError::MatchNotFound(match_id));
    }
    let mut query = connection.prepare(
        "SELECT id,round_number,utility_type,thrower_steamid,thrower_name,thrower_team,
                throw_tick,throw_x,throw_y,throw_z,detonate_tick,
                detonate_x,detonate_y,detonate_z,trajectory
         FROM utility_events WHERE match_id=?1 ORDER BY round_number,throw_tick,id",
    )?;
    let throws = query
        .query_map([match_id], |row| {
            let detonate = (
                row.get::<_, Option<f64>>(11)?,
                row.get::<_, Option<f64>>(12)?,
                row.get::<_, Option<f64>>(13)?,
            );
            let raw_path: String = row.get(14)?;
            let trajectory: Vec<[f64; 4]> = serde_json::from_str::<Vec<[f64; 4]>>(&raw_path)
                .unwrap_or_default()
                .into_iter()
                .filter(|point| point.iter().all(|value| value.is_finite()))
                .collect();
            Ok(FinderThrow {
                id: row.get(0)?,
                round_number: row.get(1)?,
                utility_type: row.get(2)?,
                thrower_steamid: row.get(3)?,
                thrower_name: row.get(4)?,
                thrower_side: row.get(5)?,
                throw_tick: row.get(6)?,
                throw_position: [row.get(7)?, row.get(8)?, row.get(9)?],
                detonate_tick: row.get(10)?,
                detonate_position: match detonate {
                    (Some(x), Some(y), Some(z))
                        if [x, y, z].iter().all(|value| value.is_finite()) =>
                    {
                        Some([x, y, z])
                    }
                    _ => None,
                },
                trajectory,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(throws)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerBombEvent {
    pub source_index: i64,
    pub kind: String,
    pub tick: i64,
    pub actor_steamid: Option<String>,
    pub actor_name: Option<String>,
    pub site_id: Option<i64>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerPlayerFrame {
    pub tick: i64,
    pub steamid: String,
    pub name: String,
    pub side: Option<String>,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub is_alive: Option<bool>,
    pub health: Option<i64>,
    pub yaw: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerGrenadeFrame {
    pub tick: i64,
    pub trajectory_index: i64,
    pub entity_id: i64,
    pub grenade_type: Option<String>,
    pub thrower_steamid: Option<String>,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerFrameChunk {
    pub start_tick: i64,
    pub end_tick: i64,
    pub players: Vec<ViewerPlayerFrame>,
    pub grenades: Vec<ViewerGrenadeFrame>,
}

fn has_full_combat(connection: &Connection, capability: &str) -> Result<bool, LibraryError> {
    if capability != "native-combat-full-v2" && capability != "test-synthetic-combat-v1" {
        return Ok(false);
    }
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 7 {
        return Ok(false);
    }
    let mut statement = connection.prepare("PRAGMA table_info(player_round_combat)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok([
        "match_id",
        "round_number",
        "steamid",
        "side",
        "enemy_kills",
        "deaths",
        "enemy_assists",
        "enemy_health_damage",
    ]
    .iter()
    .all(|required| columns.iter().any(|column| column == required)))
}

fn has_ordered_combat_deaths(
    connection: &Connection,
    capability: &str,
) -> Result<bool, LibraryError> {
    if capability != "native-combat-full-v2" {
        return Ok(false);
    }
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 8 {
        return Ok(false);
    }
    let mut statement = connection.prepare("PRAGMA table_info(raw_combat_deaths)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok([
        "match_id",
        "source_index",
        "event_order",
        "tick",
        "round_number",
        "weapon",
        "attacker_steamid",
        "attacker_side",
        "victim_steamid",
        "victim_side",
        "assister_steamid",
    ]
    .iter()
    .all(|column| columns.iter().any(|name| name == column)))
}

fn has_ordered_combat_hurts(
    connection: &Connection,
    capability: &str,
) -> Result<bool, LibraryError> {
    if capability != "native-combat-full-v2" {
        return Ok(false);
    }
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 8 {
        return Ok(false);
    }
    let mut statement = connection.prepare("PRAGMA table_info(raw_combat_hurts)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok([
        "match_id",
        "source_index",
        "event_order",
        "tick",
        "round_number",
        "weapon",
        "attacker_steamid",
        "attacker_side",
        "victim_steamid",
        "victim_side",
        "raw_dmg_health",
    ]
    .iter()
    .all(|column| columns.iter().any(|name| name == column)))
}

/// Source 2 can advance its raw round counter before the next live round.
/// Assign events in that gap to the last completed earlier round. Raw rows
/// remain unchanged, as with the persisted per-player combat attribution.
fn observed_combat_side(side: Option<String>) -> Option<String> {
    match side.as_deref() {
        Some("CT") => Some("CT".to_owned()),
        Some("T" | "TERRORIST") => Some("T".to_owned()),
        _ => None,
    }
}

fn effective_event_round(
    raw_round: i64,
    tick: i64,
    boundaries: &[(i64, Option<i64>, Option<i64>)],
) -> Option<i64> {
    let (_, start, _) = boundaries
        .iter()
        .find(|(number, _, _)| *number == raw_round)?;
    if start.is_some_and(|value| tick < value) {
        return boundaries
            .iter()
            .filter(|(number, _, end)| {
                *number < raw_round && end.is_some_and(|value| value <= tick)
            })
            .max_by_key(|(_, _, end)| *end)
            .map(|(number, _, _)| *number);
    }
    Some(raw_round)
}

fn open_library(path: &str) -> Result<Connection, LibraryError> {
    if !Path::new(path).is_file() {
        return Err(LibraryError::MissingFile);
    }
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    for (table, columns) in [
        (
            "matches",
            &[
                "id",
                "file_name",
                "map_name",
                "score_ct",
                "score_t",
                "imported_at",
                "parser_capability",
            ][..],
        ),
        ("players", &["match_id", "steamid", "name"][..]),
        (
            "rounds",
            &[
                "match_id",
                "round_number",
                "winner",
                "start_tick",
                "end_tick",
            ][..],
        ),
        (
            "utility_events",
            &[
                "id",
                "match_id",
                "round_number",
                "utility_type",
                "thrower_name",
                "thrower_team",
                "throw_tick",
            ][..],
        ),
    ] {
        let exists: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get(0),
        )?;
        if exists != 1 {
            return Err(LibraryError::InvalidSchema(format!(
                "missing {table} table"
            )));
        }
        let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        for column in columns {
            if !names.iter().any(|name| name == column) {
                return Err(LibraryError::InvalidSchema(format!(
                    "missing {table}.{column} column"
                )));
            }
        }
    }
    Ok(connection)
}

/// Earlier imports can retain stored effect totals without verified attribution.
/// Only the v6+ public view can expose those totals as measured facts.
fn has_verified_effect_view(connection: &Connection) -> Result<bool, LibraryError> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 6 {
        return Ok(false);
    }
    let mut statement = connection.prepare("PRAGMA table_info(utility_events_supported)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok([
        "effect_attribution_version",
        "blind_attribution_complete",
        "damage_attribution_complete",
        "enemies_blinded",
        "damage_enemy",
    ]
    .iter()
    .all(|column| columns.iter().any(|name| name == column)))
}

fn read_matches(connection: &Connection) -> Result<Vec<MatchSummary>, LibraryError> {
    let mut columns = connection.prepare("PRAGMA table_info(matches)")?;
    let available = columns
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    let optional = |name: &str| {
        if available.iter().any(|column| column == name) {
            format!("m.{name}")
        } else {
            "NULL".to_owned()
        }
    };
    let query = format!(
        "SELECT m.id, m.file_name, m.map_name, m.score_ct, m.score_t, m.imported_at,
                m.parser_capability,
                (SELECT COUNT(*) FROM players p WHERE p.match_id = m.id),
                (SELECT COUNT(*) FROM rounds r WHERE r.match_id = m.id),
                (SELECT COUNT(*) FROM utility_events u WHERE u.match_id = m.id),
                {}, {}, {}, {}
         FROM matches m ORDER BY m.imported_at DESC, m.id DESC",
        optional("file_path"),
        optional("source_sha256"),
        optional("tick_rate"),
        optional("duration_ticks")
    );
    let mut statement = connection.prepare(&query)?;
    let matches = statement
        .query_map([], |row| {
            let score_ct: i64 = row.get(3)?;
            let score_t: i64 = row.get(4)?;
            Ok(MatchSummary {
                id: row.get(0)?,
                file_name: row.get(1)?,
                demo_file_path: row.get(10)?,
                map_name: row.get(2)?,
                score_ct,
                score_t,
                score_ct_relation: score_relation(score_ct, score_t),
                score_t_relation: score_relation(score_t, score_ct),
                imported_at: row.get(5)?,
                parser_capability: row.get(6)?,
                source_sha256: row.get(11)?,
                tick_rate: row.get(12)?,
                duration_ticks: row.get(13)?,
                player_count: row.get(7)?,
                round_count: row.get(8)?,
                utility_count: row.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(matches)
}

pub fn list_matches(path: &str) -> Result<LibrarySnapshot, LibraryError> {
    let connection = open_library(path)?;
    Ok(LibrarySnapshot {
        path: path.to_owned(),
        matches: read_matches(&connection)?,
    })
}

pub fn get_viewer_round(
    path: &str,
    match_id: i64,
    round_number: i64,
) -> Result<ViewerRoundMeta, LibraryError> {
    let connection = open_library(path)?;
    let summary = read_matches(&connection)?
        .into_iter()
        .find(|item| item.id == match_id)
        .ok_or(LibraryError::MatchNotFound(match_id))?;
    let bounds: Option<(Option<i64>, Option<i64>)> = connection
        .query_row(
            "SELECT start_tick,end_tick FROM rounds WHERE match_id=?1 AND round_number=?2",
            params![match_id, round_number],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (start_tick, end_tick) = bounds.ok_or(LibraryError::RoundNotFound(round_number))?;
    let schema_version: i64 =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let mut player_sample_count = 0;
    let mut grenade_sample_count = 0;
    let mut utility_events = Vec::new();
    let mut bomb_events = Vec::new();
    let mut bomb_events_available = false;
    let mut version = None;
    if schema_version >= 10 && has_column(&connection, "matches", "viewer_position_version")? {
        version = connection.query_row(
            "SELECT viewer_position_version FROM matches WHERE id=?1",
            [match_id],
            |row| row.get::<_, Option<i64>>(0),
        )?;
        if version == Some(1) {
            player_sample_count = connection.query_row(
                "SELECT COUNT(*) FROM viewer_player_positions WHERE match_id=?1 AND round_number=?2",
                params![match_id,round_number], |row| row.get(0))?;
            grenade_sample_count = connection.query_row(
                "SELECT COUNT(*) FROM viewer_grenade_positions WHERE match_id=?1 AND round_number=?2",
                params![match_id,round_number], |row| row.get(0))?;
            if has_column(&connection, "utility_events", "projectile_entity_id")?
                && has_column(&connection, "utility_events", "detonate_tick")?
                && has_column(&connection, "utility_events", "thrower_steamid")?
            {
                let mut query = connection.prepare(
                    "SELECT id,utility_type,throw_tick,detonate_tick,
                    detonate_x,detonate_y,detonate_z,projectile_entity_id,
                    thrower_steamid,thrower_name,thrower_team
                 FROM utility_events WHERE match_id=?1 AND round_number=?2
                 ORDER BY throw_tick,id",
                )?;
                utility_events = query
                    .query_map(params![match_id, round_number], |row| {
                        Ok(ViewerUtilityEvent {
                            id: row.get(0)?,
                            utility_type: row.get(1)?,
                            throw_tick: row.get(2)?,
                            detonate_tick: row.get(3)?,
                            detonate_x: row.get(4)?,
                            detonate_y: row.get(5)?,
                            detonate_z: row.get(6)?,
                            projectile_entity_id: row.get(7)?,
                            thrower_steamid: row.get(8)?,
                            thrower_name: row.get(9)?,
                            thrower_side: row.get(10)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
            }
            if schema_version >= 11
                && has_column(&connection, "matches", "viewer_bomb_version")?
                && has_column(&connection, "viewer_bomb_events", "kind")?
            {
                bomb_events_available = connection.query_row(
                    "SELECT viewer_bomb_version FROM matches WHERE id=?1",
                    [match_id],
                    |row| row.get::<_, Option<i64>>(0),
                )? == Some(1);
            }
            if bomb_events_available {
                let mut query = connection.prepare(
                    "SELECT source_index,kind,tick,actor_steamid,actor_name,site_id,x,y,z
                     FROM viewer_bomb_events WHERE match_id=?1 AND round_number=?2
                     ORDER BY tick,source_index",
                )?;
                bomb_events = query
                    .query_map(params![match_id, round_number], |row| {
                        Ok(ViewerBombEvent {
                            source_index: row.get(0)?,
                            kind: row.get(1)?,
                            tick: row.get(2)?,
                            actor_steamid: row.get(3)?,
                            actor_name: row.get(4)?,
                            site_id: row.get(5)?,
                            x: row.get(6)?,
                            y: row.get(7)?,
                            z: row.get(8)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
            }
        }
    }
    Ok(ViewerRoundMeta {
        round_number,
        start_tick,
        end_tick,
        tick_rate: summary
            .tick_rate
            .filter(|rate| rate.is_finite() && *rate > 0.0),
        map_name: summary.map_name,
        positions_available: version == Some(1)
            && player_sample_count > 0
            && matches!((start_tick,end_tick), (Some(start),Some(end)) if start < end),
        player_sample_count,
        grenade_sample_count,
        utility_events,
        bomb_events,
        bomb_events_available,
    })
}

/// Return at most eight seconds at 64 ticks/s per IPC call. Seek and playback
/// share the same absolute tick; the service never fabricates intermediate
/// coordinates or continues a projectile after its last observed frame.
pub fn get_viewer_frames(
    path: &str,
    match_id: i64,
    round_number: i64,
    start_tick: i64,
    end_tick: i64,
) -> Result<ViewerFrameChunk, LibraryError> {
    let meta = get_viewer_round(path, match_id, round_number)?;
    if !meta.positions_available {
        return Err(LibraryError::InvalidRequest(
            "this round has no verified native position stream".into(),
        ));
    }
    if start_tick < meta.start_tick.unwrap()
        || end_tick > meta.end_tick.unwrap()
        || end_tick < start_tick
        || end_tick - start_tick > 511
    {
        return Err(LibraryError::InvalidRequest(
            "window must be within one recorded round and at most 512 ticks".into(),
        ));
    }
    let connection = open_library(path)?;
    let mut players = Vec::new();
    let mut query = connection.prepare(
        "SELECT tick,steamid,name,side,x,y,z,is_alive,health,yaw
        FROM viewer_player_positions WHERE match_id=?1 AND round_number=?2
        AND tick BETWEEN ?3 AND ?4 ORDER BY tick,steamid",
    )?;
    let rows = query.query_map(
        params![match_id, round_number, start_tick, end_tick],
        |row| {
            let is_alive: Option<i64> = row.get(7)?;
            Ok(ViewerPlayerFrame {
                tick: row.get(0)?,
                steamid: row.get(1)?,
                name: row.get(2)?,
                side: row.get(3)?,
                x: row.get(4)?,
                y: row.get(5)?,
                z: row.get(6)?,
                is_alive: is_alive.map(|value| value != 0),
                health: row.get(8)?,
                yaw: row.get(9)?,
            })
        },
    )?;
    for row in rows {
        players.push(row?);
    }
    let mut grenades = Vec::new();
    let mut query = connection.prepare(
        "SELECT tick,trajectory_index,entity_id,grenade_type,
        thrower_steamid,x,y,z FROM viewer_grenade_positions
        WHERE match_id=?1 AND round_number=?2 AND tick BETWEEN ?3 AND ?4
        ORDER BY tick,trajectory_index",
    )?;
    let rows = query.query_map(
        params![match_id, round_number, start_tick, end_tick],
        |row| {
            Ok(ViewerGrenadeFrame {
                tick: row.get(0)?,
                trajectory_index: row.get(1)?,
                entity_id: row.get(2)?,
                grenade_type: row.get(3)?,
                thrower_steamid: row.get(4)?,
                x: row.get(5)?,
                y: row.get(6)?,
                z: row.get(7)?,
            })
        },
    )?;
    for row in rows {
        grenades.push(row?);
    }
    Ok(ViewerFrameChunk {
        start_tick,
        end_tick,
        players,
        grenades,
    })
}

fn has_column(connection: &Connection, table: &str, column: &str) -> Result<bool, LibraryError> {
    let mut query = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = query
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns.iter().any(|name| name == column))
}

pub fn get_match_detail(path: &str, match_id: i64) -> Result<MatchDetail, LibraryError> {
    let connection = open_library(path)?;
    let summary = read_matches(&connection)?
        .into_iter()
        .find(|item| item.id == match_id)
        .ok_or(LibraryError::MatchNotFound(match_id))?;
    let full_combat_coverage = has_full_combat(&connection, &summary.parser_capability)?;
    let mut combat_by_player: HashMap<String, Vec<PlayerRoundCombat>> = HashMap::new();
    let mut combat_by_round: HashMap<i64, Vec<PlayerRoundCombat>> = HashMap::new();
    if full_combat_coverage {
        let effective = if has_column(
            &connection,
            "player_round_combat",
            "effective_enemy_health_damage",
        )? {
            "effective_enemy_health_damage"
        } else {
            "NULL"
        };
        let stable = if has_column(&connection, "player_round_combat", "stable_team")? {
            "stable_team"
        } else {
            "NULL"
        };
        let sql = format!(
            "SELECT round_number, steamid, side, enemy_kills, deaths,
                    enemy_assists, enemy_health_damage, {effective}, {stable}
             FROM player_round_combat WHERE match_id = ?1 ORDER BY round_number, steamid"
        );
        let mut query = connection.prepare(&sql)?;
        let records = query.query_map([match_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                PlayerRoundCombat {
                    round_number: row.get(0)?,
                    side: row.get(2)?,
                    enemy_kills: row.get(3)?,
                    deaths: row.get(4)?,
                    enemy_assists: row.get(5)?,
                    enemy_health_damage: row.get(6)?,
                    effective_enemy_health_damage: row.get(7)?,
                    stable_team: row.get(8)?,
                },
            ))
        })?;
        for record in records {
            let (round, steamid, combat) = record?;
            combat_by_round
                .entry(round)
                .or_default()
                .push(combat.clone());
            combat_by_player.entry(steamid).or_default().push(combat);
        }
    }

    let mut boundary_query = connection.prepare(
        "SELECT round_number, start_tick, end_tick FROM rounds WHERE match_id = ?1 ORDER BY round_number",
    )?;
    let boundaries = boundary_query
        .query_map([match_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<Result<Vec<(i64, Option<i64>, Option<i64>)>, _>>()?;

    let mut deaths_by_round = None;
    if full_combat_coverage && has_ordered_combat_deaths(&connection, &summary.parser_capability)? {
        let mut query = connection.prepare(
            "SELECT source_index, event_order, tick, round_number, weapon,
                    attacker_steamid, attacker_side, victim_steamid, victim_side, assister_steamid
             FROM raw_combat_deaths WHERE match_id = ?1
             ORDER BY tick, event_order, source_index",
        )?;
        let records = query.query_map([match_id], |row| {
            Ok((
                row.get::<_, i64>(3)?,
                RoundDeath {
                    source_index: row.get(0)?,
                    event_order: row.get(1)?,
                    tick: row.get(2)?,
                    weapon: row.get(4)?,
                    attacker_steamid: row.get(5)?,
                    attacker_side: observed_combat_side(row.get(6)?),
                    victim_steamid: row.get(7)?,
                    victim_side: observed_combat_side(row.get(8)?),
                    assister_steamid: row.get(9)?,
                },
            ))
        })?;
        let mut grouped: HashMap<i64, Vec<RoundDeath>> = HashMap::new();
        for record in records {
            let (round, death) = record?;
            if let Some(effective) = effective_event_round(round, death.tick, &boundaries) {
                grouped.entry(effective).or_default().push(death);
            }
        }
        // An older match can be migrated to schema v8 without ever having
        // recorded raw events. Do not turn that absence into a measured zero.
        if !grouped.is_empty() {
            deaths_by_round = Some(grouped);
        }
    }

    let mut hurts_by_round = None;
    if full_combat_coverage && has_ordered_combat_hurts(&connection, &summary.parser_capability)? {
        let effective = if has_column(&connection, "raw_combat_hurts", "effective_health_damage")? {
            "effective_health_damage"
        } else {
            "NULL"
        };
        let sql = format!(
            "SELECT source_index, event_order, tick, round_number, weapon,
                    attacker_steamid, attacker_side, victim_steamid, victim_side, raw_dmg_health,
                    {effective}
             FROM raw_combat_hurts WHERE match_id = ?1
             ORDER BY tick, event_order, source_index"
        );
        let mut query = connection.prepare(&sql)?;
        let records = query.query_map([match_id], |row| {
            Ok((
                row.get::<_, i64>(3)?,
                RoundHurt {
                    source_index: row.get(0)?,
                    event_order: row.get(1)?,
                    tick: row.get(2)?,
                    weapon: row.get(4)?,
                    attacker_steamid: row.get(5)?,
                    attacker_side: observed_combat_side(row.get(6)?),
                    victim_steamid: row.get(7)?,
                    victim_side: observed_combat_side(row.get(8)?),
                    raw_damage: row.get(9)?,
                    effective_damage: row.get(10)?,
                },
            ))
        })?;
        let mut grouped: HashMap<i64, Vec<RoundHurt>> = HashMap::new();
        for record in records {
            let (round, hurt) = record?;
            if let Some(effective) = effective_event_round(round, hurt.tick, &boundaries) {
                grouped.entry(effective).or_default().push(hurt);
            }
        }
        if !grouped.is_empty() {
            hurts_by_round = Some(grouped);
        }
    }

    let mut columns_query = connection.prepare("PRAGMA table_info(rounds)")?;
    let round_columns = columns_query
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    let optional_round = |name: &str| {
        if round_columns.iter().any(|column| column == name) {
            name.to_owned()
        } else {
            "NULL".to_owned()
        }
    };

    let rounds_sql = format!(
        "SELECT round_number, winner, start_tick, end_tick, {}, {}, {}, {}, {}, {}
         FROM rounds WHERE match_id = ?1 ORDER BY round_number",
        optional_round("freeze_end_tick"),
        optional_round("end_reason"),
        optional_round("ct_equip_value"),
        optional_round("t_equip_value"),
        optional_round("team_a_side"),
        optional_round("team_b_side"),
    );
    let mut rounds_query = connection.prepare(&rounds_sql)?;
    let rounds = rounds_query
        .query_map([match_id], |row| {
            let round_number = row.get::<_, i64>(0)?;
            Ok(RoundSummary {
                round_number,
                winner: row.get(1)?,
                start_tick: row.get(2)?,
                freeze_end_tick: row.get(4)?,
                end_tick: row.get(3)?,
                end_reason: row.get(5)?,
                ct_equip_value: row.get(6)?,
                t_equip_value: row.get(7)?,
                team_a_side: row.get(8)?,
                team_b_side: row.get(9)?,
                combat: combat_by_round
                    .get(&round_number)
                    .map(|records| RoundCombat {
                        ct_players: records
                            .iter()
                            .filter(|item| item.side.as_deref() == Some("CT"))
                            .count() as i64,
                        t_players: records
                            .iter()
                            .filter(|item| item.side.as_deref() == Some("T"))
                            .count() as i64,
                        unassigned_players: records
                            .iter()
                            .filter(|item| item.side.is_none())
                            .count() as i64,
                        enemy_kills: records
                            .iter()
                            .try_fold(0_i64, |sum, item| Some(sum + item.enemy_kills?)),
                    }),
                deaths: deaths_by_round
                    .as_ref()
                    .map(|groups| groups.get(&round_number).cloned().unwrap_or_default()),
                hurts: hurts_by_round
                    .as_ref()
                    .map(|groups| groups.get(&round_number).cloned().unwrap_or_default()),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut players_query = connection.prepare(
        "SELECT steamid, name FROM players WHERE match_id = ?1 ORDER BY name COLLATE NOCASE",
    )?;
    let players = players_query
        .query_map([match_id], |row| {
            Ok(PlayerSummary {
                steamid: row.get(0)?,
                name: row.get(1)?,
                combat: combat_by_player
                    .get(&row.get::<_, String>(0)?)
                    .map(|records| PlayerCombat {
                        stable_team: records
                            .first()
                            .and_then(|first| first.stable_team.clone())
                            .filter(|team| {
                                records
                                    .iter()
                                    .all(|item| item.stable_team.as_ref() == Some(team))
                            }),
                        enemy_kills: records
                            .iter()
                            .try_fold(0_i64, |sum, item| Some(sum + item.enemy_kills?)),
                        deaths: records.iter().map(|item| item.deaths).sum(),
                        enemy_assists: records
                            .iter()
                            .try_fold(0_i64, |sum, item| Some(sum + item.enemy_assists?)),
                        enemy_health_damage: records
                            .iter()
                            .try_fold(0_i64, |sum, item| Some(sum + item.enemy_health_damage?)),
                        effective_enemy_health_damage: records
                            .iter()
                            .try_fold(0_i64, |sum, item| {
                                Some(sum + item.effective_enemy_health_damage?)
                            }),
                        rounds: records.clone(),
                    }),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let events_sql = if has_verified_effect_view(&connection)? {
        "SELECT e.id, e.round_number, e.utility_type, e.thrower_name, e.thrower_team,
                e.throw_tick, v.enemies_blinded, v.damage_enemy
         FROM utility_events e LEFT JOIN utility_events_supported v ON v.id = e.id
         WHERE e.match_id = ?1 ORDER BY e.round_number, e.throw_tick, e.id"
    } else {
        "SELECT id, round_number, utility_type, thrower_name, thrower_team,
                throw_tick, NULL AS enemies_blinded, NULL AS damage_enemy
         FROM utility_events WHERE match_id = ?1 ORDER BY round_number, throw_tick, id"
    };
    let mut events_query = connection.prepare(events_sql)?;
    let utility_events = events_query
        .query_map([match_id], |row| {
            Ok(UtilityEvent {
                id: row.get(0)?,
                round_number: row.get(1)?,
                utility_type: row.get(2)?,
                thrower_name: row.get(3)?,
                thrower_team: row.get(4)?,
                throw_tick: row.get(5)?,
                enemies_blinded: row.get(6)?,
                damage_enemy: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut breakdown_query = connection.prepare(
        "SELECT utility_type, COUNT(*) FROM utility_events
         WHERE match_id = ?1 GROUP BY utility_type ORDER BY COUNT(*) DESC, utility_type",
    )?;
    let utility_breakdown = breakdown_query
        .query_map(params![match_id], |row| {
            Ok(UtilityCount {
                utility_type: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(MatchDetail {
        summary,
        rounds,
        players,
        utility_events,
        utility_breakdown,
        full_combat_coverage,
    })
}
