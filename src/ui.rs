use crate::analysis::AnalyzedMatch;
use crate::analyze_raw_json;
use crate::capture_request::{build_capture_request, save_request_noclobber};
use crate::capture_receipt::{ProofFrameDraft, ReviewTakeDraft};
use crate::capture_qc::{plan_qc, run_qc_with_cancel, QcResult};
use crate::capture_preflight::CaptureTools;
use crate::event_pilot::{event_launch_spec, stage_event_pilot};
use crate::owned_capture::run_owned;
use crate::demo_import::{DemoFactsParser, ExternalExtractor, ImportControl, ImportedMatch, ParserCapabilities, import_demo, validate_demo_file};
use crate::db::{self, queries};
use crate::domain::{EventFilter, MatchSummary, UtilityEventDetail, UtilityEventSummary};
use crate::native_grenade_parser::NativeGrenadeParser;
use crate::package::{PackageDraft, finalize_stream_probed_capture_package, load_draft};
use eframe::egui;
use filmcraft_engine::{Session, autosave::AutosaveConfig};
use filmcraft_ui_egui::{FilmcraftApp, state::Mode};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

// Public FilmCraft app-host modules adapted from the pinned source commit.
// See ATTRIBUTION.md and third_party/filmcraft for license and provenance.
#[path = "filmcraft_legacy.rs"]
mod filmcraft_legacy;
#[path = "filmcraft_audio.rs"]
mod filmcraft_audio;

fn load_editor_session(path: &Path) -> Result<(Session, bool), String> {
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or_default();
    if extension.eq_ignore_ascii_case("cstrike") {
        return filmcraft_legacy::load_and_convert(path)
            .map(|session| (session, true))
            .map_err(|error| error.to_string());
    }
    if extension.eq_ignore_ascii_case("fcproj") {
        let mut session = Session::default();
        session.execute("file.open", json!({"path": path.to_string_lossy()}))
            .map_err(|error| format!("could not open FilmCraft project: {error}"))?;
        return Ok((session, false));
    }
    Err("open a .cstrike capture project or a .fcproj FilmCraft project".into())
}

#[derive(Debug, PartialEq, Eq)]
enum EditorReturnGate { Busy, ConfirmDiscard, Ready }

fn editor_return_gate(export_active: bool, stepped_active: bool, dirty: bool,
    discard_confirmed: bool) -> EditorReturnGate {
    if export_active || stepped_active { EditorReturnGate::Busy }
    else if dirty && !discard_confirmed { EditorReturnGate::ConfirmDiscard }
    else { EditorReturnGate::Ready }
}

fn editor_return_state(editor: &FilmcraftApp, discard_confirmed: bool) -> EditorReturnGate {
    editor_return_gate(editor.session.export_queue.is_active(), !editor.session.stepped.is_empty(),
        editor.session.is_dirty(), discard_confirmed)
}

enum ImportMessage {
    Stage(&'static str),
    Finished(Result<ImportedMatch, String>),
}

enum PilotOutcome {
    Staged(PathBuf), Ran, Sealed(PathBuf), Reviewed(PathBuf),
    PreviewPrepared(usize, String, String),
    IdentityLogs(Vec<(String, PathBuf)>),
    QcPrepared(QcResult),
}
enum PilotMessage { Progress(String), Finished(Result<PilotOutcome, String>) }

pub struct DemoStrikeApp {
    pub analyzed: Option<AnalyzedMatch>,
    pub error: Option<String>,
    capabilities: Option<ParserCapabilities>,
    source_label: Option<String>,
    pub draft: Option<PackageDraft>,
    draft_label: Option<String>,
    pub package_destination: String,
    pub package_result: Option<String>,
    pub node_path: String,
    pub extractor_path: String,
    pub database_path: String,
    import_source: Option<PathBuf>,
    import_stage: Option<String>,
    import_messages: Option<mpsc::Receiver<ImportMessage>>,
    import_cancel: Arc<ImportControl>,
    capture_request_messages: Option<mpsc::Receiver<Result<PathBuf, String>>>,
    capture_request_status: Option<String>,
    capture_request_path: String,
    pilot_destination: String,
    pilot_manifest: String,
    pilot_qc_review_name: String,
    review_identity_destination: String,
    reviewed_receipt: String,
    review_takes: Vec<ReviewTakeDraft>,
    review_preview_role: Option<String>,
    receipt_summary: Option<String>,
    proof_frames: Vec<(String, PathBuf, String)>,
    proof_texture: Option<egui::TextureHandle>,
    proof_label: Option<String>,
    reviewed_package_destination: String,
    cs2_exe: String,
    hlae_exe: String,
    ffmpeg_exe: String,
    ffprobe_exe: String,
    pilot_messages: Option<mpsc::Receiver<PilotMessage>>,
    pilot_cancel: Arc<AtomicBool>,
    pilot_cancel_available: bool,
    pilot_status: Option<String>,
    package_import_messages: Option<mpsc::Receiver<Result<(PathBuf, String), String>>>,
    package_import_cancel: Arc<AtomicBool>,
    imported_project_path: Option<PathBuf>,
    package_import_summary: Option<String>,
    editor_render_state: Option<eframe::egui_wgpu::RenderState>,
    embedded_editor: Option<FilmcraftApp>,
    confirm_discard_editor: bool,
    library_matches: Vec<MatchSummary>,
    library_opened: bool,
    library_events: Vec<UtilityEventSummary>,
    library_match_id: Option<i64>,
    library_page: i64,
    library_search: String,
    library_search_applied: String,
    library_favorites_only: bool,
    selected_event: Option<UtilityEventDetail>,
    tag_draft: String,
    capture: Option<CaptureOnce>,
}

struct CaptureOnce {
    path: PathBuf,
    outcome: Arc<Mutex<Option<Result<(), String>>>>,
    started: Instant,
    settle_frames: u32,
    requested: bool,
}

impl Default for DemoStrikeApp {
    fn default() -> Self {
        let data_root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let runtime = std::env::var_os("DEMOSTRIKE_CAPTURE_WORKSPACE")
            .map(PathBuf::from).unwrap_or_else(|| data_root.join("DemoStrike"))
            .join("capture-runtime");
        let ffprobe = data_root.join("Microsoft/WinGet/Links/ffprobe.exe");
        Self {
            analyzed: None, error: None, capabilities: None, source_label: None,
            draft: None, draft_label: None, package_destination: String::new(),
            package_result: None,
            node_path: std::env::var("DEMOSTRIKE_NODE_PATH").unwrap_or_default(),
            extractor_path: std::env::var("DEMOSTRIKE_EXTRACTOR_SCRIPT").unwrap_or_default(),
            database_path: data_root.join("DemoStrike/matches.sqlite").display().to_string(),
            import_source: None, import_stage: None, import_messages: None,
            import_cancel: Arc::new(ImportControl::new()),
            capture_request_messages: None, capture_request_status: None,
            capture_request_path: String::new(),
            pilot_destination: String::new(), pilot_manifest: String::new(),
            pilot_qc_review_name: "review".into(),
            review_identity_destination: String::new(),
            reviewed_receipt: String::new(), reviewed_package_destination: String::new(),
            review_takes: Vec::new(), review_preview_role: None,
            receipt_summary: None, proof_frames: Vec::new(),
            proof_texture: None, proof_label: None,
            cs2_exe: runtime.join("cs2-1.41.6.7/game/bin/win64/cs2.exe").display().to_string(),
            hlae_exe: runtime.join("hlae-2.190.2/portable/HLAE.exe").display().to_string(),
            ffmpeg_exe: runtime.join("hlae-2.190.2/portable/ffmpeg/bin/ffmpeg.exe").display().to_string(),
            ffprobe_exe: ffprobe.display().to_string(),
            pilot_messages: None, pilot_cancel: Arc::new(AtomicBool::new(false)),
            pilot_cancel_available: false,
            pilot_status: None,
            package_import_messages: None, imported_project_path: None,
            package_import_cancel: Arc::new(AtomicBool::new(false)),
            package_import_summary: None,
            editor_render_state: None,
            embedded_editor: None, confirm_discard_editor: false,
            library_matches: Vec::new(), library_opened: false, library_events: Vec::new(), library_match_id: None,
            library_page: 0, library_search: String::new(), library_search_applied: String::new(),
            library_favorites_only: false, selected_event: None, tag_draft: String::new(),
            capture: None,
        }
    }
}

impl Drop for DemoStrikeApp {
    fn drop(&mut self) {
        self.import_cancel.request_cancel();
        self.package_import_cancel.store(true, Ordering::Relaxed);
        self.pilot_cancel.store(true, Ordering::Relaxed);
        if let Some(mut editor) = self.embedded_editor.take() {
            eframe::App::on_exit(&mut editor);
        }
    }
}

impl DemoStrikeApp {
    /// Capture one fully painted native window, then close. Existing files are preserved.
    pub fn capture_once(&mut self, path: PathBuf,
        outcome: Arc<Mutex<Option<Result<(), String>>>>) {
        self.capture = Some(CaptureOnce {
            path, outcome, started: Instant::now(), settle_frames: 0, requested: false,
        });
    }

    fn finish_capture(&mut self, ctx: &egui::Context, result: Result<(), String>) {
        if let Some(capture) = self.capture.take() {
            *capture.outcome.lock().unwrap() = Some(result);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn capture_after_frame(&mut self, ctx: &egui::Context) {
        let Some(capture) = self.capture.as_ref() else { return; };
        if capture.started.elapsed() > Duration::from_secs(15) {
            self.finish_capture(ctx, Err("Timed out waiting for a native screenshot".into()));
            return;
        }
        if capture.requested {
            let image = ctx.input(|input| input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(Arc::clone(image)),
                _ => None,
            }));
            if let Some(image) = image {
                let result = Self::write_capture_png(&capture.path, &image);
                self.finish_capture(ctx, result);
            } else {
                ctx.request_repaint();
            }
            return;
        }
        let capture = self.capture.as_mut().unwrap();
        if capture.settle_frames < 2 {
            capture.settle_frames += 1;
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            capture.requested = true;
        }
        ctx.request_repaint();
    }

