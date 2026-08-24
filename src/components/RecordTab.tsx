import { useEffect, useState } from "react";
import { getRecord, getRecordIndex, getRecords } from "../api";
import { fmtInt } from "../format";
import type { RecordRef, RecordView, TreeNode } from "../types";

interface Props {
  node: TreeNode | null;
  /** Прыжок из таблицы значений: конкретная запись, к которой надо встать. */
  target: { fileId: number; rec: number } | null;
  onTargetConsumed: () => void;
}

export function RecordTab({ node, target, onTargetConsumed }: Props) {
  const [index, setIndex] = useState(0);
  const [total, setTotal] = useState(0);
  const [ref, setRef] = useState<RecordRef | null>(null);
  const [view, setView] = useState<RecordView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const pathIds = node?.pathIds ?? [];
  const key = pathIds.join(",");

  // Смена ключа — заново считаем, сколько записей его содержат.
  useEffect(() => {
    let alive = true;
    getRecords(pathIds, 0, 1)
      .then((p) => {
        if (!alive) return;
        setTotal(p.total);
        setIndex(0);
      })
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  // Прыжок из таблицы: переводим (file, rec) в порядковый номер внутри выборки.
  useEffect(() => {
    if (!target) return;
    let alive = true;
    getRecordIndex(pathIds, target.fileId, target.rec)
      .then((i) => {
        if (!alive) return;
        setIndex(Math.max(0, i));
        onTargetConsumed();
      })
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target]);

  useEffect(() => {
    if (total === 0) {
      setView(null);
      setRef(null);
      return;
    }
    let alive = true;
    setBusy(true);
    setError(null);
    getRecords(pathIds, index, 1)
      .then((p) => {
        const r = p.rows[0];
        if (!r) throw new Error("запись не найдена");
        if (alive) setRef(r);
        return getRecord(r.fileId, r.rec);
      })
      .then((v) => alive && setView(v))
      .catch((e) => alive && setError(String(e)))
      .finally(() => alive && setBusy(false));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, index, total]);

  // Стрелки листают записи, пока фокус не в поле ввода.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null;
      const tag = el?.tagName;
      if (tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA") return;
      // В дереве стрелки свои: там они двигают выделение и раскрывают узлы.
      if (el?.closest(".tree-root")) return;
      if (e.key === "ArrowLeft") setIndex((i) => Math.max(0, i - 1));
      if (e.key === "ArrowRight") setIndex((i) => Math.min(total - 1, i + 1));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [total]);

  if (!node) return <div className="empty">Выбери ключ в дереве слева.</div>;
  if (total === 0 && !busy) {
    return <div className="empty">Записей с этим ключом нет.</div>;
  }

  const text = view ? JSON.stringify(view.json, null, 2) : "";
  const needle = node.key.replace(/\[\]$/, "");

  return (
    <div className="record">
      <div className="record-toolbar">
        <button className="btn" disabled={index === 0 || busy} onClick={() => setIndex(index - 1)}>
          ← пред.
        </button>
        <input
          className="input input-num"
          type="number"
          min={1}
          max={total}
          value={index + 1}
          onChange={(e) => {
            const v = Number(e.target.value) - 1;
            if (!Number.isNaN(v)) setIndex(Math.min(Math.max(0, v), total - 1));
          }}
        />
        <span className="record-count">из {fmtInt(total)}</span>
        <button
          className="btn"
          disabled={index >= total - 1 || busy}
          onClick={() => setIndex(index + 1)}
        >
          след. →
        </button>
        {ref && (
          <span className="record-src" title={ref.file}>
            {ref.file} · запись {fmtInt(ref.rec)}
          </span>
        )}
      </div>

      {error && <div className="error-box">{error}</div>}

      <pre
        className="record-json"
        tabIndex={0}
        role="region"
        aria-label="Полная JSON-запись"
      >
        {text.split("\n").map((line, i) => (
          <div
            key={i}
            className={
              "json-line" + (line.trimStart().startsWith(`"${needle}"`) ? " is-hit" : "")
            }
          >
            {line}
          </div>
        ))}
      </pre>
    </div>
  );
}
