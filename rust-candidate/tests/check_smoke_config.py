"""Keep CDP exposure confined to the hosted smoke build configuration."""

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
normal = json.loads((ROOT / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))
smoke = json.loads((ROOT / "src-tauri" / "tauri.smoke.conf.json").read_text(encoding="utf-8"))
original_smoke = json.loads((ROOT / "src-tauri" / "tauri.upstream.smoke.conf.json").read_text(encoding="utf-8"))
original_normal = json.loads((ROOT / "src-tauri" / "tauri.upstream.conf.json").read_text(encoding="utf-8"))
workflow = (ROOT.parent / ".github" / "workflows" / "rust-candidate-verify.yml").read_text(encoding="utf-8")

for config_path in (ROOT / "src-tauri").glob("tauri*.conf.json"):
    if config_path.name in {"tauri.smoke.conf.json", "tauri.upstream.smoke.conf.json"}:
        continue
    config = json.loads(config_path.read_text(encoding="utf-8"))
    for window in config.get("app", {}).get("windows", []):
        assert "remote-debugging" not in window.get("additionalBrowserArgs", "")
for window in normal["app"]["windows"]:
    assert "dataDirectory" not in window
assert len(smoke["app"]["windows"]) == 1
window = smoke["app"]["windows"][0]
assert "--remote-debugging-address=127.0.0.1" in window["additionalBrowserArgs"]
assert "--remote-debugging-port=19373" in window["additionalBrowserArgs"]
assert "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection" in window["additionalBrowserArgs"]
assert window["dataDirectory"] == "demostrike-ci-webview2"
assert not Path(window["dataDirectory"]).is_absolute()
assert "tauri.smoke.conf.json" in workflow
assert len(original_smoke["app"]["windows"]) == 1
original_window = original_smoke["app"]["windows"][0]
assert "--remote-debugging-address=127.0.0.1" in original_window["additionalBrowserArgs"]
assert "--remote-debugging-port=19375" in original_window["additionalBrowserArgs"]
assert original_window["dataDirectory"] == "demostrike-csdm-ci-webview2"
assert "tauri.upstream.smoke.conf.json" in workflow
assert "Rebuild release host without synthetic parser feature" in workflow
assert "npm run tauri -- build --config src-tauri/tauri.upstream.smoke.conf.json\n" in workflow
assert "EXPECT_FEATURE_FREE: '1'" in workflow
assert original_normal["app"]["security"]["csp"] == normal["app"]["security"]["csp"]
assert original_smoke["app"]["security"]["csp"] == normal["app"]["security"]["csp"]
print("PASS: normal Tauri windows have no CDP flags; both hosted smoke windows bind loopback with distinct data")
