//! First persistent backend slice for the preserved CS Demo Manager renderer.
//! Unsupported protocol requests fail explicitly until their contracts are ported.
use rusqlite::Connection;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

struct Store {
    root: PathBuf,
}

impl Store {
    fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn settings(&self, defaults: Value) -> Result<Value, String> {
        fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        let path = self.root.join("csdm-settings.json");
        if path.exists() {
            return serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
                .map_err(|error| format!("Could not read saved settings: {error}"));
        }
        if defaults.is_null() {
            return Err("Settings have not been initialized for this profile.".into());
        }
        self.save_settings(&defaults)?;
        Ok(defaults)
    }

    fn save_settings(&self, settings: &Value) -> Result<(), String> {
        fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        let path = self.root.join("csdm-settings.json");
        let bytes = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
        fs::write(path, bytes).map_err(|error| error.to_string())
    }

    fn database(&self) -> Result<Connection, String> {
        fs::create_dir_all(&self.root).map_err(|error| error.to_string())?;
        let connection = Connection::open(self.root.join("csdm-library.sqlite"))
            .map_err(|error| format!("Could not open the local library: {error}"))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS matches (id INTEGER PRIMARY KEY, checksum TEXT NOT NULL UNIQUE);
             CREATE TABLE IF NOT EXISTS maps (payload TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS cameras (payload TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS tags (payload TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS faceit_accounts (payload TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS five_eplay_accounts (payload TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS renown_accounts (payload TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS ignored_steam_accounts (payload TEXT NOT NULL);",
        ).map_err(|error| format!("Could not initialize the local library: {error}"))?;
        Ok(connection)
    }

    fn payloads(connection: &Connection, table: &str) -> Result<Vec<Value>, String> {
        let mut statement = connection
            .prepare(&format!("SELECT payload FROM {table}"))
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?;
        rows.map(|row| {
            let payload = row.map_err(|error| error.to_string())?;
            serde_json::from_str(&payload).map_err(|error| error.to_string())
        })
        .collect()
    }

    fn request(
        &self,
        name: &str,
        payload: Option<Value>,
        settings: Value,
    ) -> Result<Value, String> {
        match name {
            "connect-database" => {
                if payload.is_some_and(|value| !value.is_null()) {
                    return Err("External CS Demo Manager database settings are not supported by this Tauri port.".to_owned());
                }
                self.database()?;
                Ok(Value::Null)
            }
            "init-application" => {
                let connection = self.database()?;
                let mut statement = connection
                    .prepare("SELECT checksum FROM matches")
                    .map_err(|error| error.to_string())?;
                let checksums = statement
                    .query_map([], |row| row.get::<_, String>(0))
                    .map_err(|error| error.to_string())?
                    .map(|row| row.map_err(|error| error.to_string()))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(json!({
                    "matchChecksums": checksums,
                    "maps": Self::payloads(&connection, "maps")?,
                    "cameras": Self::payloads(&connection, "cameras")?,
                    "tags": Self::payloads(&connection, "tags")?,
                    "settings": settings,
                    "faceitAccounts": Self::payloads(&connection, "faceit_accounts")?,
                    "fiveEPlayAccounts": Self::payloads(&connection, "five_eplay_accounts")?,
                    "renownAccounts": Self::payloads(&connection, "renown_accounts")?,
                    "ignoredSteamAccounts": Self::payloads(&connection, "ignored_steam_accounts")?,
                    "analyses": [], "downloads": [], "videos": [], "isVideoQueuePaused": false
                }))
            }
            "fetch-matches-table" => {
                let connection = self.database()?;
                let count: i64 = connection
                    .query_row("SELECT COUNT(*) FROM matches", [], |row| row.get(0))
                    .map_err(|error| error.to_string())?;
                if count != 0 {
                    return Err("Match rows exist, but the full CS Demo Manager MatchTable migration is not implemented yet.".into());
                }
                Ok(json!([]))
            }
            _ => Err(format!(
                "CS Demo Manager request '{name}' has not been ported to the Rust backend."
            )),
        }
    }
}

fn app_store(app: &tauri::AppHandle) -> Result<Store, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    Ok(Store::new(root))
}

#[tauri::command]
pub async fn csdm_choose_library(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("SQLite library", &["sqlite", "sqlite3", "db"])
            .blocking_pick_file()
            .map(|file| {
                file.into_path()
                    .map_err(|error| error.to_string())?
                    .into_os_string()
                    .into_string()
                    .map_err(|_| "The selected library path is not valid UTF-8.".to_owned())
            })
            .transpose()
    })
    .await
    .map_err(|error| error.to_string())?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoFile {
    name: String,
    path: String,
    size_bytes: u64,
    modified_at: Option<u64>,
}

fn demo_files(folder: &str) -> Result<Vec<DemoFile>, String> {
    let entries =
        fs::read_dir(folder).map_err(|error| format!("Could not read demo folder: {error}"))?;
    let mut demos = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("Could not read demo folder: {error}"))?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("dem"))
        {
            continue;
        }
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        demos.push(DemoFile {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: path.to_string_lossy().into_owned(),
            size_bytes: metadata.len(),
            modified_at: metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs()),
        });
    }
    demos.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(demos)
}

