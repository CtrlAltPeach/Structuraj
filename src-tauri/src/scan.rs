//! Обход папки, потоковый разбор JSON/JSONL и наполнение индекса.
//!
//! Ключевая идея — канонический путь: индексы массивов схлопываются в `[]`,
//! поэтому одинаковые ключи из разных файлов и разных элементов массива
//! попадают в один и тот же узел автоматически.

use rusqlite::{params, Connection, Statement};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use crate::db::*;
use crate::model::{ScanProgress, ScanSummary};

/// Больше ошибок с одного файла в отчёт не пишем — иначе битый гигабайт
/// забьёт вкладку миллионом одинаковых строк.
const MAX_ERRORS_PER_FILE: usize = 100;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Json,
    Jsonl,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Json => "json",
            Kind::Jsonl => "jsonl",
        }
    }
}

pub struct PathMeta {
    pub path: String,
    pub parent: Option<i64>,
    pub key: String,
    pub depth: i64,
    pub is_array: bool,
    pub cnt: i64,
    /// Вхождения с непустым значением: null, "", {} и [] не считаются.
    pub nonempty: i64,
    pub mask: i64,
}

/// Интернер путей: строка пути -> id. Счётчики копим в памяти и пишем одним
/// проходом в конце — путей тысячи, а не миллионы, так что это дёшево.
pub struct Interner {
    map: HashMap<String, i64>,
    pub meta: Vec<PathMeta>,
}

impl Interner {
    pub fn new() -> Self {
        let mut me = Interner {
            map: HashMap::new(),
            meta: Vec::new(),
        };
        me.intern("$", None, "$", 0);
        me
    }

    pub fn intern(&mut self, path: &str, parent: Option<i64>, key: &str, depth: i64) -> i64 {
        if let Some(id) = self.map.get(path) {
            return *id;
        }
        let id = self.meta.len() as i64 + 1;
        self.meta.push(PathMeta {
            path: path.to_string(),
            parent,
            key: key.to_string(),
            depth,
            is_array: false,
            cnt: 0,
            nonempty: 0,
            mask: 0,
        });
        self.map.insert(path.to_string(), id);
        id
    }

    pub fn bump(&mut self, id: i64, mask: i64, is_array: bool, non_empty: bool) {
        let m = &mut self.meta[(id - 1) as usize];
        m.cnt += 1;
        m.mask |= mask;
        if non_empty {
            m.nonempty += 1;
        }
        if is_array {
            m.is_array = true;
        }
    }
}

fn truncate_utf8(s: &str) -> (&str, bool) {
    if s.len() <= MAX_TEXT {
        return (s, false);
    }
    let mut end = MAX_TEXT;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    (&s[..end], true)
}

#[allow(clippy::too_many_arguments)]
fn put(
    stmt: &mut Statement<'_>,
    path_id: i64,
    file_id: i64,
    rec: i64,
    vtype: i64,
    txt: Option<&str>,
    num: Option<f64>,
    trunc: bool,
) -> rusqlite::Result<()> {
    stmt.execute(params![
        path_id,
        file_id,
        rec,
        vtype,
        txt,
        num,
        if trunc { 1 } else { 0 }
    ])?;
    Ok(())
}

/// Рекурсивный обход одной записи. `path` мутируется на месте, чтобы не
/// аллоцировать строку на каждый ключ.
#[allow(clippy::too_many_arguments)]
fn walk(
    v: &Value,
    path: &mut String,
    parent: Option<i64>,
    key: &str,
    depth: i64,
    interner: &mut Interner,
    stmt: &mut Statement<'_>,
    file_id: i64,
    rec: i64,
    values: &mut u64,
) -> rusqlite::Result<()> {
    let id = interner.intern(path, parent, key, depth);

    match v {
        Value::Object(map) => {
            interner.bump(id, M_OBJ, false, false);
            if map.is_empty() {
                put(stmt, id, file_id, rec, T_OBJ, Some("{}"), None, false)?;
                *values += 1;
            }
            let base = path.len();
            for (k, val) in map {
                path.push('.');
                path.push_str(k);
                walk(
                    val,
                    path,
                    Some(id),
                    k,
                    depth + 1,
                    interner,
                    stmt,
                    file_id,
                    rec,
                    values,
                )?;
                path.truncate(base);
            }
        }
        Value::Array(arr) => {
            interner.bump(id, M_ARR, true, false);
            if arr.is_empty() {
                put(stmt, id, file_id, rec, T_ARR, Some("[]"), None, false)?;
                *values += 1;
            }
            let base = path.len();
            path.push_str("[]");
            for el in arr {
                walk(
                    el,
                    path,
                    Some(id),
                    "[]",
                    depth + 1,
                    interner,
                    stmt,
                    file_id,
                    rec,
                    values,
                )?;
            }
            path.truncate(base);
        }
        Value::Null => {
            interner.bump(id, M_NULL, false, false);
            put(stmt, id, file_id, rec, T_NULL, None, None, false)?;
            *values += 1;
        }
        Value::Bool(b) => {
            interner.bump(id, M_BOOL, false, true);
            put(
                stmt,
                id,
                file_id,
                rec,
                T_BOOL,
                Some(if *b { "true" } else { "false" }),
                Some(if *b { 1.0 } else { 0.0 }),
                false,
            )?;
            *values += 1;
        }
        Value::Number(n) => {
            interner.bump(id, M_NUM, false, true);
            let s = n.to_string();
            put(stmt, id, file_id, rec, T_NUM, Some(&s), n.as_f64(), false)?;
            *values += 1;
        }
        Value::String(s) => {
            interner.bump(id, M_STR, false, !s.trim().is_empty());
            let (cut, trunc) = truncate_utf8(s);
            put(stmt, id, file_id, rec, T_STR, Some(cut), None, trunc)?;
            *values += 1;
        }
    }
    Ok(())
}

