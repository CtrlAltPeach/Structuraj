// Зеркало Rust-моделей из src-tauri/src/model.rs.
// Менять только вместе с ними.

export type TreeMode = "byName" | "byPath";
export type SortKey = "file" | "rec" | "value" | "type" | "path";
export type SortOrder = "asc" | "desc";

export interface TreeNode {
  id: string;
  key: string;
  path: string;
  isArray: boolean;
  isLeaf: boolean;
  count: number;
  /** Непустые значения во всём поддереве. 0 — ключ всегда null, "", [] или {}. */
  nonEmpty: number;
  types: string[];
  /** Все канонические пути, слитые в этот узел. В режиме byName их может быть много. */
  pathIds: number[];
  paths: string[];
  children: TreeNode[];
}

export interface ScanSummary {
  root: string;
  filesScanned: number;
  filesFailed: number;
  records: number;
  keys: number;
  values: number;
  bytes: number;
  elapsedMs: number;
}

export interface ScanProgress {
  filesDone: number;
  filesTotal: number;
  records: number;
  current: string;
}

export interface ValueRow {
  fileId: number;
  file: string;
  rec: number;
  path: string;
  vtype: "null" | "bool" | "number" | "string" | "object" | "array" | "?";
  value: string | null;
  truncated: boolean;
}

export interface ValuePage {
  total: number;
  offset: number;
  rows: ValueRow[];
}

export interface RecordRef {
  fileId: number;
  file: string;
  rec: number;
}

export interface RecordPage {
  total: number;
  offset: number;
  rows: RecordRef[];
}

export interface RecordView {
  fileId: number;
  file: string;
  rec: number;
  json: unknown;
  truncated: boolean;
}

export interface ErrorRow {
  file: string;
  line: number | null;
  message: string;
}

export interface FileRow {
  id: number;
  path: string;
  records: number;
}

export interface ValueQuery {
  pathIds: number[];
  filter?: string | null;
  fileId?: number | null;
  sort: SortKey;
  order: SortOrder;
  offset: number;
  limit: number;
}