    fn write_capture_png(path: &Path, screenshot: &egui::ColorImage) -> Result<(), String> {
        let [width, height] = screenshot.size;
        let mut pixels = Vec::with_capacity(width.checked_mul(height)
            .and_then(|n| n.checked_mul(4)).ok_or("Screenshot dimensions overflow")?);
        for color in &screenshot.pixels { pixels.extend_from_slice(&color.to_array()); }
        let image = image::RgbaImage::from_raw(
            u32::try_from(width).map_err(|_| "Screenshot width exceeds PNG limits")?,
            u32::try_from(height).map_err(|_| "Screenshot height exceeds PNG limits")?, pixels)
            .ok_or("Screenshot pixel count does not match its dimensions")?;
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)
            .map_err(|e| format!("Create screenshot {}: {e}", path.display()))?;
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut file, image::ImageFormat::Png)
            .map_err(|e| format!("Encode screenshot: {e}"))?;
        file.sync_all().map_err(|e| format!("Sync screenshot: {e}"))?;
        Ok(())
    }

    fn start_package_import(&mut self, package_dir: &Path, project_path: &Path) -> Result<(), String> {
        if self.package_import_messages.is_some() {
            return Err("another capture package is still being imported".into());
        }
        let package_dir = package_dir.to_path_buf();
        let project_path = project_path.to_path_buf();
        let ffmpeg = PathBuf::from(self.ffmpeg_exe.trim());
        let ffprobe = PathBuf::from(self.ffprobe_exe.trim());
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new().name("capture-package-import".into()).spawn(move || {
            let result = crate::media_tools::with_selected_media_tools(&ffmpeg, &ffprobe, ||
                crate::capture_project::import_package_to_new_project_with_cancel(
                    &package_dir, &project_path,
                    || worker_cancel.load(Ordering::Relaxed)))
                .and_then(|result| result.map_err(|error| error.to_string()))
                .map(|imported| {
                    let shortened: Vec<_> = imported.coverage.iter()
                        .filter(|clip| clip.editor_duration_ms < clip.declared_duration_ms)
                        .map(|clip| format!("{}: {} ms editor / {} ms source",
                            clip.clip_id, clip.editor_duration_ms, clip.declared_duration_ms))
                        .collect();
                    let summary = if shortened.is_empty() {
                        "All visual clips fit their verified video and audio streams.".to_string()
                    } else {
                        format!("Editor clips stop at verified video/audio coverage; original source durations remain in capture provenance. Shortened: {}", shortened.join(", "))
                    };
                    (project_path, summary)
                });
            let _ = sender.send(result);
        }).map_err(|error| format!("could not start package import: {error}"))?;
        self.package_import_messages = Some(receiver);
        self.package_import_cancel = cancel;
        self.imported_project_path = None;
        self.package_import_summary = None;
        Ok(())
    }

    fn poll_package_import(&mut self) {
        let result = self.package_import_messages.as_ref().and_then(|receiver| match receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                "package import worker stopped before reporting a result".into())),
        });
        if let Some(result) = result {
            self.package_import_messages = None;
            match result {
                Ok((path, summary)) => {
                    self.imported_project_path = Some(path);
                    self.package_import_summary = Some(summary);
                    self.error = None;
                }
                Err(error) => {
                    self.package_import_summary = None;
                    self.error = Some(error);
                },
            }
        }
    }

    /// Retain the native render state for projects opened later in this window.
    pub fn with_native_context(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        let mut app = Self::default();
        app.editor_render_state = cc.wgpu_render_state.clone();
        Ok(app)
    }

    pub fn with_saved_project(cc: &eframe::CreationContext<'_>, path: &Path) -> Result<Self, String> {
        let mut app = Self::with_native_context(cc)?;
        app.open_saved_project(path)?;
        Ok(app)
    }

    fn open_saved_project(&mut self, path: &Path) -> Result<(), String> {
        if self.embedded_editor.is_some() { return Err("return from the current edit before opening another project".into()); }
        if self.pilot_messages.is_some() { return Err("finish or cancel the capture operation before opening the editor".into()); }
        let (session, legacy) = load_editor_session(path)?;
        self.install_editor(session, legacy, true)
    }

    fn install_editor(&mut self, mut session: Session, legacy: bool,
        recovery: bool) -> Result<(), String> {
        if recovery { if let Some(root) = std::env::var_os("LOCALAPPDATA") {
            session.start_autosave(AutosaveConfig::new(PathBuf::from(root).join("DemoStrike/FilmCraftEditor")))
                .map_err(|error| format!("could not start editor recovery: {error}"))?;
        }}
        let mut editor = FilmcraftApp::new(session);
        editor.host_brand = filmcraft_ui_egui::brand::HostBrand {
            product_name: "DemoStrike", wordmark: "DEMOSTRIKE",
        };
        editor.ui.mode = Mode::Edit;
        if legacy {
            editor.ui.status = "Capture project converted in memory. Save As a new .fcproj; the original .cstrike remains unchanged.".into();
        }
        if let Some(state) = self.editor_render_state.clone() { editor.set_wgpu(state); }
        editor.audio = Some(Box::new(filmcraft_audio::CpalOut::new()));
        editor.hooks.pick_files = Some(Box::new(|exts| rfd::FileDialog::new()
            .add_filter("Media", exts).pick_files().unwrap_or_default()
            .into_iter().map(|p| p.to_string_lossy().into_owned()).collect()));
        editor.hooks.pick_save = Some(Box::new(|name| rfd::FileDialog::new()
            .add_filter("FilmCraft project", &["fcproj"]).set_file_name(name).save_file()
            .map(|p| p.to_string_lossy().into_owned())));
        editor.hooks.pick_save_as = Some(Box::new(|label, extensions, name| rfd::FileDialog::new()
            .add_filter(label, extensions).set_file_name(name).save_file()
            .map(|p| p.to_string_lossy().into_owned())));
        editor.hooks.pick_open_project = Some(Box::new(|| rfd::FileDialog::new()
            .add_filter("FilmCraft project", &["fcproj"]).pick_file()
            .map(|p| p.to_string_lossy().into_owned())));
        editor.hooks.pick_folder = Some(Box::new(|| rfd::FileDialog::new().pick_folder()
            .map(|p| p.to_string_lossy().into_owned())));
        self.embedded_editor = Some(editor);
        self.confirm_discard_editor = false;
        self.error = None;
        Ok(())
    }

    fn open_raw_take_preview(&mut self, index: usize, video_hash: String,
        wav_hash: String) -> Result<(), String> {
        if self.embedded_editor.is_some() { return Err("return from the current editor first".into()); }
        let take = self.review_takes.get(index).ok_or("review role changed while preparing preview")?;
        let video = PathBuf::from(take.muxed_mp4.trim());
        let wav = PathBuf::from(take.hlae_wav.trim());
        let role = take.camera_role.clone();
        let mut session = Session::default();
        let imported = session.execute("file.import", json!({
            "paths": [video.to_string_lossy(), wav.to_string_lossy()]
        })).map_err(|e| format!("FilmCraft could not import raw take: {e}"))?;
        let items = imported.get("items").and_then(|v| v.as_array())
            .ok_or("FilmCraft did not return imported media")?;
        if items.len() != 2 || imported.get("errors").and_then(|v| v.as_array())
            .is_some_and(|errors| !errors.is_empty()) {
            return Err(format!("FilmCraft could not open both video and WAV: {imported}"));
        }
        let video_item = items[0].as_u64().ok_or("FilmCraft video item ID is unavailable")?;
        session.execute("source.open", json!({"item": video_item}))
            .map_err(|e| format!("FilmCraft source monitor could not open video: {e}"))?;
        // The temporary review session has no project file or recovery journal.
        // User edits still trigger the normal explicit-discard return guard.
        session.saved_revision = session.revision;
        self.install_editor(session, false, false)?;
        if let Some(editor) = self.embedded_editor.as_mut() {
            editor.ui.status = format!("Review {role}: scrub/play MP4 in Source. Open the imported WAV in Project to listen. Return, then explicitly record each decision. This is an unsaved review workspace.");
        }
        if let Some(take) = self.review_takes.get_mut(index) {
            take.preview_muxed_sha256 = video_hash;
            take.preview_wav_sha256 = wav_hash;
        }
        self.review_preview_role = Some(role);
        Ok(())
    }

    fn show_embedded_editor(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut return_requested = false;
        let mut discard_editor_changes = false;
        if let Some(editor) = self.embedded_editor.as_mut() {
            egui::Panel::top("demostrike-editor-return").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("DemoStrike");
                    if let Some(role) = &self.review_preview_role {
                        ui.label(format!("Raw take review / {role} — Source monitor playback and scrubbing"));
                    } else {
                        ui.label("Capture and analysis / FilmCraft editor");
                    }
                    return_requested = ui.button("Return to analysis").clicked();
                    if let Some(error) = &self.error { ui.colored_label(egui::Color32::RED, error); }
                });
            });
            eframe::App::ui(editor, ui, frame);
            if self.confirm_discard_editor && editor_return_state(editor, false) == EditorReturnGate::Busy {
                self.confirm_discard_editor = false;
                self.error = Some("Finish or cancel the active editor export before returning to analysis".into());
            }
            if return_requested {
                match editor_return_state(editor, false) {
                    EditorReturnGate::Busy => {
                        self.error = Some("Finish or cancel the active editor export before returning to analysis".into());
                        return_requested = false;
                    }
                    EditorReturnGate::ConfirmDiscard => {
                        self.confirm_discard_editor = true;
                        return_requested = false;
                    }
                    EditorReturnGate::Ready => self.confirm_discard_editor = false,
                }
            }
        }
        if self.confirm_discard_editor {
            egui::Window::new("Unsaved editor changes")
                .collapsible(false).resizable(false).show(&ctx, |ui| {
                    ui.label("Save in the editor before returning, or discard these unsaved edits.");
                    if ui.button("Keep editing").clicked() { self.confirm_discard_editor = false; }
                    if ui.button("Discard edits and return").clicked() {
                        self.confirm_discard_editor = false;
                        return_requested = true;
                        discard_editor_changes = true;
                    }
                });
        }
        if return_requested {
            // The confirmation window is nonmodal: export may have started since
            // it opened. Every route passes this final check before taking state.
            if let Some(editor) = self.embedded_editor.as_ref() {
                match editor_return_state(editor, discard_editor_changes) {
                    EditorReturnGate::Busy => {
                        self.confirm_discard_editor = false;
                        self.error = Some("Finish or cancel the active editor export before returning to analysis".into());
                        return;
                    }
                    EditorReturnGate::ConfirmDiscard => {
                        self.confirm_discard_editor = true;
                        return;
                    }
                    EditorReturnGate::Ready => {}
                }
            }
            if let Some(mut editor) = self.embedded_editor.take() {
                if discard_editor_changes {
                    // Explicit discard also clears this session's unsaved recovery journal.
                    editor.session.saved_revision = editor.session.revision;
                    editor.session.sync_persistence();
                }
                eframe::App::on_exit(&mut editor);
            }
            self.review_preview_role = None;
            self.error = None;
        }
    }

    fn pilot_tools(&self) -> CaptureTools {
        CaptureTools {
            cs2_exe: PathBuf::from(&self.cs2_exe),
            hlae_exe: PathBuf::from(&self.hlae_exe),
            ffmpeg_exe: PathBuf::from(&self.ffmpeg_exe),
            ffprobe_exe: PathBuf::from(&self.ffprobe_exe),
        }
    }

    fn inspect_receipt(&mut self) -> Result<(), String> {
        let path = PathBuf::from(self.reviewed_receipt.trim());
        if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 4 * 1024 * 1024 {
            return Err("capture receipt exceeds the 4 MiB UI inspection limit".into());
        }
        let receipt: crate::capture_receipt::CaptureReceipt =
            serde_json::from_slice(&std::fs::read(&path)
                .map_err(|e| format!("cannot open receipt {}: {e}", path.display()))?)
                .map_err(|e| format!("invalid reviewed receipt: {e}"))?;
        self.proof_frames.clear();
        self.proof_texture = None;
        let mut summaries = Vec::new();
        for take in &receipt.takes {
            let verdict = match &take.visual_review {
                Some(review) => {
                    for frame in &review.proof_frames {
                        self.proof_frames.push((format!("{} tick {}", take.camera_role, frame.tick),
                            frame.path.clone(), frame.sha256.clone()));
                    }
                    format!("review by {}; extent {}; {} proof frames; player={} image={} listened audio={} release={} flight={} stance={}",
                        review.reviewed_by, review.review_extent, review.proof_frames.len(),
                        review.correct_player, review.clean_image, review.audible_game_audio,
                        review.release_observed,
                        review.same_event_complete_flight, review.model_feet_stance_visible)
                }
                None => "no visual review".into(),
            };
            summaries.push(format!("{}: {} ms, {}x{} at {}/{} fps, {} Hz / {} channels; {verdict}\nVideo: {}\nHLAE WAV: {}",
                take.camera_role, take.duration_ms, take.width, take.height,
                take.fps_num, take.fps_den, take.audio_sample_rate, take.audio_channels,
                take.muxed_mp4.display(), take.hlae_wav.display()));
        }
        self.receipt_summary = Some(format!("Receipt event {} / {} takes. This display reports claims; sealing independently checks source hashes, live identity logs, full media decode, and proof bytes.\n{}",
            receipt.event_id, receipt.takes.len(), summaries.join("\n")));
        Ok(())
    }

    fn view_proof_frame(&mut self, ctx: &egui::Context, label: &str, path: &Path,
        expected_sha256: &str) -> Result<(), String> {
        if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 32 * 1024 * 1024 {
            return Err("proof frame exceeds the 32 MiB preview limit".into());
        }
        let bytes = std::fs::read(path).map_err(|e| format!("cannot read proof frame: {e}"))?;
        if format!("{:x}", Sha256::digest(&bytes)) != expected_sha256 {
            return Err(format!("proof frame changed since the reviewed receipt: {}", path.display()));
        }
        let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode()
            .map_err(|e| format!("cannot decode proof frame: {e}"))?.to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
        self.proof_texture = Some(ctx.load_texture("capture-proof-frame", color, Default::default()));
        self.proof_label = Some(format!("{} — {}", label, path.display()));
        Ok(())
    }

    fn stage_pilot(&mut self) -> Result<(), String> {
        if self.pilot_messages.is_some() { return Err("another capture operation is active".into()); }
        let request = PathBuf::from(self.capture_request_path.trim());
        let destination = PathBuf::from(self.pilot_destination.trim());
        if !request.is_file() || !destination.is_absolute() || destination.exists() {
            return Err("choose an existing request and a new absolute pilot folder".into());
        }
        let tools = self.pilot_tools();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new().name("event-pilot-stage".into()).spawn(move || {
            let result = if worker_cancel.load(Ordering::Relaxed) {
                Err("pilot staging canceled".into())
            } else {
                stage_event_pilot(&request, &tools, &destination).map(PilotOutcome::Staged)
            };
            let result = if worker_cancel.load(Ordering::Relaxed) {
                Err("pilot staging canceled; inspect the new folder before retrying".into())
            } else { result };
            let _ = sender.send(PilotMessage::Finished(result));
        }).map_err(|e| format!("cannot start pilot stage worker: {e}"))?;
        self.pilot_cancel = cancel;
        self.pilot_cancel_available = true;
        self.pilot_messages = Some(receiver);
        self.pilot_status = Some("Checking source, runtime versions, process conflicts, and staging three roles".into());
        Ok(())
    }

    fn run_pilot(&mut self) -> Result<(), String> {
        if self.pilot_messages.is_some() { return Err("another capture operation is active".into()); }
        let manifest = PathBuf::from(self.pilot_manifest.trim());
        if !manifest.is_file() { return Err("select a staged event-pilot-manifest.json".into()); }
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new().name("owned-event-pilot".into()).spawn(move || {
            let result = event_launch_spec(&manifest).and_then(|launch|
                run_owned(launch, &worker_cancel, Duration::from_secs(25 * 60),
                    |message| { let _ = sender.send(PilotMessage::Progress(message)); }))
                .map(|_| PilotOutcome::Ran);
            let _ = sender.send(PilotMessage::Finished(result));
        }).map_err(|e| format!("cannot start owned capture worker: {e}"))?;
        self.pilot_cancel = cancel;
        self.pilot_cancel_available = true;
        self.pilot_messages = Some(receiver);
        self.pilot_status = Some("Rechecking staged hashes and offline runtime before launch".into());
        Ok(())
    }

    fn load_review_draft(&mut self) -> Result<(), String> {
        if self.pilot_messages.is_some() || self.embedded_editor.is_some() {
            return Err("finish the current capture or editor operation first".into());
        }
        let request = PathBuf::from(self.capture_request_path.trim());
        self.review_takes = crate::capture_receipt::draft_takes_for_request(&request)?;
        self.receipt_summary = None;
        self.pilot_status = Some("Three unreviewed roles loaded. Select each exact MP4, HLAE WAV and runtime identity log; inspect motion and listen before checking decisions.".into());
        Ok(())
    }

    fn prepare_pilot_media_qc(&mut self) -> Result<(), String> {
        if self.pilot_messages.is_some() || self.embedded_editor.is_some() {
            return Err("finish the current capture or editor operation first".into());
        }
        let manifest = PathBuf::from(self.pilot_manifest.trim());
        let request = PathBuf::from(self.capture_request_path.trim());
        let review_name = self.pilot_qc_review_name.trim().to_string();
        let ffmpeg = PathBuf::from(self.ffmpeg_exe.trim());
        let ffprobe = PathBuf::from(self.ffprobe_exe.trim());
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new().name("owned-event-media-qc".into()).spawn(move || {
            let result = plan_qc(&manifest, &request, &review_name, &ffmpeg, &ffprobe)
                .and_then(|plan| run_qc_with_cancel(&plan, &worker_cancel,
                    |message| { let _ = sender.send(PilotMessage::Progress(message)); }))
                .map(PilotOutcome::QcPrepared);
            let _ = sender.send(PilotMessage::Finished(result));
        }).map_err(|e| format!("cannot start media QC worker: {e}"))?;
        self.pilot_cancel = cancel;
        self.pilot_cancel_available = true;
        self.pilot_messages = Some(receiver);
        self.pilot_status = Some("Checking raw media, output budget and free space before owned FFmpeg QC/mux".into());
        Ok(())
    }

    fn author_runtime_identity_logs(&mut self) -> Result<(), String> {
        if self.pilot_messages.is_some() { return Err("another capture operation is active".into()); }
        let manifest = PathBuf::from(self.pilot_manifest.trim());
        let destination = PathBuf::from(self.review_identity_destination.trim());
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new().name("recorded-identity-logs".into()).spawn(move || {
            let result = crate::capture_receipt::author_runtime_identity_logs_with_cancel(
                &manifest, &destination, || worker_cancel.load(Ordering::Relaxed))
                .map(PilotOutcome::IdentityLogs);
            let _ = sender.send(PilotMessage::Finished(result));
        }).map_err(|e| format!("cannot start identity-log preparation: {e}"))?;
        self.pilot_cancel = cancel;
        self.pilot_cancel_available = true;
        self.pilot_messages = Some(receiver);
        self.pilot_status = Some("Checking recorded console markers against all three planned camera roles".into());
        Ok(())
    }

    fn prepare_raw_take_preview(&mut self, index: usize) -> Result<(), String> {
        if self.pilot_messages.is_some() || self.embedded_editor.is_some() {
            return Err("finish the current capture or editor operation first".into());
        }
        let take = self.review_takes.get_mut(index).ok_or("select a review role")?;
        let video = PathBuf::from(take.muxed_mp4.trim());
        let wav = PathBuf::from(take.hlae_wav.trim());
        take.preview_muxed_sha256.clear();
        take.preview_wav_sha256.clear();
        // A new source preview invalidates prior human decisions. No checkbox is
        // set on behalf of the reviewer by media probes or by opening a player.
        take.correct_player = false;
        take.release_observed = false;
        take.expected_utility_visible_in_pov = false;
        take.same_event_complete_flight = false;
        take.model_feet_stance_visible = false;
        take.clean_image = false;
        take.audible_game_audio = false;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new().name("raw-take-preview-identity".into()).spawn(move || {
            let result = crate::capture_receipt::preview_identities_with_cancel(
                &video, &wav, || worker_cancel.load(Ordering::Relaxed))
                .map(|(video, wav)| PilotOutcome::PreviewPrepared(index, video, wav));
            let _ = sender.send(PilotMessage::Finished(result));
        }).map_err(|e| format!("cannot start review source verification: {e}"))?;
        self.pilot_cancel = cancel;
        self.pilot_cancel_available = true;
        self.pilot_messages = Some(receiver);
        self.pilot_status = Some("Hashing exact MP4 and WAV before opening FilmCraft source playback".into());
        Ok(())
    }

    fn author_review_receipt(&mut self) -> Result<(), String> {
        if self.pilot_messages.is_some() || self.embedded_editor.is_some() {
            return Err("finish the current capture or editor operation first".into());
        }
        let request = PathBuf::from(self.capture_request_path.trim());
        let destination = PathBuf::from(self.reviewed_receipt.trim());
        let takes = self.review_takes.clone();
        let ffmpeg = PathBuf::from(self.ffmpeg_exe.trim());
        let ffprobe = PathBuf::from(self.ffprobe_exe.trim());
        if takes.is_empty() { return Err("load the capture request and review all roles first".into()); }
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new().name("author-capture-review".into()).spawn(move || {
            let result = crate::media_tools::with_selected_media_tools(&ffmpeg, &ffprobe, ||
                crate::capture_receipt::author_reviewed_receipt_with_cancel(
                    &request, &takes, &destination, || worker_cancel.load(Ordering::Relaxed)))
                .and_then(|result| result)
                .map(|_| PilotOutcome::Reviewed(destination));
            let _ = sender.send(PilotMessage::Finished(result));
        }).map_err(|e| format!("cannot start receipt authoring: {e}"))?;
        self.pilot_cancel = cancel;
        self.pilot_cancel_available = true;
        self.pilot_messages = Some(receiver);
        self.pilot_status = Some("Checking live identity, exact preview hashes, full media decode and explicit decisions before creating a new receipt".into());
        Ok(())
    }

    fn seal_reviewed_pilot(&mut self) -> Result<(), String> {
        if self.pilot_messages.is_some() { return Err("another capture operation is active".into()); }
        let request = PathBuf::from(self.capture_request_path.trim());
        let receipt = PathBuf::from(self.reviewed_receipt.trim());
        let destination = PathBuf::from(self.reviewed_package_destination.trim());
        let ffmpeg = PathBuf::from(self.ffmpeg_exe.trim());
        let ffprobe = PathBuf::from(self.ffprobe_exe.trim());
        if !request.is_file() || !receipt.is_file() || !destination.is_absolute() || destination.exists() {
            return Err("select an existing request and reviewed receipt, plus a new absolute package folder".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new().name("reviewed-capture-seal".into()).spawn(move || {
            let result = crate::media_tools::with_selected_media_tools(&ffmpeg, &ffprobe, ||
                crate::capture_receipt::seal_reviewed_capture_with_cancel(
                    &request, &receipt, &destination,
                    || worker_cancel.load(Ordering::Relaxed)))
                .and_then(|result| result)
                .map(|package| PilotOutcome::Sealed(package.root));
            let _ = sender.send(PilotMessage::Finished(result));
        }).map_err(|e| format!("cannot start reviewed capture validation: {e}"))?;
        self.pilot_cancel = cancel;
        self.pilot_messages = Some(receiver);
        self.pilot_cancel_available = true;
        self.pilot_status = Some("Validating live identity, proof frames, complete video/audio decode, and package streams (20-minute limit)".into());
        Ok(())
    }

    fn poll_pilot(&mut self) {
        if self.pilot_messages.is_none() { return; }
        loop {
            let message = self.pilot_messages.as_ref().unwrap().try_recv();
            match message {
                Ok(PilotMessage::Progress(message)) => self.pilot_status = Some(message),
                Ok(PilotMessage::Finished(result)) => {
                    self.pilot_messages = None;
                    self.pilot_cancel_available = false;
                    match result {
                        Ok(PilotOutcome::Staged(path)) => {
                            self.pilot_manifest = path.display().to_string();
                            self.review_identity_destination = path.parent().unwrap_or(Path::new("."))
                                .join("identity-logs").display().to_string();
                            self.reviewed_receipt = path.parent().unwrap_or(Path::new("."))
                                .join("reviewed-capture-receipt.json").display().to_string();
                            self.pilot_status = Some(format!("Three-role pilot staged: {}. Launch is a separate action.", path.display()));
                        }
                        Ok(PilotOutcome::Ran) => self.pilot_status = Some(
                            "Capture processes exited. Media and POV are unreviewed; inspect all three roles before sealing.".into()),
                        Ok(PilotOutcome::QcPrepared(result)) => {
                            if self.review_takes.is_empty() {
                                if let Ok(takes) = crate::capture_receipt::draft_takes_for_request(
                                    Path::new(self.capture_request_path.trim())) {
                                    self.review_takes = takes;
                                }
                            }
                            for prepared in &result.takes {
                                if let Some(take) = self.review_takes.iter_mut().find(|take| take.camera_role == prepared.role) {
                                    take.muxed_mp4 = prepared.muxed_mp4.display().to_string();
                                    take.hlae_wav = prepared.hlae_wav.display().to_string();
                                    take.preview_muxed_sha256.clear();
                                    take.preview_wav_sha256.clear();
                                    take.correct_player = false;
                                    take.release_observed = false;
                                    take.expected_utility_visible_in_pov = false;
                                    take.same_event_complete_flight = false;
                                    take.model_feet_stance_visible = false;
                                    take.clean_image = false;
                                    take.audible_game_audio = false;
                                }
                            }
                            self.pilot_status = Some(format!("Technical QC/mux outputs created in {}. No take has been human-reviewed; open each source and listen before authoring a receipt.", result.review_root.display()));
                        }
                        Ok(PilotOutcome::IdentityLogs(paths)) => {
                            for (role, path) in &paths {
                                if let Some(take) = self.review_takes.iter_mut().find(|take| &take.camera_role == role) {
                                    take.runtime_identity_log = path.display().to_string();
                                }
                            }
                            self.pilot_status = Some(format!("Created {} logs from recorded console markers. These prove logged identity only; visually confirm POV in each take.", paths.len()));
                        }
                        Ok(PilotOutcome::PreviewPrepared(index, video, wav)) => {
                            if let Err(error) = self.open_raw_take_preview(index, video, wav) {
                                self.pilot_status = Some(format!("Raw take preview failed: {error}"));
                            } else {
                                self.pilot_status = Some("Raw take opened in FilmCraft Source; scrub motion and listen to the imported WAV before recording decisions.".into());
                            }
                        }
                        Ok(PilotOutcome::Reviewed(path)) => {
                            self.reviewed_receipt = path.display().to_string();
                            self.receipt_summary = None;
                            self.pilot_status = Some(format!("New hash-bound review receipt created: {}. Inspect it, then seal as a separate action.", path.display()));
                        }
                        Ok(PilotOutcome::Sealed(path)) => self.pilot_status = Some(format!(
                            "Reviewed capture package sealed: {}. Import this package to make an editable project.", path.display())),
                        Err(error) => self.pilot_status = Some(format!("Capture operation stopped: {error}")),
                    }
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pilot_messages = None;
                    self.pilot_cancel_available = false;
                    self.pilot_status = Some("Capture worker stopped without a result; inspect staged artifacts.".into());
                    break;
                }
            }
        }
    }

    fn start_capture_request(&mut self, event_id: i64, destination: &Path) -> Result<(), String> {
        if self.capture_request_messages.is_some() {
            return Err("another capture request is still being prepared".into());
        }
        let database = PathBuf::from(&self.database_path);
        let destination = destination.to_path_buf();
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new().name("capture-request".into()).spawn(move || {
            let result = build_capture_request(&database, event_id)
                .and_then(|request| save_request_noclobber(&request, &destination).map(|_| destination));
            let _ = sender.send(result);
        }).map_err(|error| format!("could not start capture request worker: {error}"))?;
        self.capture_request_messages = Some(receiver);
        self.capture_request_status = Some("Verifying source demo and event".into());
        self.error = None;
        Ok(())
    }

    fn poll_capture_request(&mut self) {
        let result = self.capture_request_messages.as_ref().and_then(|receiver| match receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                "capture request worker stopped before reporting a result".into())),
        });
        if let Some(result) = result {
            self.capture_request_messages = None;
            match result {
                Ok(path) => {
                    self.capture_request_path = path.display().to_string();
                    self.capture_request_status = Some(format!(
                        "Capture request saved: {}. No recording has started; POV remains unverified.", path.display()));
                }
                Err(error) => {
                    self.capture_request_status = Some("Capture request failed".into());
                    self.error = Some(error);
                }
            }
        }
    }

    fn start_demo_import(&mut self, path: &Path) -> Result<(), String> {
        self.start_demo_import_with_parser(path, Arc::new(NativeGrenadeParser))
    }

    fn start_reference_demo_import(&mut self, path: &Path) -> Result<(), String> {
        let parser = ExternalExtractor {
            node: PathBuf::from(&self.node_path),
            script: PathBuf::from(&self.extractor_path),
        };
        parser.check_available().map_err(|error| {
            self.error = Some(error.clone()); error
        })?;
        self.start_demo_import_with_parser(path, Arc::new(parser))
    }

    fn start_demo_import_with_parser(
        &mut self,
        path: &Path,
        parser: Arc<dyn DemoFactsParser>,
    ) -> Result<(), String> {
        if self.import_messages.is_some() {
            return Err("another demo import is still running".into());
        }
        validate_demo_file(path).map_err(|error| {
            self.error = Some(error.clone()); error
        })?;
        let path = path.to_path_buf();
        let worker_path = path.clone();
        let database = PathBuf::from(&self.database_path);
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(ImportControl::new());
        let worker_cancelled = Arc::clone(&cancelled);
        std::thread::Builder::new().name("demo-import".into()).spawn(move || {
            let result = import_demo(&worker_path, &database, parser.as_ref(), &worker_cancelled,
                |stage| { let _ = sender.send(ImportMessage::Stage(stage)); });
            let _ = sender.send(ImportMessage::Finished(result));
        }).map_err(|error| {
            let message = format!("could not start demo import: {error}");
            self.error = Some(message.clone()); message
        })?;
        self.import_source = Some(path);
        self.import_stage = Some("Queued".into());
        self.import_messages = Some(receiver);
        self.import_cancel = cancelled;
        self.error = None;
        Ok(())
    }

    fn poll_demo_import(&mut self) {
        let mut finished = false;
        let mut new_match_id = None;
        if let Some(receiver) = &self.import_messages {
            loop {
                let message = match receiver.try_recv() {
                    Ok(message) => message,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        if !finished {
                            self.error = Some("demo import worker stopped before reporting a result".into());
                            self.import_stage = Some("Failed; previous analysis retained".into());
                            finished = true;
                        }
                        break;
                    }
                };
                match message {
                    ImportMessage::Stage(stage) => self.import_stage = Some(stage.into()),
                    ImportMessage::Finished(result) => {
                        finished = true;
                        match result {
                            Ok(imported) => {
                                new_match_id = Some(imported.match_id);
                                self.capabilities = Some(imported.capabilities);
                                self.analyzed = Some(imported.analyzed);
                                self.source_label = self.import_source.as_ref()
                                    .map(|path| format!("{} (match {})", path.display(), imported.match_id));
                                self.error = None;
                                self.import_stage = Some("Complete".into());
                            }
                            Err(error) => {
                                self.error = Some(error);
                                self.import_stage = Some("Failed or cancelled; previous analysis retained".into());
                            }
                        }
                    }
                }
            }
        }
        if finished { self.import_messages = None; }
        if let Some(id) = new_match_id {
            self.library_match_id = Some(id);
            if let Err(error) = self.refresh_library() { self.error = Some(error); }
        }
    }

    fn refresh_library(&mut self) -> Result<(), String> {
        self.library_opened = true;
        let connection = db::open(Path::new(&self.database_path)).map_err(|e| e.to_string())?;
        self.library_matches = queries::list_matches(&connection).map_err(|e| e.to_string())?;
        if !self.library_matches.iter().any(|m| Some(m.id) == self.library_match_id) {
            self.library_match_id = self.library_matches.first().map(|m| m.id);
        }
        self.refresh_library_events()
    }

    fn refresh_library_events(&mut self) -> Result<(), String> {
        self.library_events.clear();
        self.selected_event = None;
        let Some(id) = self.library_match_id else { return Ok(()); };
        let connection = db::open(Path::new(&self.database_path)).map_err(|e| e.to_string())?;
        self.library_events = queries::query_events(&connection, &EventFilter {
            match_ids: Some(vec![id]), search: Some(self.library_search_applied.clone()),
            favorites_only: Some(self.library_favorites_only), limit: Some(200),
            offset: Some(self.library_page * 200), ..Default::default()
        }).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn select_saved_event(&mut self, id: i64) -> Result<(), String> {
        let connection = db::open(Path::new(&self.database_path)).map_err(|e| e.to_string())?;
        self.selected_event = Some(queries::get_event_detail(&connection, id).map_err(|e| e.to_string())?);
        Ok(())
    }

    fn update_saved_event(&mut self, id: i64, favorite: Option<bool>, tag: Option<&str>, remove_tag: Option<&str>) -> Result<(), String> {
        let connection = db::open(Path::new(&self.database_path)).map_err(|e| e.to_string())?;
        if let Some(value) = favorite { queries::set_favorite(&connection, id, value).map_err(|e| e.to_string())?; }
        if let Some(name) = tag { queries::tag_event(&connection, id, name).map_err(|e| e.to_string())?; }
        if let Some(name) = remove_tag {
            let tag = queries::list_tags(&connection).map_err(|e| e.to_string())?
                .into_iter().find(|t| t.name == name).ok_or("tag no longer exists")?;
            queries::untag_event(&connection, id, tag.id).map_err(|e| e.to_string())?;
        }
        self.refresh_library_events()?;
        self.select_saved_event(id)
    }

    fn show_library(&mut self, ui: &mut egui::Ui) {
        if !self.library_opened { return; }
        let mut choose_match = None;
        let mut choose_event = None;
        let mut refresh_events = false;
        let mut favorite_change = None;
        let mut add_tag = None;
        let mut remove_tag = None;
        let mut prepare_capture = None;
        ui.group(|ui| {
            ui.heading("Saved matches");
            if self.library_matches.is_empty() { ui.label("No imported matches in this database."); return; }
            egui::ComboBox::from_id_salt("saved_match").selected_text(
                self.library_matches.iter().find(|m| Some(m.id) == self.library_match_id)
                    .map(|m| format!("{} · {} · {} events", m.file_name, m.map_name, m.event_count))
                    .unwrap_or_else(|| "Select match".into()))
                .show_ui(ui, |ui| {
                    for m in &self.library_matches {
                        if ui.selectable_label(Some(m.id) == self.library_match_id,
                            format!("{} · {} · {} events", m.file_name, m.map_name, m.event_count)).clicked() {
                            choose_match = Some(m.id);
                        }
                    }
                });
            if let Some(m) = self.library_matches.iter().find(|m| Some(m.id) == self.library_match_id) {
                if m.effect_attribution_version == Some(2) {
                    ui.small(format!("Effect facts: {}; ambiguous: {}; unresolved: {}",
                        m.effect_fact_count, m.ambiguous_effect_count, m.unresolved_effect_count));
                } else if m.parser_capability.supports_combat() {
                    ui.colored_label(egui::Color32::YELLOW, "Older match: per-grenade effect totals are unverified and hidden.");
                }
                ui.small(format!("{} · {} · {}", m.file_path, m.imported_at, m.parser_capability.key()));
            }
            ui.horizontal(|ui| {
                ui.label("Find player, map, utility, or tag");
                ui.text_edit_singleline(&mut self.library_search);
                if ui.button("Search").clicked() { self.library_search_applied = self.library_search.clone(); self.library_page = 0; refresh_events = true; }
                if ui.checkbox(&mut self.library_favorites_only, "Favorites").changed() { self.library_page = 0; refresh_events = true; }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(self.library_page > 0, egui::Button::new("Previous")).clicked() {
                    self.library_page -= 1; refresh_events = true;
                }
                ui.label(format!("Page {} · {} shown", self.library_page + 1, self.library_events.len()));
                if ui.add_enabled(self.library_events.len() == 200, egui::Button::new("Next")).clicked() {
                    self.library_page += 1; refresh_events = true;
                }
            });
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                for event in &self.library_events {
                    let label = format!("{} Round {} · {} {} · tick {}{}",
                        if event.favorite { "★" } else { "☆" }, event.round_number,
                        event.thrower_name, event.utility_type.as_str(), event.throw_tick,
                        if event.tags.is_empty() { String::new() } else { format!(" · {}", event.tags.join(", ")) });
                    if ui.selectable_label(self.selected_event.as_ref().is_some_and(|d| d.summary.id == event.id), label).clicked() {
                        choose_event = Some(event.id);
                    }
                }
            });
            if let Some(detail) = &self.selected_event {
                let e = &detail.summary;
                ui.separator();
                ui.label(format!("{} · {} · round {} · tick {} · SteamID {}",
                    e.thrower_name, e.utility_type.as_str(), e.round_number, e.throw_tick, e.thrower_steamid));
                ui.label(format!("Throw ({:.1}, {:.1}, {:.1}); trajectory {} points; {} nearby players",
                    e.throw_pos.x, e.throw_pos.y, e.throw_pos.z, detail.trajectory.len(), detail.nearby_players.len()));
                if e.parser_capability == ParserCapabilities::GrenadeAnalysisOnlyV1 {
                    ui.small("Combat, flash effects, kills, assists, and economy unavailable. POV slot has not been capture-verified.");
                } else if matches!(e.parser_capability,
                    ParserCapabilities::NativeCombatFlashV1 | ParserCapabilities::NativeCombatFullV2) {
                    ui.small("Native grenade, combat, and flash events. Economy unavailable; POV slot has not been capture-verified.");
                    if e.effect_attribution_status != "complete" {
                        ui.colored_label(egui::Color32::YELLOW, format!("Effect attribution: {}. Raw facts are preserved; unavailable totals are not zero.", e.effect_attribution_status));
                    }
                    ui.label(format!("Enemy damage: {}; enemy flash duration: {}; kills: {}; flash assists: {}",
                        e.damage_enemy.map_or("unavailable".into(), |n| n.to_string()),
                        e.blind_enemy_duration.map_or("unavailable".into(), |n| format!("{n:.1}s")),
                        e.kills.map_or("unavailable".into(), |n| n.to_string()),
                        detail.flash_assists.map_or("unavailable".into(), |n| n.to_string())));
                }
                ui.horizontal(|ui| {
                    if ui.button(if e.favorite { "Remove favorite" } else { "Favorite" }).clicked() {
                        favorite_change = Some((e.id, !e.favorite));
                    }
                    ui.text_edit_singleline(&mut self.tag_draft);
                    if ui.button("Add tag").clicked() { add_tag = Some(e.id); }
                });
                ui.horizontal_wrapped(|ui| {
                    for tag in &e.tags {
                        if ui.small_button(format!("Remove {tag}")).clicked() { remove_tag = Some((e.id, tag.clone())); }
                    }
                });
                if ui.add_enabled(self.capture_request_messages.is_none(),
                    egui::Button::new("Prepare source-bound capture request")).clicked() {
                    prepare_capture = Some(e.id);
                }
            }
        });
        if let Some(id) = choose_match { self.library_match_id = Some(id); self.library_page = 0; refresh_events = true; }
        if refresh_events { if let Err(error) = self.refresh_library_events() { self.error = Some(error); } }
        if let Some(id) = choose_event { if let Err(error) = self.select_saved_event(id) { self.error = Some(error); } }
        if let Some(id) = prepare_capture {
            if let Some(path) = rfd::FileDialog::new().add_filter("DemoStrike capture request", &["json"])
                .set_file_name(format!("demostrike-event-{id}-capture-request.json")).save_file() {
                if let Err(error) = self.start_capture_request(id, &path) { self.error = Some(error); }
            }
        }
        if let Some((id, value)) = favorite_change {
            if let Err(error) = self.update_saved_event(id, Some(value), None, None) { self.error = Some(error); }
        }
        if let Some(id) = add_tag {
            let tag = self.tag_draft.clone();
            match self.update_saved_event(id, None, Some(&tag), None) {
                Ok(()) => self.tag_draft.clear(), Err(error) => self.error = Some(error),
            }
        }
        if let Some((id, tag)) = remove_tag {
            if let Err(error) = self.update_saved_event(id, None, None, Some(&tag)) { self.error = Some(error); }
        }
    }

    pub fn load_raw_json(&mut self, path: &Path) -> Result<(), String> {
        let result = std::fs::read(path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| analyze_raw_json(&bytes).map_err(|error| error.to_string()));
        match result {
            Ok(analyzed) => {
                self.analyzed = Some(analyzed);
                self.capabilities = Some(ParserCapabilities::ReferenceRawFacts);
                self.source_label = Some(path.display().to_string());
                self.error = None;
                Ok(())
            }
            Err(error) => {
                self.error = Some(error.clone());
                Err(error)
            }
        }
    }

    pub fn load_capture_draft(&mut self, path: &Path) -> Result<(), String> {
        match load_draft(path) {
            Ok(draft) => {
                let folder_name = if draft
                    .package_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
                {
                    format!("{}-package", draft.package_id)
                } else {
                    "new-capture-package".into()
                };
                self.package_destination = path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join(folder_name)
                    .display()
                    .to_string();
                self.draft_label = Some(path.display().to_string());
                self.draft = Some(draft);
                self.package_result = None;
                self.error = None;
                Ok(())
            }
            Err(error) => {
                self.error = Some(error.to_string());
                Err(error.to_string())
            }
        }
    }

    pub fn build_capture_package(&mut self) -> Result<(), String> {
        let draft = self
            .draft
            .as_ref()
            .ok_or_else(|| "open a capture draft first".to_string())?;
        let ffmpeg = PathBuf::from(self.ffmpeg_exe.trim());
        let ffprobe = PathBuf::from(self.ffprobe_exe.trim());
        match crate::media_tools::with_selected_media_tools(&ffmpeg, &ffprobe, ||
            finalize_stream_probed_capture_package(
                Path::new(&self.package_destination), &draft.package_id, &draft.inputs))
            .and_then(|result| result.map_err(|error| error.to_string())) {
            Ok(package) => {
                self.package_result = Some(format!(
                    "{}: {} stream-probed clips. Listen to and review the capture before use.",
                    package.root.display(),
                    package.manifest.clips.len(),
                ));
                self.error = None;
                Ok(())
            }
            Err(error) => {
                self.error = Some(error.to_string());
                Err(error.to_string())
            }
        }
    }
}

