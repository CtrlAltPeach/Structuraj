//! Типы, которые уходят во фронтенд. Это же — контракт для UI.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub root: String,
    pub files_scanned: u64,
    pub files_failed: u64,
    pub records: u64,
    pub keys: u64,
    pub values: u64,
    pub bytes: u64,
    pub elapsed_ms: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub files_done: u64,
    pub files_total: u64,
    pub records: u64,
    pub current: String,
}

/// Узел дерева структуры. `pathIds` — все канонические пути, слитые в этот узел
/// (в режиме `byName` их может быть несколько).
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    pub id: String,
    pub key: String,
    pub path: String,
    pub is_array: bool,
    pub is_leaf: bool,
    pub count: i64,
    /// Сколько значений в поддереве непустые. 0 — ключ есть, но всегда пустой
    /// (null, "", {} или []). По этому полю UI прячет пустые ключи.
    pub non_empty: i64,
    pub types: Vec<String>,
    pub path_ids: Vec<i64>,
    pub paths: Vec<String>,
    pub children: Vec<TreeNode>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ValueRow {
    pub file_id: i64,
    pub file: String,
    pub rec: i64,
    pub path: String,
    pub vtype: String,
    pub value: Option<String>,
    pub truncated: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ValuePage {
    pub total: i64,
    pub offset: i64,
    pub rows: Vec<ValueRow>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ErrorRow {
    pub file: String,
    pub line: Option<i64>,
    pub message: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RecordRef {
    pub file_id: i64,
    pub file: String,
    pub rec: i64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RecordPage {
    pub total: i64,
    pub offset: i64,
    pub rows: Vec<RecordRef>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RecordView {
    pub file_id: i64,
    pub file: String,
    pub rec: i64,
    pub json: serde_json::Value,
    pub truncated: bool,
}

/// Параметры запроса значений. Приходят из UI одним объектом.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ValueQuery {
    pub path_ids: Vec<i64>,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub file_id: Option<i64>,
    /// `file` | `rec` | `value` | `type` | `path`
    #[serde(default = "default_sort")]
    pub sort: String,
    /// `asc` | `desc`
    #[serde(default = "default_order")]
    pub order: String,
    #[serde(default)]
    pub offset: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_sort() -> String {
    "rec".to_string()
}
fn default_order() -> String {
    "asc".to_string()
}
fn default_limit() -> i64 {
    200
}