#[tauri::command]
pub async fn csdm_list_demo_files(folder: String) -> Result<Vec<DemoFile>, String> {
    tauri::async_runtime::spawn_blocking(move || demo_files(&folder))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn csdm_choose_demo_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_folder()
            .map(|folder| {
                folder
                    .into_path()
                    .map_err(|error| error.to_string())?
                    .into_os_string()
                    .into_string()
                    .map_err(|_| "The selected demo folder path is not valid UTF-8.".to_owned())
            })
            .transpose()
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn csdm_load_settings(app: tauri::AppHandle, defaults: Value) -> Result<Value, String> {
    app_store(&app)?.settings(defaults)
}

#[tauri::command]
pub fn csdm_save_settings(app: tauri::AppHandle, settings: Value) -> Result<(), String> {
    app_store(&app)?.save_settings(&settings)
}

#[tauri::command]
pub fn csdm_request(
    app: tauri::AppHandle,
    name: String,
    payload: Option<Value>,
) -> Result<Value, String> {
    let store = app_store(&app)?;
    let settings = store.settings(Value::Null)?;
    store.request(&name, payload, settings)
}

#[tauri::command]
pub fn csdm_window_action(window: tauri::Window, action: String) -> Result<bool, String> {
    match action.as_str() {
        "is-maximized" => window.is_maximized().map_err(|error| error.to_string()),
        "maximize" => window
            .maximize()
            .map(|_| true)
            .map_err(|error| error.to_string()),
        "unmaximize" => window
            .unmaximize()
            .map(|_| true)
            .map_err(|error| error.to_string()),
        "minimize" => window
            .minimize()
            .map(|_| true)
            .map_err(|error| error.to_string()),
        "close" => window
            .close()
            .map(|_| true)
            .map_err(|error| error.to_string()),
        _ => Err(format!("Unsupported window action: {action}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    #[test]
    fn fresh_library_reports_actual_empty_state_and_rejects_unsupported_data() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::new(directory.path().to_owned());
        let settings = json!({"ui":{"theme":"dark"}});
        assert_eq!(store.settings(settings.clone()).unwrap(), settings);
        assert_eq!(
            store
                .request("connect-database", None, settings.clone())
                .unwrap(),
            Value::Null
        );
        assert!(
            store
                .request(
                    "connect-database",
                    Some(json!({"host":"localhost"})),
                    settings.clone()
                )
                .unwrap_err()
                .contains("External CS Demo Manager database settings")
        );
        let initial = store
            .request("init-application", None, settings.clone())
            .unwrap();
        assert_eq!(initial["matchChecksums"], json!([]));
        assert_eq!(initial["maps"], json!([]));
        assert_eq!(
            store
                .request("fetch-matches-table", None, settings.clone())
                .unwrap(),
            json!([])
        );
        store
            .database()
            .unwrap()
            .execute(
                "INSERT INTO matches (checksum) VALUES (?1)",
                params!["real-checksum"],
            )
            .unwrap();
        assert!(
            store
                .request("fetch-matches-table", None, settings)
                .unwrap_err()
                .contains("MatchTable migration")
        );
        assert!(
            store
                .request("delete-matches", None, Value::Null)
                .unwrap_err()
                .contains("not been ported")
        );
    }

    #[test]
    fn demo_folder_lists_only_direct_demo_files_without_changing_them() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("alpha.dem");
        let second = directory.path().join("Beta.DEM");
        fs::write(&first, b"synthetic demo one").unwrap();
        fs::write(&second, b"synthetic demo two").unwrap();
        fs::write(directory.path().join("notes.txt"), b"not a demo").unwrap();
        let nested = directory.path().join("nested");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("hidden.dem"), b"not direct").unwrap();

        let demos = demo_files(directory.path().to_str().unwrap()).unwrap();
        assert_eq!(demos.len(), 2);
        assert_eq!(demos[0].name, "alpha.dem");
        assert_eq!(demos[0].size_bytes, 18);
        assert_eq!(demos[1].name, "Beta.DEM");
        assert_eq!(fs::read(&first).unwrap(), b"synthetic demo one");
        assert_eq!(fs::read(&second).unwrap(), b"synthetic demo two");
    }
}
