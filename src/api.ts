// Единственное место, где фронтенд разговаривает с Rust.
// Это и есть контракт UI: любая вёрстка работает поверх этих девяти функций.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ErrorRow,
  FileRow,
  RecordPage,
  RecordView,
  ScanProgress,
  ScanSummary,
  TreeMode,
  TreeNode,
  ValuePage,
  ValueQuery,
} from "./types";

/** Системный диалог выбора папки. `null`, если пользователь отменил. */
export const pickFolder = (): Promise<string | null> =>
  invoke<string | null>("pick_folder");

/** Полный пересбор индекса. Долгая операция — слушай `onScanProgress`. */
export const scanFolder = (path: string): Promise<ScanSummary> =>
  invoke<ScanSummary>("scan_folder", { path });

export const onScanProgress = (
  cb: (p: ScanProgress) => void,
): Promise<UnlistenFn> => listen<ScanProgress>("scan:progress", (e) => cb(e.payload));

export const getTree = (mode: TreeMode): Promise<TreeNode[]> =>
  invoke<TreeNode[]>("get_tree", { mode });

export const getValues = (query: ValueQuery): Promise<ValuePage> =>
  invoke<ValuePage>("get_values", { query });

export const getRecords = (
  pathIds: number[],
  offset: number,
  limit: number,
): Promise<RecordPage> =>
  invoke<RecordPage>("get_records", { pathIds, offset, limit });

export const getRecord = (fileId: number, rec: number): Promise<RecordView> =>
  invoke<RecordView>("get_record", { fileId, rec });

/** Порядковый номер записи внутри выборки по этим ключам — для прыжка из таблицы. */
export const getRecordIndex = (
  pathIds: number[],
  fileId: number,
  rec: number,
): Promise<number> => invoke<number>("get_record_index", { pathIds, fileId, rec });

export const getErrors = (): Promise<ErrorRow[]> => invoke<ErrorRow[]>("get_errors");

export const getFiles = (): Promise<FileRow[]> => invoke<FileRow[]>("get_files");

export const getSummary = (): Promise<ScanSummary | null> =>
  invoke<ScanSummary | null>("get_summary");

/** Пишет STRUCTURE.md в целевую папку и возвращает полный путь к файлу. */
export const exportMd = (mode: TreeMode): Promise<string> =>
  invoke<string>("export_md", { mode });
