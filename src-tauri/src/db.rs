//! SQLite-индекс: схема, константы типов, открытие базы.
//!
//! База пересоздаётся при каждом скане и живёт вне рабочей папки пользователя,
//! чтобы не мусорить в целевом каталоге и не попадать в синхронизацию OneDrive.

use rusqlite::Connection;
use std::path::Path;

/// Длинные строки режем: индекс нужен для поиска и сортировки, а не для хранения блобов.
pub const MAX_TEXT: usize = 4096;

pub const T_NULL: i64 = 0;
pub const T_BOOL: i64 = 1;
pub const T_NUM: i64 = 2;
pub const T_STR: i64 = 3;
pub const T_OBJ: i64 = 4;
pub const T_ARR: i64 = 5;

pub const M_NULL: i64 = 1 << 0;
pub const M_BOOL: i64 = 1 << 1;
pub const M_NUM: i64 = 1 << 2;
pub const M_STR: i64 = 1 << 3;
pub const M_OBJ: i64 = 1 << 4;
pub const M_ARR: i64 = 1 << 5;

pub fn type_name(t: i64) -> &'static str {
    match t {
        T_NULL => "null",
        T_BOOL => "bool",
        T_NUM => "number",
        T_STR => "string",
        T_OBJ => "object",
        T_ARR => "array",
        _ => "?",
    }
}

pub fn mask_names(mask: i64) -> Vec<String> {
    let mut out = Vec::new();
    for (bit, name) in [
        (M_NULL, "null"),
        (M_BOOL, "bool"),
        (M_NUM, "number"),
        (M_STR, "string"),
        (M_OBJ, "object"),
        (M_ARR, "array"),
    ] {
        if mask & bit != 0 {
            out.push(name.to_string());
        }
    }
    out
}

const SCHEMA: &str = r#"
PRAGMA journal_mode = MEMORY;
PRAGMA synchronous = OFF;
PRAGMA temp_store = MEMORY;
PRAGMA cache_size = -262144;

CREATE TABLE files (
    id       INTEGER PRIMARY KEY,
    abs_path TEXT    NOT NULL,
    rel_path TEXT    NOT NULL,
    kind     TEXT    NOT NULL,
    bytes    INTEGER NOT NULL DEFAULT 0,
    records  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE paths (
    id        INTEGER PRIMARY KEY,
    path      TEXT    NOT NULL UNIQUE,
    parent_id INTEGER,
    key       TEXT    NOT NULL,
    depth     INTEGER NOT NULL,
    is_array  INTEGER NOT NULL DEFAULT 0,
    cnt       INTEGER NOT NULL DEFAULT 0,
    -- Сколько вхождений несут хоть какое-то значение. null, пустая строка,
    -- {} и [] сюда не попадают: по ним ключ считается пустым.
    nonempty  INTEGER NOT NULL DEFAULT 0,
    mask      INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE vals (
    path_id INTEGER NOT NULL,
    file_id INTEGER NOT NULL,
    rec     INTEGER NOT NULL,
    vtype   INTEGER NOT NULL,
    txt     TEXT,
    num     REAL,
    trunc   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE errors (
    id      INTEGER PRIMARY KEY,
    file    TEXT NOT NULL,
    line    INTEGER,
    message TEXT NOT NULL
);

CREATE TABLE meta (
    k TEXT PRIMARY KEY,
    v TEXT NOT NULL
);
"#;

/// Индексы строим после массовой вставки — так вставка идёт в разы быстрее.
const INDEXES: &str = r#"
CREATE INDEX idx_vals_path ON vals(path_id);
CREATE INDEX idx_vals_rec  ON vals(file_id, rec);
"#;

pub fn open_fresh(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::remove_file(path);
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

pub fn build_indexes(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(INDEXES)
}