/// Первый непробельный байт решает, как читать `.json`: массив записей или
/// одиночное значение.
fn first_meaningful_byte(path: &Path) -> std::io::Result<Option<u8>> {
    let mut f = File::open(path)?;
    let mut buf = [0u8; 4096];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok(None);
        }
        for b in &buf[..n] {
            if !b.is_ascii_whitespace() && *b != 0xEF && *b != 0xBB && *b != 0xBF {
                return Ok(Some(*b));
            }
        }
    }
}

struct SeqSink<F: FnMut(Value) -> Result<(), String>> {
    f: F,
}

impl<'de, F: FnMut(Value) -> Result<(), String>> serde::de::Visitor<'de> for SeqSink<F> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("массив JSON верхнего уровня")
    }

    fn visit_seq<A>(mut self, mut seq: A) -> Result<(), A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        while let Some(v) = seq.next_element::<Value>()? {
            (self.f)(v).map_err(serde::de::Error::custom)?;
        }
        Ok(())
    }
}

pub struct FileOutcome {
    pub records: u64,
    pub values: u64,
    pub errors: Vec<(Option<i64>, String)>,
}

fn scan_one_file(
    abs: &Path,
    kind: Kind,
    file_id: i64,
    root_id: i64,
    interner: &mut Interner,
    stmt: &mut Statement<'_>,
) -> FileOutcome {
    let mut out = FileOutcome {
        records: 0,
        values: 0,
        errors: Vec::new(),
    };

    match kind {
        Kind::Jsonl => {
            let file = match File::open(abs) {
                Ok(f) => f,
                Err(e) => {
                    out.errors.push((None, format!("не открывается: {e}")));
                    return out;
                }
            };
            let reader = BufReader::with_capacity(1 << 20, file);
            for (i, line) in reader.lines().enumerate() {
                let line = match line {
                    Ok(l) => l,
                    Err(e) => {
                        if out.errors.len() < MAX_ERRORS_PER_FILE {
                            out.errors
                                .push((Some(i as i64 + 1), format!("не читается строка: {e}")));
                        }
                        break;
                    }
                };
                let t = line.trim_start_matches('\u{feff}').trim();
                if t.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Value>(t) {
                    Ok(v) => {
                        interner.bump(root_id, M_ARR, true, false);
                        let mut path = String::from("$[]");
                        let rec = out.records as i64;
                        if let Err(e) = walk(
                            &v,
                            &mut path,
                            Some(root_id),
                            "[]",
                            1,
                            interner,
                            stmt,
                            file_id,
                            rec,
                            &mut out.values,
                        ) {
                            out.errors.push((Some(i as i64 + 1), e.to_string()));
                            break;
                        }
                        out.records += 1;
                    }
                    Err(e) => {
                        if out.errors.len() < MAX_ERRORS_PER_FILE {
                            out.errors.push((Some(i as i64 + 1), format!("{e}")));
                        }
                    }
                }
            }
        }
        Kind::Json => {
            let head = match first_meaningful_byte(abs) {
                Ok(Some(b)) => b,
                Ok(None) => {
                    out.errors.push((None, "файл пустой".into()));
                    return out;
                }
                Err(e) => {
                    out.errors.push((None, format!("не открывается: {e}")));
                    return out;
                }
            };

            let file = match File::open(abs) {
                Ok(f) => f,
                Err(e) => {
                    out.errors.push((None, format!("не открывается: {e}")));
                    return out;
                }
            };
            let reader = BufReader::with_capacity(1 << 20, file);

            if head == b'[' {
                // Массив верхнего уровня — тянем поэлементно, весь файл в память не грузим.
                interner.bump(root_id, M_ARR, true, false);
                let mut records: u64 = 0;
                let mut values: u64 = 0;
                let mut fail: Option<String> = None;
                {
                    let cb = |v: Value| -> Result<(), String> {
                        let mut path = String::from("$[]");
                        let rec = records as i64;
                        walk(
                            &v,
                            &mut path,
                            Some(root_id),
                            "[]",
                            1,
                            interner,
                            stmt,
                            file_id,
                            rec,
                            &mut values,
                        )
                        .map_err(|e| e.to_string())?;
                        records += 1;
                        Ok(())
                    };
                    let mut de = serde_json::Deserializer::from_reader(reader);
                    use serde::Deserializer as _;
                    if let Err(e) = (&mut de).deserialize_seq(SeqSink { f: cb }) {
                        fail = Some(e.to_string());
                    }
                }
                out.records = records;
                out.values = values;
                if let Some(e) = fail {
                    out.errors.push((None, e));
                }
            } else {
                // Одно значение (или несколько подряд) — читаем потоком значений.
                let de = serde_json::Deserializer::from_reader(reader);
                for item in de.into_iter::<Value>() {
                    match item {
                        Ok(v) => {
                            let mut path = String::from("$");
                            let rec = out.records as i64;
                            if let Err(e) = walk(
                                &v,
                                &mut path,
                                None,
                                "$",
                                0,
                                interner,
                                stmt,
                                file_id,
                                rec,
                                &mut out.values,
                            ) {
                                out.errors.push((None, e.to_string()));
                                break;
                            }
                            out.records += 1;
                        }
                        Err(e) => {
                            if out.errors.len() < MAX_ERRORS_PER_FILE {
                                out.errors.push((Some(e.line() as i64), format!("{e}")));
                            }
                            break;
                        }
                    }
                }
            }
        }
    }

    out
}

