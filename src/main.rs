use demostrike::ui::DemoStrikeApp;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn parse_args(args: &[OsString]) -> Result<(Option<PathBuf>, Option<PathBuf>), &'static str> {
    if args.len() % 2 != 0 || args.len() > 4 {
        return Err(
            "usage: demostrike [--project saved-project.cstrike|saved-project.fcproj] [--capture ABSOLUTE.png]",
        );
    }
    let (mut project, mut capture) = (None, None);
    for pair in args.chunks_exact(2) {
        let path = PathBuf::from(&pair[1]);
        if pair[0] == "--project" && project.is_none() {
            project = Some(path);
        } else if pair[0] == "--capture" && capture.is_none() {
            if !path.is_absolute()
                || !path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("png"))
            {
                return Err("Capture output must be an absolute .png path");
            }
            capture = Some(path);
        } else {
            return Err(
                "usage: demostrike [--project saved-project.cstrike|saved-project.fcproj] [--capture ABSOLUTE.png]",
            );
        }
    }
    Ok((project, capture))
}

fn main() -> eframe::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let (project, capture) = match parse_args(&args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let capturing = capture.is_some();
    let capture_status = Arc::new(Mutex::new(None));
    let capture_status_ui = Arc::clone(&capture_status);
    eframe::run_native(
        "DemoStrike — utility analysis",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Wgpu,
            viewport: if capturing {
                eframe::egui::ViewportBuilder::default().with_inner_size([1440.0, 900.0])
            } else {
                Default::default()
            },
            ..Default::default()
        },
        Box::new(move |context| {
            let mut app = match &project {
                Some(path) => DemoStrikeApp::with_saved_project(context, path)
                    .map_err(std::io::Error::other)?,
                None => {
                    DemoStrikeApp::with_native_context(context).map_err(std::io::Error::other)?
                }
            };
            if let Some(path) = capture.clone() {
                app.capture_once(path, Arc::clone(&capture_status_ui));
            }
            Ok(Box::new(app))
        }),
    )?;
    if capturing {
        match capture_status.lock().unwrap().take() {
            Some(Ok(())) => {}
            Some(Err(error)) => {
                eprintln!("DemoStrike capture: {error}");
                std::process::exit(1);
            }
            None => {
                eprintln!("DemoStrike capture: window closed before screenshot completed");
                std::process::exit(1);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_args;
    use std::ffi::OsString;

    #[test]
    fn capture_requires_absolute_png_and_does_not_select_a_project() {
        let path = std::env::temp_dir().join("demostrike-capture.png");
        let args = [OsString::from("--capture"), path.as_os_str().to_os_string()];
        let (project, capture) = parse_args(&args).unwrap();
        assert!(project.is_none());
        assert_eq!(capture, Some(path.clone()));
        assert!(
            parse_args(&[OsString::from("--capture"), OsString::from("relative.png")]).is_err()
        );
        assert!(
            parse_args(&[
                OsString::from("--capture"),
                OsString::from("C:\\not-a-png.txt")
            ])
            .is_err()
        );
        let combined = [
            OsString::from("--project"),
            OsString::from("offline-edit.fcproj"),
            OsString::from("--capture"),
            path.as_os_str().to_os_string(),
        ];
        let (project, capture) = parse_args(&combined).unwrap();
        assert_eq!(
            project,
            Some(std::path::PathBuf::from("offline-edit.fcproj"))
        );
        assert_eq!(capture, Some(path));
        assert!(
            parse_args(&[
                OsString::from("--project"),
                OsString::from("a.cstrike"),
                OsString::from("--project"),
                OsString::from("b.fcproj"),
            ])
            .is_err()
        );
    }
}