impl eframe::App for DemoStrikeApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if let Some(editor) = self.embedded_editor.as_mut() {
            eframe::App::raw_input_hook(editor, ctx, input);
        }
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Some(editor) = self.embedded_editor.as_mut() {
            eframe::App::logic(editor, ctx, frame);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.capture_after_frame(&ctx);
        if self.embedded_editor.is_some() {
            self.show_embedded_editor(ui, frame);
            return;
        }
        self.poll_demo_import();
        self.poll_capture_request();
        self.poll_pilot();
        self.poll_package_import();
        ctx.set_visuals(egui::Visuals::dark());
        egui::Panel::top("header").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("DemoStrike");
                ui.label("Utility event analysis");
                if ui.add_enabled(self.import_messages.is_none(), egui::Button::new("Import CS2 demo (Rust analysis)" )).clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("CS2 demo", &["dem"]).pick_file() {
                        let _ = self.start_demo_import(&path);
                    }
                }
                if ui.add_enabled(self.import_messages.is_none(), egui::Button::new("Import with reference extractor")).clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("CS2 demo", &["dem"]).pick_file() {
                        let _ = self.start_reference_demo_import(&path);
                    }
                }
                if ui.button("Open saved matches").clicked() {
                    if let Err(error) = self.refresh_library() { self.error = Some(error); }
                }
                if ui.button("Open editable project").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Capture or FilmCraft project", &["cstrike", "fcproj"]).pick_file() {
                        if let Err(error) = self.open_saved_project(&path) { self.error = Some(error); }
                    }
                }
                if ui.button("Open capture draft").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Capture draft", &["json"])
                        .pick_file()
                    {
                        let _ = self.load_capture_draft(&path);
                    }
                }
                if ui.add_enabled(self.package_import_messages.is_none(),
                    egui::Button::new("Import accepted capture package")).clicked() {
                    if let Some(package_dir) = rfd::FileDialog::new().pick_folder() {
                        if let Some(project_path) = rfd::FileDialog::new()
                            .add_filter("DemoStrike capture project", &["cstrike"])
                            .set_file_name("captured-event.cstrike").save_file() {
                            if let Err(error) = self.start_package_import(&package_dir, &project_path) {
                                self.error = Some(error);
                            }
                        }
                    }
                }
                if ui.button("Open raw demo facts…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Raw demo facts", &["json"])
                        .pick_file()
                    {
                        let _ = self.load_raw_json(&path);
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.collapsing("Demo parser configuration", |ui| {
                ui.small("Rust import uses source2-demo 0.5.9 and validates grenade, combat, and flash event schemas before saving. Unsupported schemas fail with an error. Economy and capture facts remain unavailable. Optional reference extractor settings follow.");
                ui.horizontal(|ui| {
                    ui.label("Node runtime");
                    ui.text_edit_singleline(&mut self.node_path);
                    if ui.button("Browse").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() { self.node_path = path.display().to_string(); }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("DemoStrike reference extractor script");
                    ui.text_edit_singleline(&mut self.extractor_path);
                    if ui.button("Browse").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("JavaScript module", &["mjs"]).pick_file() {
                            self.extractor_path = path.display().to_string();
                        }
                    }
                });
                ui.label(format!("Match database: {}", self.database_path));
            });
            if let Some(stage) = &self.import_stage { ui.label(format!("Demo import: {stage}")); }
            if let Some(status) = &self.capture_request_status { ui.label(status); }
            if self.capture_request_messages.is_some() { ctx.request_repaint_after(Duration::from_millis(100)); }
            ui.collapsing("Offline three-camera capture", |ui| {
                ui.small("A saved source-bound request starts this flow. Staging checks the demo and historical task-owned runtime; running the pilot is a separate action. Capture is local and offline.");
                ui.horizontal(|ui| {
                    ui.label("Request");
                    ui.text_edit_singleline(&mut self.capture_request_path);
                    if ui.button("Browse").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("Capture request", &["json"]).pick_file() {
                            self.capture_request_path = path.display().to_string();
                        }
                    }
                });
                ui.collapsing("Capture executables", |ui| {
                    for (name, value) in [
                        ("CS2", &mut self.cs2_exe), ("HLAE", &mut self.hlae_exe),
                        ("FFmpeg", &mut self.ffmpeg_exe), ("FFprobe", &mut self.ffprobe_exe),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(name);
                            ui.text_edit_singleline(value);
                        });
                    }
                    ui.small("CS2 and the new pilot folder must live under this DemoStrike source root. The version and provenance checks fail closed.");
                });
                ui.horizontal(|ui| {
                    ui.label("New pilot folder");
                    ui.text_edit_singleline(&mut self.pilot_destination);
                    if ui.button("Choose parent").clicked() {
                        if let Some(parent) = rfd::FileDialog::new().pick_folder() {
                            let name = format!("event-pilot-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
                            self.pilot_destination = parent.join(name).display().to_string();
                        }
                    }
                });
                if ui.add_enabled(self.pilot_messages.is_none(), egui::Button::new("Stage three-camera pilot")).clicked() {
                    if let Err(error) = self.stage_pilot() { self.pilot_status = Some(error); }
                }
                ui.horizontal(|ui| {
                    ui.label("Staged manifest");
                    ui.text_edit_singleline(&mut self.pilot_manifest);
                    if ui.button("Browse").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("Event pilot manifest", &["json"]).pick_file() {
                            self.pilot_manifest = path.display().to_string();
                        }
                    }
                });
                if ui.add_enabled(self.pilot_messages.is_none(), egui::Button::new("Run offline pilot (up to 25 min)")).clicked() {
                    if let Err(error) = self.run_pilot() { self.pilot_status = Some(error); }
                }
                ui.horizontal(|ui| {
                    ui.label("New QC folder name under pilot");
                    ui.text_edit_singleline(&mut self.pilot_qc_review_name);
                });
                if ui.add_enabled(self.pilot_messages.is_none(), egui::Button::new("Prepare muxed takes and technical QC (up to 25 min)")).clicked() {
                    if let Err(error) = self.prepare_pilot_media_qc() { self.pilot_status = Some(error); }
                }
                ui.small("Uses the existing qc_event_pilot.ps1 checks inside a kill-on-close Windows job. A new review folder is required. Technical decode, signal and stream checks do not approve image, POV, motion or listened audio.");
                if self.pilot_messages.is_some() {
                    if ui.add_enabled(self.pilot_cancel_available && !self.pilot_cancel.load(Ordering::Relaxed),
                        egui::Button::new("Cancel capture operation")).clicked() {
                        self.pilot_cancel.store(true, Ordering::Relaxed);
                    }
                    ctx.request_repaint_after(Duration::from_millis(100));
                    ui.small("Cancellation stops the owned game or FFmpeg job, hashing, or the receipt validator. Partial QC output remains unreviewed; choose a new review-* folder for a retry.");
                }
                if let Some(status) = &self.pilot_status { ui.label(status); }
                ui.separator();
                ui.small("Review each raw take in FilmCraft Source: scrub motion, inspect player and utility, and play the imported HLAE WAV to listen. Opening media or decoding it never checks a quality decision.");
                if ui.add_enabled(self.pilot_messages.is_none(), egui::Button::new("Load three-role review draft from request")).clicked() {
                    if let Err(error) = self.load_review_draft() { self.pilot_status = Some(error); }
                }
                ui.horizontal(|ui| {
                    ui.label("New identity-log folder");
                    ui.text_edit_singleline(&mut self.review_identity_destination);
                    if ui.button("Choose parent").clicked() {
                        if let Some(parent) = rfd::FileDialog::new().pick_folder() {
                            self.review_identity_destination = parent.join(format!("identity-logs-{}",
                                chrono::Utc::now().format("%Y%m%d-%H%M%S"))).display().to_string();
                        }
                    }
                });
                if ui.add_enabled(self.pilot_messages.is_none(), egui::Button::new("Build identity logs from recorded pilot console")).clicked() {
                    if let Err(error) = self.author_runtime_identity_logs() { self.pilot_status = Some(error); }
                }
                let mut preview_index = None;
                let mut draft_proof_preview: Option<(String, PathBuf)> = None;
                for (index, take) in self.review_takes.iter_mut().enumerate() {
                    ui.collapsing(format!("Review {}", take.camera_role), |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Muxed MP4");
                            if ui.text_edit_singleline(&mut take.muxed_mp4).changed() {
                                take.preview_muxed_sha256.clear(); take.preview_wav_sha256.clear();
                            }
                            if ui.button("Browse video").clicked() {
                                if let Some(path) = rfd::FileDialog::new().add_filter("Muxed video", &["mp4"]).pick_file() {
                                    take.muxed_mp4 = path.display().to_string();
                                    take.preview_muxed_sha256.clear(); take.preview_wav_sha256.clear();
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("HLAE WAV");
                            if ui.text_edit_singleline(&mut take.hlae_wav).changed() {
                                take.preview_muxed_sha256.clear(); take.preview_wav_sha256.clear();
                            }
                            if ui.button("Browse audio").clicked() {
                                if let Some(path) = rfd::FileDialog::new().add_filter("HLAE audio", &["wav"]).pick_file() {
                                    take.hlae_wav = path.display().to_string();
                                    take.preview_muxed_sha256.clear(); take.preview_wav_sha256.clear();
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Runtime identity JSON");
                            ui.text_edit_singleline(&mut take.runtime_identity_log);
                            if ui.button("Browse log").clicked() {
                                if let Some(path) = rfd::FileDialog::new().add_filter("Identity log", &["json"]).pick_file() {
                                    take.runtime_identity_log = path.display().to_string();
                                }
                            }
                        });
                        if ui.add_enabled(self.pilot_messages.is_none(), egui::Button::new("Open MP4 + WAV in FilmCraft Source")).clicked() {
                            preview_index = Some(index);
                        }
                        if !take.preview_muxed_sha256.is_empty() {
                            ui.small(format!("Source opened from video SHA-256 {}… and WAV SHA-256 {}…. Files are rehashed when authoring and sealing.",
                                &take.preview_muxed_sha256[..12], &take.preview_wav_sha256[..12]));
                        }
                        ui.separator();
                        ui.label("Explicit reviewer decisions (leave unchecked if uncertain)");
                        ui.horizontal(|ui| { ui.label("Reviewed by"); ui.text_edit_singleline(&mut take.reviewed_by); });
                        ui.label("Review notes: identify the observed player, motion and audible content");
                        ui.text_edit_multiline(&mut take.notes);
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut take.review_extent, "continuous".into(), "Continuous full take");
                            ui.selectable_value(&mut take.review_extent, "spot-check".into(), "Spot check only");
                        });
                        ui.checkbox(&mut take.correct_player, "Correct player visibly confirmed");
                        ui.checkbox(&mut take.clean_image, "Clean image and readable motion confirmed");
                        ui.checkbox(&mut take.audible_game_audio, "I listened and heard the expected game audio");
                        if take.camera_role == "player_pov" {
                            ui.checkbox(&mut take.release_observed, "Grenade release observed");
                            ui.checkbox(&mut take.expected_utility_visible_in_pov, "Expected utility visible in POV");
                        } else if take.camera_role == "projectile" {
                            ui.checkbox(&mut take.same_event_complete_flight, "Same event's complete flight observed");
                        } else {
                            ui.checkbox(&mut take.model_feet_stance_visible, "Model, feet and stance visible");
                        }
                        if ui.button("Add proof PNG").clicked() {
                            if let Some(path) = rfd::FileDialog::new().add_filter("Proof PNG", &["png"]).pick_file() {
                                take.proof_frames.push(ProofFrameDraft { path: path.display().to_string(), tick: String::new() });
                            }
                        }
                        let mut remove_proof = None;
                        let proof_role = take.camera_role.clone();
                        for (frame_index, frame) in take.proof_frames.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("Proof {} tick", frame_index + 1));
                                ui.text_edit_singleline(&mut frame.tick);
                                ui.label(&frame.path);
                                if ui.button("Preview PNG").clicked() {
                                    draft_proof_preview = Some((format!("{} draft proof tick {}",
                                        proof_role, frame.tick), PathBuf::from(&frame.path)));
                                }
                                if ui.button("Remove").clicked() { remove_proof = Some(frame_index); }
                            });
                        }
                        if let Some(frame_index) = remove_proof { take.proof_frames.remove(frame_index); }
                    });
                }
                if let Some(index) = preview_index {
                    if let Err(error) = self.prepare_raw_take_preview(index) { self.pilot_status = Some(error); }
                }
                if let Some((label, path)) = draft_proof_preview {
                    let result = std::fs::metadata(&path).map_err(|e| e.to_string()).and_then(|meta| {
                        if meta.len() > 32 * 1024 * 1024 { return Err("proof PNG exceeds 32 MiB".into()); }
                        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                        let hash = format!("{:x}", Sha256::digest(&bytes));
                        self.view_proof_frame(&ctx, &label, &path, &hash)
                    });
                    if let Err(error) = result { self.pilot_status = Some(error); }
                }
                ui.small("Proof PNGs can be exported from FilmCraft's Source monitor. Enter their actual source ticks; sealing validates file bytes and tick ranges. Runtime identity logs must be built from the recorded console, not typed player claims.");
                ui.horizontal(|ui| {
                    ui.label("Reviewed receipt");
                    if ui.text_edit_singleline(&mut self.reviewed_receipt).changed() {
                        self.receipt_summary = None;
                        self.proof_frames.clear();
                        self.proof_texture = None;
                    }
                    if ui.button("Browse").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("Reviewed capture receipt", &["json"]).pick_file() {
                            self.reviewed_receipt = path.display().to_string();
                            self.receipt_summary = None;
                            self.proof_frames.clear();
                            self.proof_texture = None;
                        }
                    }
                });
                if ui.add_enabled(self.pilot_messages.is_none(), egui::Button::new("Author new hash-bound review receipt")).clicked() {
                    if let Err(error) = self.author_review_receipt() { self.pilot_status = Some(error); }
                }
                if ui.button("Inspect review record and proof frames").clicked() {
                    if let Err(error) = self.inspect_receipt() { self.receipt_summary = Some(error); }
                }
                if let Some(summary) = &self.receipt_summary { ui.small(summary); }
                for (label, path, hash) in self.proof_frames.clone() {
                    if ui.button(format!("View hash-bound proof frame: {label}")).clicked() {
                        if let Err(error) = self.view_proof_frame(&ctx, &label, &path, &hash) {
                            self.pilot_status = Some(error);
                        }
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("New accepted package folder");
                    ui.text_edit_singleline(&mut self.reviewed_package_destination);
                    if ui.button("Choose parent").clicked() {
                        if let Some(parent) = rfd::FileDialog::new().pick_folder() {
                            let name = format!("reviewed-package-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
                            self.reviewed_package_destination = parent.join(name).display().to_string();
                        }
                    }
                });
                if ui.add_enabled(self.pilot_messages.is_none(), egui::Button::new("Validate receipt and seal accepted package")).clicked() {
                    if let Err(error) = self.seal_reviewed_pilot() { self.pilot_status = Some(error); }
                }
            });
            if let Some(status) = &self.pilot_status {
                ui.label(format!("Offline capture: {status}"));
            }
            if self.pilot_messages.is_some() {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            if self.package_import_messages.is_some() {
                ui.label("Validating accepted package and creating editable project...");
                if ui.add_enabled(!self.package_import_cancel.load(Ordering::Relaxed),
                    egui::Button::new("Cancel package import")).clicked() {
                    self.package_import_cancel.store(true, Ordering::Relaxed);
                }
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            if let Some(path) = self.imported_project_path.clone() {
                ui.colored_label(egui::Color32::LIGHT_GREEN,
                    format!("Editable project created: {}", path.display()));
                if let Some(summary) = &self.package_import_summary { ui.small(summary.as_str()); }
                if ui.button("Edit imported project").clicked() {
                    if let Err(error) = self.open_saved_project(&path) { self.error = Some(error); }
                }
            }
            if self.import_messages.is_some() {
                if self.import_stage.as_deref() == Some("Copying source") {
                    if let Some((copied, total)) = self.import_cancel.snapshot_progress() {
                        ui.add(egui::ProgressBar::new(copied as f32 / total as f32)
                            .text(format!("Copied {:.1} / {:.1} MiB", copied as f64 / 1_048_576.0,
                                total as f64 / 1_048_576.0)));
                    }
                } else if self.import_stage.as_deref() == Some("Parsing demo") {
                    if let Some((tick, total)) = self.import_cancel.parsing_progress() {
                        ui.add(egui::ProgressBar::new(tick as f32 / total as f32)
                            .text(format!("Decoded tick {tick} / {total}")));
                    }
                }
                if ui.add_enabled(self.import_cancel.can_cancel(), egui::Button::new("Cancel demo import")).clicked() {
                    self.import_cancel.request_cancel();
                }
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            if let Some(result) = &self.package_result {
                ui.colored_label(egui::Color32::LIGHT_GREEN, result);
            } else {
                ui.colored_label(egui::Color32::YELLOW, "No capture media verified in this view.");
            }
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            self.show_library(ui);
            ui.separator();
            let mut build_requested = false;
            if let Some(draft) = &self.draft {
                ui.heading(format!("Capture draft: {}", draft.package_id));
                if let Some(path) = &self.draft_label { ui.small(path); }
                ui.label(format!("{} separate takes", draft.inputs.len()));
                for input in &draft.inputs {
                    ui.collapsing(format!("{} / {} / order {}", input.clip.clip_id, input.clip.event_id, input.clip.group_order), |ui| {
                        ui.label(format!("Source: {}", input.source_path.display()));
                        ui.label(format!("{} / round {} / ticks {}-{}", input.clip.map, input.clip.round, input.clip.start_tick, input.clip.end_tick));
                        ui.label(format!("Declared audio: {:?}, {} channels, {} Hz", input.clip.audio.source, input.clip.audio.channels, input.clip.audio.sample_rate));
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("New package folder");
                    ui.text_edit_singleline(&mut self.package_destination);
                });
                build_requested = ui.button("Build stream-probed package").clicked();
                ui.small("Stream checks do not replace listening, source identity, or visual review.");
                ui.separator();
            }
            if build_requested { let _ = self.build_capture_package(); }
            let Some(analyzed) = &self.analyzed else {
                ui.heading("Import a CS2 demo or open raw facts");
                ui.label("Open a CS2 demo for local grenade, combat, and flash event analysis. Unsupported schemas fail before saving; economy and game capture are separate.");
                return;
            };
            if let Some(path) = &self.source_label {
                ui.small(path);
            }
            if self.capabilities == Some(ParserCapabilities::GrenadeAnalysisOnlyV1) {
                ui.colored_label(egui::Color32::YELLOW, "Grenade analysis only: combat damage, blinds, kills, assists, and economy are unavailable. These metrics are not zero-valued results.");
            } else if matches!(self.capabilities,
                Some(ParserCapabilities::NativeCombatFlashV1 | ParserCapabilities::NativeCombatFullV2)) {
                ui.colored_label(egui::Color32::YELLOW, "Native grenade, combat, and flash facts: economy and verified POV capture remain unavailable.");
            }
            let ambiguous = analyzed.effect_facts.iter().filter(|fact| fact.resolution == "ambiguous").count();
            let unresolved = analyzed.effect_facts.iter().filter(|fact| fact.resolution == "unresolved").count();
            ui.small(format!("Effect facts: {}; ambiguous: {ambiguous}; unresolved: {unresolved}", analyzed.effect_facts.len()));
            ui.horizontal(|ui| {
                ui.label(format!("Map: {}", analyzed.map_name));
                ui.label(format!("Rounds: {}", analyzed.rounds.len()));
                ui.label(format!("Players: {}", analyzed.players.len()));
                ui.label(format!("Events: {}", analyzed.events.len()));
                ui.label(format!("Tick rate: {:.1}", analyzed.tick_rate));
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for event in &analyzed.events {
                    let label = format!(
                        "Round {} · {} · {} · tick {}",
                        event.round_number,
                        event.thrower_name,
                        event.utility_type.as_str(),
                        event.throw_tick
                    );
                    ui.collapsing(label, |ui| {
                        ui.label(format!("Player SteamID64: {}", event.thrower_steamid));
                        ui.label(format!("Team: {}", event.thrower_team));
                        ui.label(format!("Trajectory samples: {}", event.trajectory.len()));
                        if self.capabilities.is_some_and(ParserCapabilities::supports_combat) {
                            ui.label(format!("Enemy damage: {}; enemy flash duration: {}; kills: {}; flash assists: {}",
                                event.damage_attribution_complete.then(|| event.damage_enemy.to_string()).unwrap_or("unavailable".into()),
                                event.blind_attribution_complete.then(|| format!("{:.1}s", event.blind_enemy_duration)).unwrap_or("unavailable".into()),
                                event.kill_attribution_complete.then(|| event.kills.to_string()).unwrap_or("unavailable".into()),
                                event.assist_attribution_complete.then(|| event.flash_assists.to_string()).unwrap_or("unavailable".into())));
                            if !event.blind_attribution_complete || !event.damage_attribution_complete ||
                                !event.kill_attribution_complete || !event.assist_attribution_complete {
                                ui.small("Overlapping or unresolved effects: raw facts retained; affected totals unavailable.");
                            }
                        }
                        if let Some(tick) = event.detonate_tick {
                            ui.label(format!("Detonation tick: {tick}"));
                        }
                    });
                }
            });
        });
        if let (Some(texture), Some(label)) = (self.proof_texture.clone(), self.proof_label.clone()) {
            let mut open = true;
            egui::Window::new(label).open(&mut open).resizable(true).show(&ctx, |ui| {
                let size = texture.size_vec2();
                let scale = (960.0 / size.x).min(600.0 / size.y).min(1.0);
                ui.image((texture.id(), size * scale));
                ui.small("This frame is a receipt claim. Check full motion, camera identity, and game audio before accepting a capture.");
            });
            if !open { self.proof_texture = None; self.proof_label = None; }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DemoStrikeApp, EditorReturnGate, editor_return_gate, load_editor_session};
    use crate::demo_import::ParserCapabilities;
    use crate::demo_import::DemoFactsParser;
    use crate::parser::raw::RawDemo;
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;
    use crate::demo_import::ImportControl;
    use std::time::Duration;

    #[test]
    fn stale_discard_confirmation_cannot_close_an_exporting_editor() {
        assert_eq!(editor_return_gate(true, false, true, true), EditorReturnGate::Busy);
        assert_eq!(editor_return_gate(false, true, true, true), EditorReturnGate::Busy);
        assert_eq!(editor_return_gate(true, false, false, false), EditorReturnGate::Busy);
        assert_eq!(editor_return_gate(false, false, true, false), EditorReturnGate::ConfirmDiscard);
        assert_eq!(editor_return_gate(false, false, true, true), EditorReturnGate::Ready);
        assert_eq!(editor_return_gate(false, false, false, false), EditorReturnGate::Ready);
    }

    #[test]
    fn proof_preview_rejects_bytes_changed_after_receipt() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("proof.png");
        fs::write(&path, b"changed pixels").unwrap();
        let mut app = DemoStrikeApp::default();
        let error = app.view_proof_frame(&eframe::egui::Context::default(),
            "pov tick 12", &path, &"0".repeat(64)).unwrap_err();
        assert!(error.contains("changed since the reviewed receipt"));
        assert!(app.proof_texture.is_none());
    }

    struct UiFixtureParser { wait_for_cancel: bool }
    impl DemoFactsParser for UiFixtureParser {
        fn parse(&self, _path: &Path, cancelled: &ImportControl) -> Result<RawDemo, String> {
            if self.wait_for_cancel {
                while !cancelled.is_cancelled() {
                    std::thread::sleep(Duration::from_millis(5));
                }
                return Err("demo import cancelled".into());
            }
            serde_json::from_slice(br#"{"version":1,"header":{"mapName":"de_train",
                "serverName":null,"demoTicks":6400,"playbackTime":100.0}}"#)
                .map_err(|error| error.to_string())
        }
    }

    fn wait_for_import(app: &mut DemoStrikeApp) {
        for _ in 0..200 {
            app.poll_demo_import();
            if app.import_messages.is_none() { return; }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("demo import worker did not complete");
    }

    #[test]
    fn native_import_worker_persists_demo_and_cancel_keeps_previous_analysis() {
        let dir = tempfile::tempdir().unwrap();
        let demo = dir.path().join("fixture.dem");
        fs::write(&demo, b"PBDEMS2\0fixture").unwrap();
        let mut app = DemoStrikeApp::default();
        app.database_path = dir.path().join("matches.sqlite").display().to_string();
        app.start_demo_import_with_parser(&demo, Arc::new(UiFixtureParser { wait_for_cancel: false })).unwrap();
        wait_for_import(&mut app);
        assert_eq!(app.analyzed.as_ref().unwrap().map_name, "de_train");
        assert!(app.error.is_none());
        let saved = crate::db::queries::list_matches(&crate::db::open(Path::new(&app.database_path)).unwrap()).unwrap();
        assert_eq!(saved.len(), 1);

        app.start_demo_import_with_parser(&demo, Arc::new(UiFixtureParser { wait_for_cancel: true })).unwrap();
        app.import_cancel.request_cancel();
        wait_for_import(&mut app);
        assert!(app.error.as_ref().unwrap().contains("cancelled"));
        assert_eq!(app.analyzed.as_ref().unwrap().map_name, "de_train");
        let saved = crate::db::queries::list_matches(&crate::db::open(Path::new(&app.database_path)).unwrap()).unwrap();
        assert_eq!(saved.len(), 1);

        app.start_demo_import_with_parser(&demo, Arc::new(UiFixtureParser { wait_for_cancel: true })).unwrap();
        let cancel_signal = Arc::clone(&app.import_cancel);
        drop(app);
        assert!(cancel_signal.is_cancelled());
    }

    #[test]
    fn native_view_model_loads_analyzed_raw_facts_and_reports_bad_input() {
        let dir = tempfile::TempDir::new().unwrap();
        let good = dir.path().join("raw.json");
        fs::write(
            &good,
            br#"{
            "version": 1,
            "header": {"mapName": "de_mirage", "serverName": null,
                "demoTicks": 6400, "playbackTime": 100.0}
        }"#,
        )
        .unwrap();
        let mut app = DemoStrikeApp::default();
        app.load_raw_json(&good).unwrap();
        assert_eq!(app.analyzed.as_ref().unwrap().map_name, "de_mirage");
        assert!(app.error.is_none());

        let bad = dir.path().join("bad.json");
        fs::write(&bad, b"not JSON").unwrap();
        assert!(app.load_raw_json(&bad).is_err());
        assert!(app.error.is_some());
        assert_eq!(app.analyzed.as_ref().unwrap().map_name, "de_mirage");
    }

    #[test]
    fn native_view_model_reviews_draft_and_refuses_fake_media() {
        let dir = tempfile::TempDir::new().unwrap();
        let source = dir.path().join("fake.mp4");
        fs::write(&source, b"abc").unwrap();
        let draft_path = dir.path().join("draft.json");
        let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let draft = serde_json::json!({"package_id":"native-pilot","inputs":[{
            "source_path":source,"clip":{
                "clip_id":"take-1","relative_path":"","sha256":"",
                "demo_sha256":hash,"event_id":"event-1","player_steamid64":"76561198000000001",
                "map":"de_mirage","round":4,"utility_type":"smoke","start_tick":1000,"end_tick":1100,
                "camera_role":"projectile","duration_ms":1000,"width":1920,"height":1080,
                "fps_num":60,"fps_den":1,"audio":{"source":"hlae_wav","channels":2,
                    "sample_rate":48000,"verified":true},"qc_status":"accepted","group_order":1
            }
        }]});
        fs::write(&draft_path, serde_json::to_vec(&draft).unwrap()).unwrap();
        let mut app = DemoStrikeApp::default();
        app.load_capture_draft(&draft_path).unwrap();
        assert_eq!(app.draft.as_ref().unwrap().inputs.len(), 1);
        assert!(app.package_destination.ends_with("native-pilot-package"));
        assert!(app.build_capture_package().is_err());
        assert!(app.package_result.is_none());
        assert!(!std::path::Path::new(&app.package_destination).exists());
    }
}