pub fn collect_files(root: &Path) -> Vec<(PathBuf, Kind)> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // .git пропускаем: там лежат служебные json, к данным не относящиеся.
            !(e.file_type().is_dir() && e.file_name() == ".git")
        })
        .flatten()
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        let kind = match ext.as_deref() {
            Some("json") => Kind::Json,
            Some("jsonl") | Some("ndjson") => Kind::Jsonl,
            _ => continue,
        };
        files.push((p.to_path_buf(), kind));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

pub fn scan<F: FnMut(ScanProgress)>(
    root: &Path,
    conn: &Connection,
    mut on_progress: F,
) -> rusqlite::Result<ScanSummary> {
    let started = std::time::Instant::now();
    let files = collect_files(root);
    let files_total = files.len() as u64;

    let mut summary = ScanSummary {
        root: root.to_string_lossy().to_string(),
        ..Default::default()
    };

    let mut interner = Interner::new();
    let root_id = 1i64;

    conn.execute_batch("BEGIN")?;
    let mut val_stmt = conn.prepare(
        "INSERT INTO vals (path_id, file_id, rec, vtype, txt, num, trunc) VALUES (?1,?2,?3,?4,?5,?6,?7)",
    )?;
    let mut file_stmt = conn.prepare(
        "INSERT INTO files (id, abs_path, rel_path, kind, bytes, records) VALUES (?1,?2,?3,?4,?5,?6)",
    )?;
    let mut err_stmt =
        conn.prepare("INSERT INTO errors (file, line, message) VALUES (?1,?2,?3)")?;

    let mut since_commit: u64 = 0;

    for (i, (abs, kind)) in files.iter().enumerate() {
        let file_id = i as i64 + 1;
        let rel = abs
            .strip_prefix(root)
            .unwrap_or(abs)
            .to_string_lossy()
            .to_string();
        let bytes = std::fs::metadata(abs).map(|m| m.len()).unwrap_or(0);

        let outcome = scan_one_file(abs, *kind, file_id, root_id, &mut interner, &mut val_stmt);

        file_stmt.execute(params![
            file_id,
            abs.to_string_lossy().to_string(),
            rel,
            kind.as_str(),
            bytes as i64,
            outcome.records as i64
        ])?;

        for (line, msg) in &outcome.errors {
            err_stmt.execute(params![rel, line, msg])?;
        }
        if !outcome.errors.is_empty() {
            summary.files_failed += 1;
        }

        summary.files_scanned += 1;
        summary.records += outcome.records;
        summary.values += outcome.values;
        summary.bytes += bytes;
        since_commit += outcome.values;

        if since_commit > 250_000 {
            conn.execute_batch("COMMIT; BEGIN;")?;
            since_commit = 0;
        }

        on_progress(ScanProgress {
            files_done: summary.files_scanned,
            files_total,
            records: summary.records,
            current: rel,
        });
    }

    // Пути и счётчики пишем одним проходом в конце.
    {
        let mut path_stmt = conn.prepare(
            "INSERT INTO paths (id, path, parent_id, key, depth, is_array, cnt, nonempty, mask) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        )?;
        for (idx, m) in interner.meta.iter().enumerate() {
            path_stmt.execute(params![
                idx as i64 + 1,
                m.path,
                m.parent,
                m.key,
                m.depth,
                if m.is_array { 1 } else { 0 },
                m.cnt,
                m.nonempty,
                m.mask
            ])?;
        }
    }

    drop(val_stmt);
    drop(file_stmt);
    drop(err_stmt);
    conn.execute_batch("COMMIT")?;

    build_indexes(conn)?;

    summary.keys = interner.meta.len() as u64;
    summary.elapsed_ms = started.elapsed().as_millis() as u64;

    conn.execute(
        "INSERT OR REPLACE INTO meta (k, v) VALUES ('summary', ?1)",
        params![serde_json::to_string(&summary).unwrap_or_default()],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO meta (k, v) VALUES ('root', ?1)",
        params![summary.root],
    )?;

    Ok(summary)
}
