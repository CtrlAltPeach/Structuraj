import { useEffect, useMemo, useRef, useState } from "react";
import type {
  KeyboardEvent as ReactKeyboardEvent,
  PointerEvent as ReactPointerEvent,
} from "react";
import { getValues } from "../api";
import { fmtInt } from "../format";
import type { FileRow, SortKey, SortOrder, TreeNode, ValuePage } from "../types";

const PAGE = 200;
const COLUMN_STORAGE_KEY = "structuraj.values-column-widths.v1";

type ColumnKey = SortKey | "actions";

const DEFAULT_COLUMN_WIDTHS: Record<ColumnKey, number> = {
  file: 140,
  rec: 78,
  path: 180,
  type: 80,
  value: 340,
  actions: 64,
};

interface Props {
  node: TreeNode | null;
  files: FileRow[];
  onOpenRecord: (fileId: number, rec: number) => void;
}

const COLUMNS: { key: SortKey; label: string }[] = [
  { key: "file", label: "Файл" },
  { key: "rec", label: "№ записи" },
  { key: "path", label: "Путь" },
  { key: "type", label: "Тип" },
  { key: "value", label: "Значение" },
];

function loadColumnWidths(): Record<ColumnKey, number> {
  try {
    const saved = JSON.parse(localStorage.getItem(COLUMN_STORAGE_KEY) ?? "{}") as Partial<
      Record<ColumnKey, number>
    >;
    return Object.fromEntries(
      (Object.keys(DEFAULT_COLUMN_WIDTHS) as ColumnKey[]).map((key) => [
        key,
        Number.isFinite(saved[key])
          ? Math.max(0, Number(saved[key]))
          : DEFAULT_COLUMN_WIDTHS[key],
      ]),
    ) as Record<ColumnKey, number>;
  } catch {
    return { ...DEFAULT_COLUMN_WIDTHS };
  }
}

