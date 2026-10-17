use std::path::Path;
use demostrike::demo_import::DemoFactsParser;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command, demo_path] if command == "probe-demo" || command == "probe-grenades" => {
            let path = Path::new(demo_path);
            demostrike::demo_import::validate_demo_file(path)?;
            let raw = demostrike::native_grenade_parser::NativeGrenadeParser
                .parse(path, &demostrike::demo_import::ImportControl::new())?;
            let samples: usize = raw.trajectories.iter().map(|t| t.points.len()).sum();
            println!("{}: {} releases, {} trajectories ({} samples), {} detonations, {} blinds, {} utility hurts, {} relevant deaths, {} total combat hurts, {} total combat deaths, {} nearby-player samples, {} round transitions; capability={} (economy unavailable)",
                raw.header.map_name, raw.throws.len(), raw.trajectories.len(), samples,
                raw.detonations.len(), raw.blinds.len(), raw.hurts.len(), raw.deaths.len(),
                raw.combat_hurts.len(), raw.combat_deaths.len(), raw.positions.len(), raw.round_events.len(),
                demostrike::native_grenade_parser::NativeGrenadeParser.capabilities().key());
            Ok(())
        }
        [command, demo_path, database] if command == "import-demo" || command == "import-grenades" => {
            let parser = demostrike::native_grenade_parser::NativeGrenadeParser;
            let control = demostrike::demo_import::ImportControl::new();
            let imported = demostrike::demo_import::import_demo(
                Path::new(demo_path), Path::new(database), &parser, &control, |_| {}
            )?;
            println!("match {}: {} grenade events on {}; capability={} (economy and capture unavailable)",
                imported.match_id, imported.analyzed.events.len(), imported.analyzed.map_name,
                imported.capabilities.key());
            Ok(())
        }
        [command, database, event_id, destination] if command == "plan-capture" => {
            let event_id: i64 = event_id.parse().map_err(|_| "event ID must be an integer".to_string())?;
            let request = demostrike::capture_request::build_capture_request(
                Path::new(database), event_id)?;
            demostrike::capture_request::save_request_noclobber(&request, Path::new(destination))?;
            println!("planned {} {} event {}: demo {}, throw tick {}, detonation tick {}, {} source takes; observer slot {} remains unverified in CS2",
                request.map, request.utility_type, request.event_id, request.demo_sha256,
                request.throw_tick, request.detonate_tick, request.takes.len(), request.observer_slot_candidate);
            Ok(())
        }
        [command, request, receipt, destination] if command == "seal-capture" => {
            let package = demostrike::capture_receipt::seal_reviewed_capture(
                Path::new(request), Path::new(receipt), Path::new(destination))?;
            println!("sealed {} reviewed source clips in {} using capture-package v1",
                package.manifest.clips.len(), package.root.display());
            Ok(())
        }
        [command, package_dir, project_path] if command == "import-capture-package" => {
            let imported = demostrike::capture_project::import_package_to_new_project(
                Path::new(package_dir), Path::new(project_path))
                .map_err(|error| error.to_string())?;
            println!("imported {} accepted clips into new editable project {}",
                imported.project.clips.len(), project_path);
            for clip in &imported.coverage {
                println!("{}: editor {} ms; video {} ms; audio {} ms; accepted container {} ms; audio beyond video {} ms",
                    clip.clip_id, clip.editor_duration_ms, clip.video_duration_ms,
                    clip.audio_duration_ms, clip.declared_duration_ms, clip.audio_only_tail_ms());
            }
            Ok(())
        }
        [command, request, cs2, hlae, ffmpeg, ffprobe] if command == "preflight-capture" => {
            let report = demostrike::capture_preflight::preflight_capture(Path::new(request),
                &demostrike::capture_preflight::CaptureTools {
                    cs2_exe: cs2.into(), hlae_exe: hlae.into(),
                    ffmpeg_exe: ffmpeg.into(), ffprobe_exe: ffprobe.into(),
                })?;
            println!("{}", serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?);
            if report.issues.is_empty() { Ok(()) }
            else { Err(format!("capture preflight has {} blocking issue(s); no game was launched", report.issues.len())) }
        }
        [command, request, cs2, hlae, ffmpeg, ffprobe, destination] if command == "stage-pov-pilot" => {
            let manifest = demostrike::capture_pilot::stage_pov_pilot(Path::new(request),
                &demostrike::capture_preflight::CaptureTools {
                    cs2_exe: cs2.into(), hlae_exe: hlae.into(),
                    ffmpeg_exe: ffmpeg.into(), ffprobe_exe: ffprobe.into(),
                }, Path::new(destination))?;
            println!("staged unreviewed offline POV pilot: {}", manifest.display());
            Ok(())
        }
        [command, manifest] if command == "launch-pov-pilot" => {
            println!("{}", demostrike::capture_pilot::launch_staged_pilot(Path::new(manifest))?);
            Ok(())
        }
        [command, request, cs2, hlae, ffmpeg, ffprobe, destination] if command == "stage-event-pilot" => {
            let manifest = demostrike::event_pilot::stage_event_pilot(Path::new(request),
                &demostrike::capture_preflight::CaptureTools {
                    cs2_exe: cs2.into(), hlae_exe: hlae.into(),
                    ffmpeg_exe: ffmpeg.into(), ffprobe_exe: ffprobe.into(),
                }, Path::new(destination))?;
            println!("staged unreviewed three-role offline event pilot: {}", manifest.display());
            Ok(())
        }
        [command, manifest] if command == "launch-event-pilot" => {
            println!("{}", demostrike::event_pilot::launch_event_pilot(Path::new(manifest))?);
            Ok(())
        }
        [command, demo_path] if command == "inspect-demo" => {
            let path = Path::new(demo_path);
            demostrike::demo_import::validate_demo_file(path)?;
            let bytes = std::fs::metadata(path).map_err(|error| error.to_string())?.len();
            println!("{}: CS2 PBDEMS2 header, {bytes} bytes; events not parsed", path.display());
            Ok(())
        }
        [command, raw_path] if command == "analyze" => {
            let path = Path::new(raw_path);
            let bytes = std::fs::read(path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let analyzed = demostrike::analyze_raw_json(&bytes)
                .map_err(|error| error.to_string())?;
            println!(
                "{}: {} utility events, {} rounds, {} players, {:.1} ticks/s",
                analyzed.map_name,
                analyzed.events.len(),
                analyzed.rounds.len(),
                analyzed.players.len(),
                analyzed.tick_rate
            );
            Ok(())
        }
        [command, draft_path, destination] if command == "package" => {
            let draft = demostrike::package::load_draft(Path::new(draft_path))
                .map_err(|error| error.to_string())?;
            let package = demostrike::package::finalize_stream_probed_capture_package(
                Path::new(destination), &draft.package_id, &draft.inputs,
            ).map_err(|error| error.to_string())?;
            println!("{}: {} separate stream-probed clips; audible sound and HLAE origin still require review", package.root.display(), package.manifest.clips.len());
            Ok(())
        }
        [command, draft_path, destination] if command == "package-structural" => {
            let draft = demostrike::package::load_draft(Path::new(draft_path))
                .map_err(|error| error.to_string())?;
            let package = demostrike::package::finalize_capture_package(
                Path::new(destination), &draft.package_id, &draft.inputs,
            ).map_err(|error| error.to_string())?;
            println!("{}: {} separate clips; STRUCTURAL ONLY, media and audio unverified", package.root.display(), package.manifest.clips.len());
            Ok(())
        }
        [command, video, wav] if command == "probe-pair" => {
            let (picture, sound) = demostrike::media_probe::probe_capture_pair(Path::new(video), Path::new(wav))
                .map_err(|error| error.to_string())?;
            println!("video {}x{} {}/{} fps {:.3}s; WAV {} channels {} Hz {:.3}s; stream headers only, sound not verified",
                picture.width, picture.height, picture.fps_num, picture.fps_den, picture.duration_seconds,
                sound.channels, sound.sample_rate, sound.duration_seconds);
            Ok(())
        }
        [command, destination] if command == "recover" => {
            let package = demostrike::package::recover_complete_staged_package(Path::new(destination))
                .map_err(|error| error.to_string())?;
            println!("recovered {}: {} clips", package.root.display(), package.manifest.clips.len());
            Ok(())
        }
        _ => Err("usage: demostrike-cli probe-demo <demo.dem> | import-demo <demo.dem> <matches.sqlite> | plan-capture <matches.sqlite> <event-id> <new-request.json> | preflight-capture <request.json> <cs2.exe> <HLAE.exe> <ffmpeg.exe> <ffprobe.exe> | stage-pov-pilot/stage-event-pilot <request.json> <cs2.exe> <HLAE.exe> <ffmpeg.exe> <ffprobe.exe> <new-folder> | launch-pov-pilot/launch-event-pilot <manifest.json> | seal-capture <request.json> <reviewed-receipt.json> <new-package-folder> | import-capture-package <accepted-package-folder> <new-project.cstrike> | inspect-demo <demo.dem> | analyze <raw-demo-facts.json> | package <draft.json> <new-package-folder> | package-structural <draft.json> <new-package-folder> | probe-pair <video.mp4> <audio.wav> | recover <package-folder>".into()),
    }
}

