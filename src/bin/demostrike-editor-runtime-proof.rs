//! Hosted-only acceptance of the public Finder handoff in the embedded editor.
//! Synthetic still images stand in for unrecorded takes; no capture is claimed.
#[path = "../filmcraft_legacy.rs"]
mod filmcraft_legacy;

use demostrike::edit_handoff::{EditSpan, OfflineEditPlan};
use filmcraft_engine::Session;
use filmcraft_project::ClipId;
use filmcraft_time::{FrameRate, Tick};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn frame_tick(frame: i64) -> Tick {
    FrameRate::FPS_60.tick_of(frame)
}

fn frame_at_ms(ms: i64) -> i64 {
    ((i128::from(ms) * 60 + 500) / 1_000) as i64
}

fn fail(message: impl Into<String>) -> String {
    message.into()
}

fn file_sha(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn synthetic_image(path: &Path, index: u8) -> Result<(), String> {
    const W: u32 = 320;
    const H: u32 = 180;
    let colors = [
        [32, 113, 185],
        [49, 180, 126],
        [202, 108, 62],
        [129, 99, 195],
        [222, 176, 65],
    ];
    let color = colors[index as usize];
    let mut pixels = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        for x in 0..W {
            let stripe = x > 16 && x < 304 && y > 16 && y < 164 && ((x / 20 + y / 20) % 2 == 0);
            let rgb = if stripe { color } else { [16, 24, 36] };
            pixels.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    let png = filmcraft_export::encode_png(pixels, W, H).map_err(|e| e.to_string())?;
    fs::write(path, png).map_err(|e| e.to_string())
}

fn open_document(path: &Path) -> Result<Session, String> {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cstrike"))
    {
        filmcraft_legacy::load_and_convert(path).map_err(|e| e.to_string())
    } else {
        let mut session = Session::default();
        session
            .execute("file.open", json!({"path": path.to_string_lossy()}))
            .map_err(|e| e.to_string())?;
        Ok(session)
    }
}

fn assert_cues(session: &Session, plan: &OfflineEditPlan) -> Result<(), String> {
    let seq = session
        .active_sequence()
        .ok_or_else(|| fail("editor did not open the planned sequence"))?;
    if seq.video_tracks.len() != 3 || seq.markers.len() != 4 {
        return Err(format!(
            "editor opened {} video tracks and {} cues",
            seq.video_tracks.len(),
            seq.markers.len()
        ));
    }
    let hold = plan
        .spans
        .iter()
        .find(|s| s.camera_role == "player_pov" && s.kind == "hold")
        .ok_or_else(|| fail("plan has no POV lineup hold"))?;
    let detonate = ((plan.capture_request.detonate_tick - plan.timeline_origin_tick) as f64
        * 1_000.0
        / plan.capture_request.tick_rate)
        .round() as i64
        + plan.lineup_hold_ms;
    let expected = [
        (
            hold.timeline_start_frame,
            "Lineup pause begins - both cameras",
        ),
        (hold.timeline_end_frame, "Lineup pause ends - both cameras"),
        (plan.throw_timeline_frame, "Grenade release - synchronized"),
        (frame_at_ms(detonate), "Detonation"),
    ];
    for (cue, (frame, name)) in seq.markers.iter().zip(expected) {
        if cue.start != frame_tick(frame) || cue.name != name {
            return Err(format!(
                "editor changed cue {:?}; expected {name} at frame {frame}",
                cue
            ));
        }
    }
    Ok(())
}

fn check_placement(session: &Session, expected: &[(u64, usize, EditSpan)]) -> Result<(), String> {
    let seq = session
        .active_sequence()
        .ok_or_else(|| fail("edited sequence disappeared"))?;
    for (clip_id, track_index, span) in expected {
        let (track, item) = seq
            .find_item(ClipId(*clip_id))
            .ok_or_else(|| format!("saved clip {clip_id} disappeared"))?;
        if track != seq.video_tracks[*track_index].id
            || item.start != frame_tick(span.timeline_start_frame)
            || item.duration != frame_tick(span.timeline_end_frame - span.timeline_start_frame)
        {
            return Err(format!("clip {clip_id} changed planned timeline placement"));
        }
    }
    Ok(())
}

fn run_one(
    source: &Path,
    output: &Path,
    plan: &OfflineEditPlan,
    media: &BTreeMap<&str, PathBuf>,
) -> Result<Value, String> {
    let source_sha = file_sha(source)?;
    let mut session = open_document(source)?;
    assert_cues(&session, plan)?;
    if session
        .active_sequence()
        .unwrap()
        .video_tracks
        .iter()
        .any(|track| !track.items.is_empty())
    {
        return Err(fail(
            "unrecorded handoff unexpectedly contains timeline media",
        ));
    }
    let mut imported = BTreeMap::new();
    for (name, path) in media {
        let result = session
            .execute("file.import", json!({"paths": [path.to_string_lossy()]}))
            .map_err(|e| format!("import {name}: {e}"))?;
        let id = result["items"]
            .as_array()
            .and_then(|items| items.first())
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("import {name} returned no item: {result}"))?;
        imported.insert(*name, id);
    }
    let mut placed = Vec::new();
    for span in &plan.spans {
        let (track_index, image) = match (span.camera_role.as_str(), span.kind.as_str()) {
            ("player_pov", "play") => (0, "pov-play"),
            ("player_pov", "hold") => (0, "pov-hold"),
            ("third_person_reference", "play") => (1, "third-play"),
            ("third_person_reference", "hold") => (1, "third-hold"),
            ("projectile", "play") => (2, "projectile"),
            _ => {
                return Err(format!(
                    "unknown planned role/kind: {}/{}",
                    span.camera_role, span.kind
                ));
            }
        };
        let id = imported[image];
        let result = session
            .execute(
                "timeline.place",
                json!({
                    "item": id, "track": format!("V{}", track_index + 1),
                    "time": frame_tick(span.timeline_start_frame).0,
                    "duration": frame_tick(span.timeline_end_frame - span.timeline_start_frame).0,
                    "video": true, "audio": false, "insert": false,
                }),
            )
            .map_err(|e| {
                format!(
                    "place {image} {}..{} ms: {e}",
                    span.timeline_start_ms, span.timeline_end_ms
                )
            })?;
        let clip_id = result["clips"]
            .as_array()
            .and_then(|items| items.first())
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("placement returned no clip: {result}"))?;
        placed.push((clip_id, track_index, span.clone()));
    }
    check_placement(&session, &placed)?;
    assert_cues(&session, plan)?;
    session
        .execute("file.saveAs", json!({"path": output.to_string_lossy()}))
        .map_err(|e| format!("save edited project: {e}"))?;
    let mut reopened = Session::default();
    reopened
        .execute("file.open", json!({"path": output.to_string_lossy()}))
        .map_err(|e| format!("reopen edited project: {e}"))?;
    assert_cues(&reopened, plan)?;
    check_placement(&reopened, &placed)?;
    if file_sha(source)? != source_sha {
        return Err(fail("original offline handoff changed during editing"));
    }
    let pov = plan
        .spans
        .iter()
        .find(|s| s.camera_role == "player_pov" && s.kind == "hold")
        .unwrap();
    let third = plan
        .spans
        .iter()
        .find(|s| s.camera_role == "third_person_reference" && s.kind == "hold")
        .unwrap();
    if (pov.timeline_start_frame, pov.timeline_end_frame)
        != (third.timeline_start_frame, third.timeline_end_frame)
        || pov.timeline_end_frame - pov.timeline_start_frame != plan.lineup_hold_frames
        || (plan.lineup_hold_frames as f64 / 60.0 * 1_000.0 - plan.lineup_hold_ms as f64).abs()
            > 8.4
    {
        return Err(fail(
            "the two views do not share the same deliberate lineup freeze",
        ));
    }
    Ok(json!({
        "input": source.file_name().unwrap().to_string_lossy(),
        "inputSha256": source_sha,
        "saved": output.file_name().unwrap().to_string_lossy(),
        "savedSha256": file_sha(output)?,
        "syntheticClips": placed.len(),
        "cueCount": 4,
        "holdStartMs": pov.timeline_start_ms,
        "holdEndMs": pov.timeline_end_ms,
        "holdStartFrame": pov.timeline_start_frame,
        "holdEndFrame": pov.timeline_end_frame,
        "commonHoldFrames": plan.lineup_hold_frames,
        "commonHoldMs": plan.lineup_hold_ms,
        "reopened": true,
    }))
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [handoff, output] = args.as_slice() else {
        return Err(fail(
            "usage: demostrike-editor-runtime-proof HANDOFF_DIR OUTPUT_DIR",
        ));
    };
    let handoff = PathBuf::from(handoff);
    let output = PathBuf::from(output);
    fs::create_dir(&output).map_err(|e| format!("new output folder: {e}"))?;
    let plan: OfflineEditPlan = serde_json::from_slice(
        &fs::read(handoff.join("capture-edit-plan.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if plan.version != 2
        || plan.editor_frame_rate != 60
        || plan.lineup_hold_frames != 120
        || plan.status != "planning_only"
        || plan.lineup_hold_ms != 2_000
        || plan.spans.len() != 7
        || plan.capture_request.demo_sha256
            != "84a1a4191302bdd2a3bbb5a727842093744b1fb1a228aeec630369e44b622cb2"
    {
        return Err(fail(
            "editor fixture is not the pinned public Finder handoff",
        ));
    }
    let media_dir = output.join("clearly-synthetic-stills");
    fs::create_dir(&media_dir).map_err(|e| e.to_string())?;
    let mut media = BTreeMap::new();
    for (index, name) in [
        "pov-play",
        "pov-hold",
        "third-play",
        "third-hold",
        "projectile",
    ]
    .into_iter()
    .enumerate()
    {
        let path = media_dir.join(format!("SYNTHETIC-{name}.png"));
        synthetic_image(&path, index as u8)?;
        media.insert(name, path);
    }
    let legacy = run_one(
        &handoff.join("offline-edit.cstrike"),
        &output.join("synthetic-edited-clipstrike.fcproj"),
        &plan,
        &media,
    )?;
    let filmcraft = run_one(
        &handoff.join("offline-edit.fcproj"),
        &output.join("synthetic-edited-filmcraft.fcproj"),
        &plan,
        &media,
    )?;
    let proof = json!({
        "scope": "actual FilmCraft engine session using the same legacy converter and native project opener as DemoStrike; synthetic stills only",
        "sourceDemoSha256": plan.capture_request.demo_sha256,
        "sourceEventId": plan.capture_request.event_id,
        "legacyClipStrike": legacy,
        "filmcraft": filmcraft,
    });
    fs::write(
        output.join("runtime-proof.json"),
        serde_json::to_vec_pretty(&proof).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!("EDITOR_RUNTIME_PROOF {}", proof);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("EDITOR_RUNTIME_FAILURE {error}");
        std::process::exit(1);
    }
}