export function ValuesTab({ node, files, onOpenRecord }: Props) {
  const [sort, setSort] = useState<SortKey>("rec");
  const [order, setOrder] = useState<SortOrder>("asc");
  const [offset, setOffset] = useState(0);
  const [rawFilter, setRawFilter] = useState("");
  const [filter, setFilter] = useState("");
  const [fileId, setFileId] = useState<number | null>(null);
  const [page, setPage] = useState<ValuePage | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [columnWidths, setColumnWidths] = useState(loadColumnWidths);
  const columnResize = useRef<{ key: ColumnKey; x: number; width: number } | null>(null);

  useEffect(() => {
    try {
      localStorage.setItem(COLUMN_STORAGE_KEY, JSON.stringify(columnWidths));
    } catch {
      // В редком окружении без localStorage ресайз всё равно работает до закрытия окна.
    }
  }, [columnWidths]);

  const setColumnWidth = (key: ColumnKey, width: number) =>
    setColumnWidths((current) => ({
      ...current,
      [key]: Math.max(0, Math.round(width)),
    }));

  const startColumnResize = (key: ColumnKey, e: ReactPointerEvent<HTMLSpanElement>) => {
    e.stopPropagation();
    columnResize.current = { key, x: e.clientX, width: columnWidths[key] };
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const resizeColumn = (e: ReactPointerEvent<HTMLSpanElement>) => {
    const start = columnResize.current;
    if (!start || !e.currentTarget.hasPointerCapture(e.pointerId)) return;
    setColumnWidth(start.key, start.width + e.clientX - start.x);
  };

  const stopColumnResize = (e: ReactPointerEvent<HTMLSpanElement>) => {
    columnResize.current = null;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
  };

  const resizeColumnWithKeyboard = (
    key: ColumnKey,
    e: ReactKeyboardEvent<HTMLSpanElement>,
  ) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    e.stopPropagation();
    const step = e.shiftKey ? 40 : 10;
    setColumnWidth(key, columnWidths[key] + (e.key === "ArrowLeft" ? -step : step));
  };

  const columnResizer = (key: ColumnKey, label: string) => (
    <span
      className="column-resizer"
      role="separator"
      aria-label={`Изменить ширину столбца «${label}»`}
      aria-orientation="vertical"
      aria-valuemin={0}
      aria-valuenow={columnWidths[key]}
      tabIndex={0}
      title="Перетащи, чтобы изменить ширину · двойной клик — сбросить"
      onClick={(e) => e.stopPropagation()}
      onPointerDown={(e) => startColumnResize(key, e)}
      onPointerMove={resizeColumn}
      onPointerUp={stopColumnResize}
      onPointerCancel={stopColumnResize}
      onKeyDown={(e) => resizeColumnWithKeyboard(key, e)}
      onDoubleClick={(e) => {
        e.stopPropagation();
        setColumnWidth(key, DEFAULT_COLUMN_WIDTHS[key]);
      }}
    />
  );

  // Фильтр по значению бьёт по всей таблице, поэтому не дёргаем базу на каждую букву.
  useEffect(() => {
    const t = setTimeout(() => setFilter(rawFilter), 250);
    return () => clearTimeout(t);
  }, [rawFilter]);

  // Смена ключа, сортировки или фильтра сбрасывает страницу на первую.
  useEffect(() => setOffset(0), [node?.id, sort, order, filter, fileId]);

  const pathIds = useMemo(() => node?.pathIds ?? [], [node]);

  useEffect(() => {
    if (!node || pathIds.length === 0) {
      setPage(null);
      return;
    }
    let alive = true;
    setBusy(true);
    setError(null);
    getValues({ pathIds, filter, fileId, sort, order, offset, limit: PAGE })
      .then((p) => alive && setPage(p))
      .catch((e) => alive && setError(String(e)))
      .finally(() => alive && setBusy(false));
    return () => {
      alive = false;
    };
  }, [pathIds, filter, fileId, sort, order, offset, node]);

  if (!node) {
    return <div className="empty">Выбери ключ в дереве слева.</div>;
  }
  if (node.isArray) {
    return (
      <div className="empty">
        <b>{node.key}</b> — массив. Массивы значений не показывают, они нужны только
        в структуре. Открой ключ внутри него.
      </div>
    );
  }

  const total = page?.total ?? 0;
  const from = total === 0 ? 0 : offset + 1;
  const to = Math.min(offset + PAGE, total);

  return (
    <div className="values">
      <div className="values-toolbar">
        <input
          className="input"
          placeholder="Фильтр по значению…"
          value={rawFilter}
          onChange={(e) => setRawFilter(e.target.value)}
        />
        <select
          className="input"
          value={fileId ?? ""}
          onChange={(e) => setFileId(e.target.value === "" ? null : Number(e.target.value))}
        >
          <option value="">все файлы ({files.length})</option>
          {files.map((f) => (
            <option key={f.id} value={f.id}>
              {f.path}
            </option>
          ))}
        </select>
        <span className="values-count">
          {busy ? "…" : `${fmtInt(from)}–${fmtInt(to)} из ${fmtInt(total)}`}
        </span>
        <div className="pager">
          <button
            className="btn"
            disabled={offset === 0 || busy}
            onClick={() => setOffset(Math.max(0, offset - PAGE))}
          >
            ← назад
          </button>
          <button
            className="btn"
            disabled={to >= total || busy}
            onClick={() => setOffset(offset + PAGE)}
          >
            вперёд →
          </button>
        </div>
      </div>

      {error && <div className="error-box">{error}</div>}

      <div
        className="table-wrap"
        tabIndex={0}
        role="region"
        aria-label="Значения выбранного ключа"
      >
        <table
          className="table table-values"
          style={{
            width: Object.values(columnWidths).reduce((sum, width) => sum + width, 0),
          }}
        >
          <colgroup>
            {COLUMNS.map((column) => (
              <col key={column.key} style={{ width: columnWidths[column.key] }} />
            ))}
            <col style={{ width: columnWidths.actions }} />
          </colgroup>
          <thead>
            <tr>
              {COLUMNS.map((c) => (
                <th
                  key={c.key}
                  className={"sortable" + (sort === c.key ? " is-sorted" : "")}
                  onClick={() => {
                    if (sort === c.key) setOrder(order === "asc" ? "desc" : "asc");
                    else {
                      setSort(c.key);
                      setOrder("asc");
                    }
                  }}
                >
                  {c.label}
                  {sort === c.key && <span className="caret">{order === "asc" ? "▲" : "▼"}</span>}
                  {columnResizer(c.key, c.label)}
                </th>
              ))}
              <th>{columnResizer("actions", "Действия")}</th>
            </tr>
          </thead>
          <tbody>
            {(page?.rows ?? []).map((r, i) => (
              <tr key={`${r.fileId}-${r.rec}-${r.path}-${i}`}>
                <td className="cell-file" title={r.file}>
                  {r.file}
                </td>
                <td className="cell-num">{fmtInt(r.rec)}</td>
                <td className="cell-path" title={r.path}>
                  {r.path}
                </td>
                <td className="cell-type">{r.vtype}</td>
                <td className="cell-value">
                  {r.vtype === "null" ? (
                    <i className="null">null</i>
                  ) : (
                    <>
                      {r.value}
                      {r.truncated && <span className="trunc" title="строка обрезана">…</span>}
                    </>
                  )}
                </td>
                <td className="cell-actions">
                  <button
                    className="btn btn-ghost"
                    onClick={() => onOpenRecord(r.fileId, r.rec)}
                    title="открыть запись целиком"
                  >
                    запись
                  </button>
                </td>
              </tr>
            ))}
            {!busy && (page?.rows.length ?? 0) === 0 && (
              <tr>
                <td colSpan={6} className="empty">
                  Ничего не найдено.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
