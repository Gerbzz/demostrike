//! Offline editorial handoff for a verified CSthrowback capture request.
//! No clips, media assets, receipts, or recording claims are created here.
use crate::capture_request::{
    CaptureRequest, PlannedTake, build_capture_request, save_request_noclobber,
};
use clipstrike_core::project::{Marker, Project, Track};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const EDIT_HANDOFF_VERSION: u32 = 2;
pub const DEFAULT_LINEUP_HOLD_MS: i64 = 1_200;
pub const EDITOR_FRAME_RATE: u32 = 60;

fn frame_at_ms(ms: i64) -> Result<i64, String> {
    if ms < 0 {
        return Err("negative editor timeline time".into());
    }
    i64::try_from((i128::from(ms) * i128::from(EDITOR_FRAME_RATE) + 500) / 1_000)
        .map_err(|_| "editor frame number overflows".into())
}

fn ms_at_frame(frame: i64) -> Result<i64, String> {
    if frame < 0 {
        return Err("negative editor frame number".into());
    }
    i64::try_from(
        (i128::from(frame) * 1_000 + i128::from(EDITOR_FRAME_RATE / 2))
            / i128::from(EDITOR_FRAME_RATE),
    )
    .map_err(|_| "editor time overflows".into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditSpan {
    pub kind: String,
    pub camera_role: String,
    pub source_start_tick: i64,
    pub source_end_tick: i64,
    pub timeline_start_ms: i64,
    pub timeline_end_ms: i64,
    pub timeline_start_frame: i64,
    pub timeline_end_frame: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineEditPlan {
    pub version: u32,
    pub status: String,
    pub capture_request: CaptureRequest,
    pub timeline_origin_tick: i64,
    pub lineup_hold_tick: i64,
    pub lineup_hold_ms: i64,
    pub lineup_hold_frames: i64,
    pub throw_timeline_ms: i64,
    pub throw_timeline_frame: i64,
    pub editor_frame_rate: u32,
    pub spans: Vec<EditSpan>,
    pub editor_project: String,
    pub capture_request_file: String,
    pub filmcraft_project_file: Option<String>,
    pub media_state: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedEditHandoff {
    pub directory: PathBuf,
    pub plan_path: PathBuf,
    pub project_path: PathBuf,
    pub request_path: PathBuf,
    pub filmcraft_project_path: Option<PathBuf>,
    pub event_id: i64,
    pub demo_sha256: String,
    pub throw_timeline_ms: i64,
    pub lineup_hold_ms: i64,
}

fn source_ms(origin: i64, tick: i64, rate: f64) -> Result<i64, String> {
    let value = ((tick - origin) as f64 * 1_000.0 / rate).round();
    if !value.is_finite() || value < 0.0 || value > i64::MAX as f64 {
        return Err("capture timing exceeds the editor timeline".into());
    }
    Ok(value as i64)
}

pub fn build_edit_plan(
    request: CaptureRequest,
    lineup_hold_ms: i64,
) -> Result<OfflineEditPlan, String> {
    if !(250..=5_000).contains(&lineup_hold_ms) {
        return Err("lineup pause must be between 250 and 5000 milliseconds".into());
    }
    if request.version != crate::capture_request::CAPTURE_REQUEST_VERSION
        || !request.tick_rate.is_finite()
        || !(1.0..=1024.0).contains(&request.tick_rate)
        || request.throw_tick < 0
        || request.detonate_tick <= request.throw_tick
        || request.demo_sha256.len() != 64
        || !request
            .demo_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("unsupported or incomplete source-bound capture request".into());
    }
    let roles = ["player_pov", "third_person_reference", "projectile"];
    if request.takes.len() != roles.len()
        || roles.iter().any(|role| {
            request
                .takes
                .iter()
                .filter(|take| take.camera_role == *role)
                .count()
                != 1
        })
    {
        return Err(
            "capture request must contain one POV, third-person, and projectile take".into(),
        );
    }
    let origin = request
        .takes
        .iter()
        .map(|take| take.start_tick)
        .min()
        .unwrap();
    let pre_release = (request.tick_rate * 0.75).round() as i64;
    let hold_tick = request.throw_tick.saturating_sub(pre_release).max(
        request
            .takes
            .iter()
            .filter(|take| take.camera_role == "third_person_reference")
            .map(|take| take.start_tick)
            .min()
            .unwrap(),
    );
    if hold_tick <= origin || hold_tick >= request.throw_tick {
        return Err(
            "throw has no common pre-release lineup moment for synchronized cameras".into(),
        );
    }
    let hold_start = source_ms(origin, hold_tick, request.tick_rate)?;
    let hold_end = hold_start
        .checked_add(lineup_hold_ms)
        .ok_or("lineup hold overflow")?;
    let mut spans = Vec::new();
    for role in roles {
        let take: &PlannedTake = request
            .takes
            .iter()
            .find(|take| take.camera_role == role)
            .unwrap();
        if take.start_tick < 0
            || take.start_tick >= take.end_tick
            || (role != "projectile"
                && (take.start_tick >= hold_tick || take.end_tick <= request.throw_tick))
            || (role == "projectile" && take.start_tick != request.throw_tick)
        {
            return Err(format!("invalid {role} capture take range"));
        }
        let start = source_ms(origin, take.start_tick, request.tick_rate)?;
        let end = source_ms(origin, take.end_tick, request.tick_rate)?;
        if role == "projectile" {
            spans.push(EditSpan {
                kind: "play".into(),
                camera_role: role.into(),
                source_start_tick: take.start_tick,
                source_end_tick: take.end_tick,
                timeline_start_ms: start + lineup_hold_ms,
                timeline_end_ms: end + lineup_hold_ms,
                timeline_start_frame: 0,
                timeline_end_frame: 0,
            });
        } else {
            spans.push(EditSpan {
                kind: "play".into(),
                camera_role: role.into(),
                source_start_tick: take.start_tick,
                source_end_tick: hold_tick,
                timeline_start_ms: start,
                timeline_end_ms: hold_start,
                timeline_start_frame: 0,
                timeline_end_frame: 0,
            });
            spans.push(EditSpan {
                kind: "hold".into(),
                camera_role: role.into(),
                source_start_tick: hold_tick,
                source_end_tick: hold_tick,
                timeline_start_ms: hold_start,
                timeline_end_ms: hold_end,
                timeline_start_frame: 0,
                timeline_end_frame: 0,
            });
            spans.push(EditSpan {
                kind: "play".into(),
                camera_role: role.into(),
                source_start_tick: hold_tick,
                source_end_tick: take.end_tick,
                timeline_start_ms: hold_end,
                timeline_end_ms: end + lineup_hold_ms,
                timeline_start_frame: 0,
                timeline_end_frame: 0,
            });
        }
    }
    let raw_throw_ms = source_ms(origin, request.throw_tick, request.tick_rate)?
        .checked_add(lineup_hold_ms)
        .ok_or("throw timeline overflow")?;
    for span in &mut spans {
        span.timeline_start_frame = frame_at_ms(span.timeline_start_ms)?;
        span.timeline_end_frame = frame_at_ms(span.timeline_end_ms)?;
        if span.timeline_end_frame <= span.timeline_start_frame {
            return Err(format!(
                "{} {} span is shorter than one editor frame",
                span.camera_role, span.kind
            ));
        }
        span.timeline_start_ms = ms_at_frame(span.timeline_start_frame)?;
        span.timeline_end_ms = ms_at_frame(span.timeline_end_frame)?;
    }
    let lineup_hold_frames = frame_at_ms(hold_end)? - frame_at_ms(hold_start)?;
    let throw_timeline_frame = frame_at_ms(raw_throw_ms)?;
    let throw_timeline_ms = ms_at_frame(throw_timeline_frame)?;
    Ok(OfflineEditPlan {
        version: EDIT_HANDOFF_VERSION,
        status: "planning_only".into(),
        capture_request: request,
        timeline_origin_tick: origin,
        lineup_hold_tick: hold_tick,
        lineup_hold_ms,
        lineup_hold_frames,
        throw_timeline_ms,
        throw_timeline_frame,
        editor_frame_rate: EDITOR_FRAME_RATE,
        spans,
        editor_project: "ClipStrike .cstrike with empty editorial lanes and editable cue markers"
            .into(),
        capture_request_file: "capture-request.json".into(),
        filmcraft_project_file: cfg!(feature = "offline-filmcraft")
            .then(|| "offline-edit.fcproj".into()),
        media_state: "unrecorded; no accepted capture package, clip, or audio".into(),
    })
}

pub fn build_from_library(
    database: &Path,
    event_id: i64,
    lineup_hold_ms: i64,
) -> Result<OfflineEditPlan, String> {
    build_edit_plan(build_capture_request(database, event_id)?, lineup_hold_ms)
}

/// A real ClipStrike project: its native schema, empty named tracks, and
/// editable markers. The sidecar holds exact source ticks and synchronized
/// editorial spans until reviewed media can be imported through capture_project.
pub fn clipstrike_project(plan: &OfflineEditPlan) -> Project {
    let request = &plan.capture_request;
    let mut project = Project::new(
        &format!(
            "{} round {} {} offline plan",
            request.map, request.round, request.utility_type
        ),
        1920,
        1080,
        60.0,
    );
    project.tracks = vec![
        Track::new(1, "Main view - player POV (unrecorded)"),
        Track::new(2, "Third person reference (unrecorded)"),
        Track::new(3, "Projectile path (unrecorded)"),
    ];
    let hold = plan
        .spans
        .iter()
        .find(|span| span.camera_role == "player_pov" && span.kind == "hold")
        .unwrap();
    let hold_start = hold.timeline_start_ms;
    let detonate_ms = source_ms(
        plan.timeline_origin_tick,
        request.detonate_tick,
        request.tick_rate,
    )
    .unwrap()
        + plan.lineup_hold_ms;
    let detonate_ms = ms_at_frame(frame_at_ms(detonate_ms).unwrap()).unwrap();
    let cues = [
        (hold_start, "Lineup pause begins - both cameras"),
        (hold.timeline_end_ms, "Lineup pause ends - both cameras"),
        (plan.throw_timeline_ms, "Grenade release - synchronized"),
        (detonate_ms, "Detonation"),
    ];
    project.markers = cues
        .into_iter()
        .enumerate()
        .map(|(index, (time_ms, label))| Marker {
            id: index + 1,
            time_ms,
            label: label.into(),
            color: [244, 180, 79, 255],
        })
        .collect();
    project
}

#[cfg(feature = "offline-filmcraft")]
pub fn filmcraft_project_bytes(plan: &OfflineEditPlan) -> Result<Vec<u8>, String> {
    use filmcraft_project::{
        ItemKind, Label, Marker as FilmcraftMarker, MarkerId, MarkerKind,
        Project as FilmcraftProject, ProjectView, SequenceSettings, SequenceView,
    };
    use filmcraft_time::{FrameRate, Tick};

    let request = &plan.capture_request;
    let mut project = FilmcraftProject::new(&format!(
        "{} round {} {} offline plan",
        request.map, request.round, request.utility_type
    ));
    project.notes = format!(
        "Unrecorded editorial plan. No approved media or capture receipt. Source SHA-256: {}. \
         Exact camera tick ranges and synchronized lineup hold: capture-edit-plan.json. \
         CSthrowback source request: capture-request.json.",
        request.demo_sha256
    );
    let settings = SequenceSettings {
        frame_rate: FrameRate::FPS_60,
        preset: "HD 1080p 60".into(),
        ..SequenceSettings::default()
    };
    let id = project.new_sequence("Verified throw - editorial cues", settings, 3, 0, None);
    let marker_ids: Vec<_> = (0..4).map(|_| MarkerId(project.alloc_id())).collect();
    let item = project
        .items
        .get_mut(&id)
        .ok_or("FilmCraft sequence was not created")?;
    let ItemKind::Sequence(sequence) = &mut item.kind else {
        return Err("FilmCraft sequence has an unexpected item kind".into());
    };
    let sequence = std::sync::Arc::make_mut(sequence);
    for (track, name) in sequence.video_tracks.iter_mut().zip([
        "Main view - player POV (unrecorded)",
        "Third person reference (unrecorded)",
        "Projectile path (unrecorded)",
    ]) {
        track.name = name.into();
    }
    let hold = plan
        .spans
        .iter()
        .find(|span| span.camera_role == "player_pov" && span.kind == "hold")
        .ok_or("editor plan has no shared lineup hold")?;
    let detonate_ms = source_ms(
        plan.timeline_origin_tick,
        request.detonate_tick,
        request.tick_rate,
    )?
    .checked_add(plan.lineup_hold_ms)
    .ok_or("detonation timeline overflow")?;
    for ((frame, name), marker_id) in [
        (
            hold.timeline_start_frame,
            "Lineup pause begins - both cameras",
        ),
        (hold.timeline_end_frame, "Lineup pause ends - both cameras"),
        (plan.throw_timeline_frame, "Grenade release - synchronized"),
        (frame_at_ms(detonate_ms)?, "Detonation"),
    ]
    .into_iter()
    .zip(marker_ids)
    {
        sequence.markers.push(FilmcraftMarker {
            id: marker_id,
            start: FrameRate::FPS_60.tick_of(frame),
            duration: Tick::ZERO,
            name: name.into(),
            comment: "Planning cue only; no recorded media".into(),
            kind: MarkerKind::Comment,
            color: Label::Mango,
        });
    }
    // An empty sequence normally fits a one-second ruler and hides planning
    // cues several seconds later. Persist a usable initial view without adding
    // fake media to the handoff.
    let cue_end_ms = detonate_ms.max(plan.throw_timeline_ms);
    let cue_pps = (560.0 / (cue_end_ms as f64 / 1_000.0 + 0.75)).clamp(28.0, 100.0);
    let mut view = ProjectView {
        open_sequences: vec![id],
        active_sequence: Some(id),
        ..ProjectView::default()
    };
    view.sequences.insert(
        id,
        SequenceView {
            pps: cue_pps,
            scroll: 0.0,
            v_scroll: 0.0,
            a_scroll: 0.0,
            video_track_h: 52.0,
            audio_track_h: 36.0,
        },
    );
    let bytes = filmcraft_format::encode_with_view(&project, Some(&view), true);
    filmcraft_format::decode(&bytes)
        .map_err(|error| format!("FilmCraft project validation failed: {error}"))?;
    Ok(bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("cannot finish {}: {error}", path.display()))
}

/// The manifest is written last, so its presence signals that both files were
/// completed. Any failed, partial folder remains visible for manual review.
pub fn save_handoff_noclobber(
    plan: &OfflineEditPlan,
    directory: &Path,
) -> Result<SavedEditHandoff, String> {
    fs::create_dir(directory).map_err(|error| {
        format!(
            "cannot create new handoff folder {}: {error}",
            directory.display()
        )
    })?;
    let project_path = directory.join("offline-edit.cstrike");
    #[cfg(feature = "offline-filmcraft")]
    let filmcraft_project_path = Some(directory.join("offline-edit.fcproj"));
    #[cfg(not(feature = "offline-filmcraft"))]
    let filmcraft_project_path: Option<PathBuf> = None;
    let request_path = directory.join("capture-request.json");
    let plan_path = directory.join("capture-edit-plan.json");
    let project = clipstrike_project(plan);
    write_new(
        &project_path,
        &project.to_json_bytes().map_err(|error| error.to_string())?,
    )?;
    #[cfg(feature = "offline-filmcraft")]
    write_new(
        filmcraft_project_path.as_ref().unwrap(),
        &filmcraft_project_bytes(plan)?,
    )?;
    save_request_noclobber(&plan.capture_request, &request_path)?;
    let bytes = serde_json::to_vec_pretty(plan).map_err(|error| error.to_string())?;
    write_new(&plan_path, &bytes)?;
    Ok(SavedEditHandoff {
        directory: directory.to_path_buf(),
        plan_path,
        project_path,
        request_path,
        filmcraft_project_path,
        event_id: plan.capture_request.event_id,
        demo_sha256: plan.capture_request.demo_sha256.clone(),
        throw_timeline_ms: plan.throw_timeline_ms,
        lineup_hold_ms: plan.lineup_hold_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Vec3;
    use std::path::PathBuf;

    fn request() -> CaptureRequest {
        CaptureRequest {
            version: 1,
            demo_path: PathBuf::from("public.dem"),
            demo_sha256: "a".repeat(64),
            demo_bytes: 60_601_900,
            match_id: 1,
            event_id: 9,
            map: "de_mirage".into(),
            round: 2,
            player_steamid64: "76561198000000001".into(),
            utility_type: "smoke".into(),
            projectile_entity_id: 51,
            observer_slot_candidate: 3,
            tick_rate: 64.0,
            throw_tick: 1_000,
            detonate_tick: 1_300,
            throw_position: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            detonate_position: Vec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            flight_samples: vec![[1000.0, 1.0, 2.0, 3.0], [1300.0, 4.0, 5.0, 6.0]],
            takes: vec![
                PlannedTake {
                    camera_role: "player_pov".into(),
                    start_tick: 776,
                    end_tick: 1032,
                },
                PlannedTake {
                    camera_role: "projectile".into(),
                    start_tick: 1000,
                    end_tick: 1364,
                },
                PlannedTake {
                    camera_role: "third_person_reference".into(),
                    start_tick: 872,
                    end_tick: 1032,
                },
            ],
        }
    }

    #[test]
    fn two_views_share_one_lineup_pause_and_release_time() {
        let plan = build_edit_plan(request(), DEFAULT_LINEUP_HOLD_MS).unwrap();
        assert_eq!(plan.lineup_hold_tick, 952);
        assert_eq!(plan.throw_timeline_ms, 4700);
        assert_eq!(plan.editor_frame_rate, 60);
        assert_eq!(plan.lineup_hold_frames, 72);
        assert_eq!(plan.throw_timeline_frame, 282);
        let pov: Vec<_> = plan
            .spans
            .iter()
            .filter(|span| span.camera_role == "player_pov")
            .collect();
        let third: Vec<_> = plan
            .spans
            .iter()
            .filter(|span| span.camera_role == "third_person_reference")
            .collect();
        assert_eq!(
            (pov[1].timeline_start_ms, pov[1].timeline_end_ms),
            (third[1].timeline_start_ms, third[1].timeline_end_ms)
        );
        assert_eq!(
            (pov[1].timeline_start_frame, pov[1].timeline_end_frame),
            (third[1].timeline_start_frame, third[1].timeline_end_frame)
        );
        assert_eq!(pov[2].timeline_end_ms, third[2].timeline_end_ms);
        assert_eq!(
            plan.spans.last().unwrap().timeline_start_ms,
            plan.throw_timeline_ms
        );
    }

    #[test]
    fn changing_lineup_pause_moves_all_three_release_cues_together() {
        let standard = build_edit_plan(request(), DEFAULT_LINEUP_HOLD_MS).unwrap();
        let longer = build_edit_plan(request(), 2_000).unwrap();
        assert_eq!(longer.throw_timeline_ms - standard.throw_timeline_ms, 800);
        for role in ["player_pov", "third_person_reference", "projectile"] {
            let end = |plan: &OfflineEditPlan| {
                plan.spans
                    .iter()
                    .filter(|span| span.camera_role == role)
                    .last()
                    .unwrap()
                    .timeline_end_ms
            };
            assert_eq!(end(&longer) - end(&standard), 800);
        }
        assert!(
            build_edit_plan(request(), 0)
                .unwrap_err()
                .contains("lineup pause")
        );
    }

    #[test]
    fn native_clipstrike_project_reopens_without_pretend_media() {
        let plan = build_edit_plan(request(), DEFAULT_LINEUP_HOLD_MS).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("handoff");
        let saved = save_handoff_noclobber(&plan, &destination).unwrap();
        let reopened = Project::load(&saved.project_path).unwrap();
        assert_eq!(reopened.tracks.len(), 3);
        assert!(
            reopened.assets.is_empty()
                && reopened.clips.is_empty()
                && reopened.capture_sources.is_empty()
        );
        assert_eq!(reopened.markers[2].time_ms, plan.throw_timeline_ms);
        assert_eq!(
            save_handoff_noclobber(&plan, &destination)
                .unwrap_err()
                .contains("cannot create"),
            true
        );
        let loaded: OfflineEditPlan =
            serde_json::from_slice(&fs::read(saved.plan_path).unwrap()).unwrap();
        let request: CaptureRequest =
            serde_json::from_slice(&fs::read(saved.request_path).unwrap()).unwrap();
        assert_eq!(request.event_id, plan.capture_request.event_id);
        #[cfg(feature = "offline-filmcraft")]
        {
            let filmcraft = fs::read(saved.filmcraft_project_path.unwrap()).unwrap();
            let loaded = filmcraft_format::decode(&filmcraft).unwrap();
            assert_eq!(loaded.project.items.len(), 1);
            assert!(loaded.project.notes.contains("Unrecorded editorial plan"));
            let sequence = loaded.project.items.values().next().unwrap();
            let filmcraft_project::ItemKind::Sequence(sequence) = &sequence.kind else {
                panic!("FilmCraft handoff is not a sequence");
            };
            assert_eq!(sequence.video_tracks.len(), 3);
            assert!(
                sequence
                    .video_tracks
                    .iter()
                    .all(|track| track.items.is_empty())
            );
            assert_eq!(sequence.markers.len(), 4);
        }
        assert_eq!(
            loaded.capture_request.demo_sha256,
            plan.capture_request.demo_sha256
        );
    }
}
