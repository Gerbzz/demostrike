pub mod analysis;
pub mod db;
pub mod demo_import;
pub mod capture_request;
pub mod edit_handoff;
pub mod capture_project;
pub mod capture_receipt;
pub mod capture_qc;
pub mod capture_preflight;
pub mod capture_pilot;
pub mod event_pilot;
pub mod owned_capture;
pub mod native_grenade_parser;
pub mod domain;
pub mod error;
pub mod parser;
pub mod package;
pub mod media_probe;
pub mod media_tools;
pub mod runtime_resources;
#[cfg(feature = "native-ui")]
pub mod ui;

use analysis::{AnalyzedMatch, build_match, validate_import};
use error::AppResult;
use parser::raw::RawDemo;

pub fn analyze_raw_json(bytes: &[u8]) -> AppResult<AnalyzedMatch> {
    let raw: RawDemo = serde_json::from_slice(bytes)?;
    let analyzed = build_match(&raw);
    validate_import(&raw, &analyzed)?;
    Ok(analyzed)
}

#[cfg(test)]
mod tests {
    use super::analyze_raw_json;

    #[test]
    fn native_rust_entry_point_analyzes_existing_raw_demo_facts() {
        let raw = br#"{
            "version": 1,
            "header": {"mapName": "de_mirage", "serverName": null,
                "demoTicks": 6400, "playbackTime": 100.0}
        }"#;
        let analyzed = analyze_raw_json(raw).unwrap();
        assert_eq!(analyzed.map_name, "de_mirage");
        assert_eq!(analyzed.tick_rate, 64.0);
        assert!(analyzed.events.is_empty());
    }
}

