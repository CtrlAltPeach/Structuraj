pub mod db;
pub mod model;
pub mod query;
pub mod scan;
pub mod tree;

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use model::*;

pub struct AppState {
    conn: Mutex<Option<Connection>>,
    db_path: PathBuf,
}

impl AppState {
    fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        let guard = self.conn.lock().map_err(|_| "состояние повреждено")?;
        let conn = guard.as_ref().ok_or("папка ещё не просканирована")?;
        f(conn)
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    id: i64,
    path: String,
    records: i64,
}

#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (tx, mut rx) = tauri::async_runtime::channel::<Option<String>>(1);
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.blocking_send(folder.map(|f| f.to_string()));
    });
    Ok(rx.recv().await.flatten())
}

#[tauri::command]
async fn scan_folder(app: tauri::AppHandle, path: String) -> Result<ScanSummary, String> {
    let app2 = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<ScanSummary, String> {
        let state = app2.state::<AppState>();
        let root = PathBuf::from(&path);
        if !root.is_dir() {
            return Err(format!("папка не найдена: {path}"));
        }

        // Старое соединение закрываем: файл базы пересоздаётся с нуля.
        {
            let mut guard = state.conn.lock().map_err(|_| "состояние повреждено")?;
            *guard = None;
        }

        let conn = db::open_fresh(&state.db_path).map_err(|e| e.to_string())?;

        let mut last = std::time::Instant::now();
        let summary = scan::scan(&root, &conn, |p| {
            // Прогресс шлём не чаще ~20 раз в секунду, иначе UI захлебнётся.
            if p.files_done == p.files_total || last.elapsed().as_millis() > 50 {
                last = std::time::Instant::now();
                let _ = app2.emit("scan:progress", p);
            }
        })
        .map_err(|e| e.to_string())?;

        {
            let mut guard = state.conn.lock().map_err(|_| "состояние повреждено")?;
            *guard = Some(conn);
        }
        Ok(summary)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_tree(state: State<'_, AppState>, mode: String) -> Result<Vec<TreeNode>, String> {
    state.with_conn(|conn| tree::build(conn, &mode).map_err(|e| e.to_string()))
}

#[tauri::command]
fn get_values(state: State<'_, AppState>, query: ValueQuery) -> Result<ValuePage, String> {
    state.with_conn(|conn| query::values(conn, &query).map_err(|e| e.to_string()))
}

#[tauri::command]
fn get_records(
    state: State<'_, AppState>,
    path_ids: Vec<i64>,
    offset: i64,
    limit: i64,
) -> Result<RecordPage, String> {
    state.with_conn(|conn| query::records(conn, &path_ids, offset, limit).map_err(|e| e.to_string()))
}

#[tauri::command]
fn get_record(state: State<'_, AppState>, file_id: i64, rec: i64) -> Result<RecordView, String> {
    state.with_conn(|conn| query::record(conn, file_id, rec))
}

#[tauri::command]
fn get_record_index(
    state: State<'_, AppState>,
    path_ids: Vec<i64>,
    file_id: i64,
    rec: i64,
) -> Result<i64, String> {
    state.with_conn(|conn| {
        query::record_index(conn, &path_ids, file_id, rec).map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn get_errors(state: State<'_, AppState>) -> Result<Vec<ErrorRow>, String> {
    state.with_conn(|conn| query::errors(conn).map_err(|e| e.to_string()))
}

#[tauri::command]
fn get_files(state: State<'_, AppState>) -> Result<Vec<FileRow>, String> {
    state.with_conn(|conn| {
        query::files(conn)
            .map(|rows| {
                rows.into_iter()
                    .map(|(id, path, records)| FileRow { id, path, records })
                    .collect()
            })
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn get_summary(state: State<'_, AppState>) -> Result<Option<ScanSummary>, String> {
    state.with_conn(|conn| Ok(query::summary(conn)))
}

#[tauri::command]
fn export_md(state: State<'_, AppState>, mode: String) -> Result<String, String> {
    state.with_conn(|conn| {
        let root: String = conn
            .query_row("SELECT v FROM meta WHERE k = 'root'", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let summary = query::summary(conn).unwrap_or_default();
        let nodes = tree::build(conn, &mode).map_err(|e| e.to_string())?;
        let md = tree::to_markdown(&nodes, &root, &mode, &summary);

        let out = PathBuf::from(&root).join("STRUCTURE.md");
        std::fs::write(&out, md).map_err(|e| format!("не удалось записать {out:?}: {e}"))?;
        Ok(out.to_string_lossy().to_string())
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app
                .path()
                .app_local_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            app.manage(AppState {
                conn: Mutex::new(None),
                db_path: dir.join("index.sqlite"),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            pick_folder,
            scan_folder,
            get_tree,
            get_values,
            get_records,
            get_record,
            get_record_index,
            get_errors,
            get_files,
            get_summary,
            export_md
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
