//! Чтение из индекса: страницы значений, список записей, одна запись целиком.

use rusqlite::{params, types::Value as SqlValue, Connection};
use serde_json::Value;
use std::io::{BufRead, BufReader};

use crate::db::type_name;
use crate::model::*;
use crate::scan::Kind;

fn placeholders(n: usize) -> String {
    (0..n).map(|_| "?").collect::<Vec<_>>().join(",")
}

fn order_clause(sort: &str, order: &str) -> String {
    let dir = if order.eq_ignore_ascii_case("desc") {
        "DESC"
    } else {
        "ASC"
    };
    match sort {
        "file" => format!("f.rel_path {dir}, v.rec {dir}"),
        "value" => format!("(v.num IS NULL) {dir}, v.num {dir}, v.txt {dir}"),
        "type" => format!("v.vtype {dir}, v.txt {dir}"),
        "path" => format!("p.path {dir}, v.file_id {dir}, v.rec {dir}"),
        _ => format!("v.file_id {dir}, v.rec {dir}"),
    }
}

pub fn values(conn: &Connection, q: &ValueQuery) -> rusqlite::Result<ValuePage> {
    if q.path_ids.is_empty() {
        return Ok(ValuePage {
            total: 0,
            offset: q.offset,
            rows: Vec::new(),
        });
    }

    let mut where_sql = format!("v.path_id IN ({})", placeholders(q.path_ids.len()));
    let mut args: Vec<SqlValue> = q.path_ids.iter().map(|i| SqlValue::from(*i)).collect();

    if let Some(fid) = q.file_id {
        where_sql.push_str(" AND v.file_id = ?");
        args.push(SqlValue::from(fid));
    }
    let filter = q.filter.as_deref().unwrap_or("").trim().to_string();
    if !filter.is_empty() {
        where_sql.push_str(" AND v.txt LIKE ? ESCAPE '\\'");
        let escaped = filter
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        args.push(SqlValue::from(format!("%{escaped}%")));
    }

    let total: i64 = {
        let sql = format!(
            "SELECT COUNT(*) FROM vals v JOIN files f ON f.id = v.file_id JOIN paths p ON p.id = v.path_id WHERE {where_sql}"
        );
        conn.query_row(&sql, rusqlite::params_from_iter(args.iter()), |r| r.get(0))?
    };

    let sql = format!(
        "SELECT v.file_id, f.rel_path, v.rec, p.path, v.vtype, v.txt, v.trunc \
         FROM vals v JOIN files f ON f.id = v.file_id JOIN paths p ON p.id = v.path_id \
         WHERE {where_sql} ORDER BY {} LIMIT ? OFFSET ?",
        order_clause(&q.sort, &q.order)
    );
    let mut page_args = args.clone();
    page_args.push(SqlValue::from(q.limit.clamp(1, 5000)));
    page_args.push(SqlValue::from(q.offset.max(0)));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(page_args.iter()), |r| {
            let vtype: i64 = r.get(4)?;
            Ok(ValueRow {
                file_id: r.get(0)?,
                file: r.get(1)?,
                rec: r.get(2)?,
                path: r.get(3)?,
                vtype: type_name(vtype).to_string(),
                value: r.get(5)?,
                truncated: r.get::<_, i64>(6)? != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(ValuePage {
        total,
        offset: q.offset,
        rows,
    })
}

pub fn records(
    conn: &Connection,
    path_ids: &[i64],
    offset: i64,
    limit: i64,
) -> rusqlite::Result<RecordPage> {
    let (where_sql, args): (String, Vec<SqlValue>) = if path_ids.is_empty() {
        ("1=1".to_string(), Vec::new())
    } else {
        (
            format!("v.path_id IN ({})", placeholders(path_ids.len())),
            path_ids.iter().map(|i| SqlValue::from(*i)).collect(),
        )
    };

    let total: i64 = {
        let sql = format!(
            "SELECT COUNT(*) FROM (SELECT DISTINCT v.file_id, v.rec FROM vals v WHERE {where_sql})"
        );
        conn.query_row(&sql, rusqlite::params_from_iter(args.iter()), |r| r.get(0))?
    };

    let sql = format!(
        "SELECT DISTINCT v.file_id, f.rel_path, v.rec FROM vals v \
         JOIN files f ON f.id = v.file_id WHERE {where_sql} \
         ORDER BY v.file_id, v.rec LIMIT ? OFFSET ?"
    );
    let mut page_args = args.clone();
    page_args.push(SqlValue::from(limit.clamp(1, 5000)));
    page_args.push(SqlValue::from(offset.max(0)));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(page_args.iter()), |r| {
            Ok(RecordRef {
                file_id: r.get(0)?,
                file: r.get(1)?,
                rec: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(RecordPage {
        total,
        offset,
        rows,
    })
}

/// Порядковый номер записи внутри выборки — нужен, чтобы прыжок из таблицы
/// значений вставал на правильную позицию листалки.
pub fn record_index(
    conn: &Connection,
    path_ids: &[i64],
    file_id: i64,
    rec: i64,
) -> rusqlite::Result<i64> {
    let (where_sql, mut args): (String, Vec<SqlValue>) = if path_ids.is_empty() {
        ("1=1".to_string(), Vec::new())
    } else {
        (
            format!("v.path_id IN ({})", placeholders(path_ids.len())),
            path_ids.iter().map(|i| SqlValue::from(*i)).collect(),
        )
    };
    args.push(SqlValue::from(file_id));
    args.push(SqlValue::from(file_id));
    args.push(SqlValue::from(rec));

    let sql = format!(
        "SELECT COUNT(*) FROM (SELECT DISTINCT v.file_id, v.rec FROM vals v \
         WHERE {where_sql} AND (v.file_id < ? OR (v.file_id = ? AND v.rec < ?)))"
    );
    conn.query_row(&sql, rusqlite::params_from_iter(args.iter()), |r| r.get(0))
}

/// Запись читаем из исходного файла, а не собираем из индекса: так она точная,
/// без обрезанных строк и без догадок о порядке элементов массива.
pub fn record(conn: &Connection, file_id: i64, rec: i64) -> Result<RecordView, String> {
    let (abs, rel, kind): (String, String, String) = conn
        .query_row(
            "SELECT abs_path, rel_path, kind FROM files WHERE id = ?1",
            params![file_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;

    let kind = if kind == "jsonl" {
        Kind::Jsonl
    } else {
        Kind::Json
    };
    let json = read_nth_record(&abs, kind, rec)?;

    Ok(RecordView {
        file_id,
        file: rel,
        rec,
        json,
        truncated: false,
    })
}

struct TakeNth<F: FnMut(Value) -> bool> {
    f: F,
}

impl<'de, F: FnMut(Value) -> bool> serde::de::Visitor<'de> for TakeNth<F> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("массив JSON верхнего уровня")
    }

    fn visit_seq<A>(mut self, mut seq: A) -> Result<(), A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        while let Some(v) = seq.next_element::<Value>()? {
            // Нашли нужный элемент — обрываем разбор, дальше файл не читаем.
            if (self.f)(v) {
                return Err(serde::de::Error::custom("__stop__"));
            }
        }
        Ok(())
    }
}

fn read_nth_record(abs: &str, kind: Kind, rec: i64) -> Result<Value, String> {
    let file = std::fs::File::open(abs).map_err(|e| format!("не открывается: {e}"))?;
    let reader = BufReader::with_capacity(1 << 20, file);

    match kind {
        Kind::Jsonl => {
            let mut idx = 0i64;
            for line in reader.lines() {
                let line = line.map_err(|e| e.to_string())?;
                let t = line.trim_start_matches('\u{feff}').trim();
                if t.is_empty() {
                    continue;
                }
                if serde_json::from_str::<Value>(t).is_err() {
                    continue;
                }
                if idx == rec {
                    return serde_json::from_str::<Value>(t).map_err(|e| e.to_string());
                }
                idx += 1;
            }
            Err(format!("запись {rec} не найдена"))
        }
        Kind::Json => {
            let mut found: Option<Value> = None;
            let mut idx = 0i64;
            {
                let cb = |v: Value| -> bool {
                    if idx == rec {
                        found = Some(v);
                        return true;
                    }
                    idx += 1;
                    false
                };
                let mut de = serde_json::Deserializer::from_reader(reader);
                use serde::Deserializer as _;
                let _ = (&mut de).deserialize_any(TakeNth { f: cb });
            }
            match found {
                Some(v) => Ok(v),
                None => {
                    // Не массив верхнего уровня — читаем поток значений.
                    let file =
                        std::fs::File::open(abs).map_err(|e| format!("не открывается: {e}"))?;
                    let de = serde_json::Deserializer::from_reader(BufReader::new(file));
                    for (i, item) in de.into_iter::<Value>().enumerate() {
                        let v = item.map_err(|e| e.to_string())?;
                        if i as i64 == rec {
                            return Ok(v);
                        }
                    }
                    Err(format!("запись {rec} не найдена"))
                }
            }
        }
    }
}

pub fn errors(conn: &Connection) -> rusqlite::Result<Vec<ErrorRow>> {
    let mut stmt =
        conn.prepare("SELECT file, line, message FROM errors ORDER BY id LIMIT 5000")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ErrorRow {
                file: r.get(0)?,
                line: r.get(1)?,
                message: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn files(conn: &Connection) -> rusqlite::Result<Vec<(i64, String, i64)>> {
    let mut stmt = conn.prepare("SELECT id, rel_path, records FROM files ORDER BY rel_path")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn summary(conn: &Connection) -> Option<ScanSummary> {
    let s: String = conn
        .query_row("SELECT v FROM meta WHERE k = 'summary'", [], |r| r.get(0))
        .ok()?;
    serde_json::from_str(&s).ok()
}
