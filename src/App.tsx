import { useCallback, useEffect, useRef, useState } from "react";
import type {
  CSSProperties,
  KeyboardEvent as ReactKeyboardEvent,
  PointerEvent as ReactPointerEvent,
} from "react";
import {
  exportMd,
  getErrors,
  getFiles,
  getSummary,
  getTree,
  onScanProgress,
  pickFolder,
  scanFolder,
} from "./api";
import { ErrorsTab } from "./components/ErrorsTab";
import { RecordTab } from "./components/RecordTab";
import { TreeView } from "./components/TreeView";
import { ValuesTab } from "./components/ValuesTab";
import { fmtBytes, fmtInt, fmtMs } from "./format";
import type {
  ErrorRow,
  FileRow,
  ScanProgress,
  ScanSummary,
  TreeMode,
  TreeNode,
} from "./types";
import "./styles.css";

type Tab = "values" | "record" | "errors";

export default function App() {
  const [mode, setMode] = useState<TreeMode>("byName");
  const [summary, setSummary] = useState<ScanSummary | null>(null);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [tree, setTree] = useState<TreeNode[]>([]);
  const [files, setFiles] = useState<FileRow[]>([]);
  const [errors, setErrors] = useState<ErrorRow[]>([]);
  const [selected, setSelected] = useState<TreeNode | null>(null);
  const [treeFilter, setTreeFilter] = useState("");
  const [hideEmpty, setHideEmpty] = useState(false);
  const [tab, setTab] = useState<Tab>("values");
  const [recordTarget, setRecordTarget] = useState<{ fileId: number; rec: number } | null>(
    null,
  );
  const [note, setNote] = useState<string | null>(null);
  const [fail, setFail] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const [treeWidth, setTreeWidth] = useState<number | null>(null);
  const resizeStart = useRef({ x: 0, width: 0 });
  const treePanelRef = useRef<HTMLElement>(null);
  const valuesPanelRef = useRef<HTMLElement>(null);

  useEffect(() => {
    const focusPanel = (panel: HTMLElement | null, targetSelectors: string[]) => {
      if (!panel) return;
      const target = targetSelectors
        .map((selector) => panel.querySelector<HTMLElement>(selector))
        .find((element) => element !== null);
      (target ?? panel).focus({ preventScroll: true });
    };

    const switchPanelFocus = (e: KeyboardEvent) => {
      if (e.key !== "F6") return;
      e.preventDefault();
      const active = document.activeElement;
      const inTree = active instanceof Node && treePanelRef.current?.contains(active);
      const inValues = active instanceof Node && valuesPanelRef.current?.contains(active);

      if (inTree || (!inValues && e.shiftKey)) {
        focusPanel(valuesPanelRef.current, [".table-wrap", ".record-json", ".panel-body"]);
      } else {
        focusPanel(treePanelRef.current, [".tree-root", ".panel-body"]);
      }
    };

    window.addEventListener("keydown", switchPanelFocus);
    return () => window.removeEventListener("keydown", switchPanelFocus);
  }, []);

  const clampTreeWidth = (splitWidth: number, width: number) =>
    Math.min(Math.max(220, width), Math.max(220, splitWidth - 480));

  const startResize = (e: ReactPointerEvent<HTMLDivElement>) => {
    const treePanel = e.currentTarget.previousElementSibling;
    if (!(treePanel instanceof HTMLElement)) return;
    resizeStart.current = { x: e.clientX, width: treePanel.getBoundingClientRect().width };
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const resizePanels = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
    const splitWidth = e.currentTarget.parentElement?.getBoundingClientRect().width ?? 0;
    setTreeWidth(
      clampTreeWidth(splitWidth, resizeStart.current.width + e.clientX - resizeStart.current.x),
    );
  };

  const stopResize = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
  };

  const resizeWithKeyboard = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    const treePanel = e.currentTarget.previousElementSibling;
    const splitWidth = e.currentTarget.parentElement?.getBoundingClientRect().width ?? 0;
    const current =
      treeWidth ?? (treePanel instanceof HTMLElement ? treePanel.getBoundingClientRect().width : 0);
    const step = e.shiftKey ? 48 : 16;
    setTreeWidth(clampTreeWidth(splitWidth, current + (e.key === "ArrowLeft" ? -step : step)));
  };

  useEffect(() => {
    const un = onScanProgress(setProgress);
    return () => {
      un.then((f) => f());
    };
  }, []);

  const reload = useCallback(
    async (m: TreeMode) => {
      const [t, f, e, s] = await Promise.all([
        getTree(m),
        getFiles(),
        getErrors(),
        getSummary(),
      ]);
      setTree(t);
      setFiles(f);
      setErrors(e);
      if (s) setSummary(s);
      // Выделение по id переживает смену режима не всегда — сбрасываем честно.
      setSelected(null);
    },
    [],
  );

  const chooseFolder = async () => {
    setFail(null);
    const dir = await pickFolder();
    if (!dir) return;
    setScanning(true);
    setProgress(null);
    setTree([]);
    setSelected(null);
    try {
      const s = await scanFolder(dir);
      setSummary(s);
      await reload(mode);
      setNote(
        `Просканировано: ${fmtInt(s.filesScanned)} файлов, ${fmtInt(s.records)} записей за ${fmtMs(s.elapsedMs)}`,
      );
    } catch (e) {
      setFail(String(e));
    } finally {
      setScanning(false);
      setProgress(null);
    }
  };

  const switchMode = async (m: TreeMode) => {
    setMode(m);
    if (summary) await reload(m);
  };

  const doExport = async () => {
    try {
      const path = await exportMd(mode);
      setNote(`Структура записана: ${path}`);
    } catch (e) {
      setFail(String(e));
    }
  };

  useEffect(() => {
    if (!note) return;
    const t = setTimeout(() => setNote(null), 6000);
    return () => clearTimeout(t);
  }, [note]);

  return (
    <div className="app">
      <header className="topbar">
        <button className="btn btn-primary" onClick={chooseFolder} disabled={scanning}>
          {scanning ? "Сканирую…" : "Выбрать папку"}
        </button>

        <div className="topbar-path" title={summary?.root ?? ""}>
          {summary?.root ?? "папка не выбрана"}
        </div>

        {summary && (
          <div className="topbar-stats">
            <span>
              файлов <b>{fmtInt(summary.filesScanned)}</b>
            </span>
            <span>
              записей <b>{fmtInt(summary.records)}</b>
            </span>
            <span>
              ключей <b>{fmtInt(summary.keys)}</b>
            </span>
            <span>
              значений <b>{fmtInt(summary.values)}</b>
            </span>
            <span>{fmtBytes(summary.bytes)}</span>
          </div>
        )}

        <div className="seg">
          <button
            className={"seg-btn" + (mode === "byName" ? " is-on" : "")}
            onClick={() => switchMode("byName")}
            title="Все одноимённые ключи — один узел"
          >
            по имени
          </button>
          <button
            className={"seg-btn" + (mode === "byPath" ? " is-on" : "")}
            onClick={() => switchMode("byPath")}
            title="user.name и order.name — разные узлы"
          >
            по пути
          </button>
        </div>

        <button className="btn" onClick={doExport} disabled={!summary}>
          Экспорт STRUCTURE.md
        </button>
      </header>

      {scanning && (
        <div className="progress">
          <div
            className="progress-bar"
            style={{
              width: progress
                ? `${Math.round((progress.filesDone / Math.max(1, progress.filesTotal)) * 100)}%`
                : "2%",
            }}
          />
          <span className="progress-text">
            {progress
              ? `${fmtInt(progress.filesDone)} / ${fmtInt(progress.filesTotal)} · ${fmtInt(progress.records)} записей · ${progress.current}`
              : "обхожу папку…"}
          </span>
        </div>
      )}

      {fail && <div className="error-box error-top">{fail}</div>}
      {note && <div className="note">{note}</div>}

      <main
        className="split"
        style={
          treeWidth === null
            ? undefined
            : ({ "--tree-width": `${treeWidth}px` } as CSSProperties)
        }
      >
        <section
          className="panel panel-tree"
          ref={treePanelRef}
          tabIndex={-1}
          aria-label="Панель структуры ключей"
        >
          <div className="panel-head">
            <input
              className="input"
              placeholder="Поиск ключа…"
              value={treeFilter}
              onChange={(e) => setTreeFilter(e.target.value)}
            />
            <label
              className="check"
              title="Скрыть ключи, у которых во всех записях null, пустая строка, [] или {}"
            >
              <input
                type="checkbox"
                checked={hideEmpty}
                onChange={(e) => setHideEmpty(e.target.checked)}
              />
              скрыть пустые
            </label>
          </div>
          <div className="panel-body" tabIndex={-1}>
            {tree.length === 0 ? (
              <div className="empty">
                Нажми «Выбрать папку». Приложение обойдёт все вложенные .json, .jsonl
                и .ndjson и соберёт общую структуру.
              </div>
            ) : (
              <TreeView
                nodes={tree}
                selectedId={selected?.id ?? null}
                onSelect={setSelected}
                filter={treeFilter}
                hideEmpty={hideEmpty}
              />
            )}
          </div>
        </section>

        <div
          className="splitter"
          role="separator"
          aria-label="Изменить ширину дерева"
          aria-orientation="vertical"
          aria-valuemin={220}
          aria-valuenow={treeWidth ?? undefined}
          tabIndex={0}
          title="Перетащи, чтобы изменить ширину · двойной клик — сбросить"
          onPointerDown={startResize}
          onPointerMove={resizePanels}
          onPointerUp={stopResize}
          onPointerCancel={stopResize}
          onKeyDown={resizeWithKeyboard}
          onDoubleClick={() => setTreeWidth(null)}
        />

        <section
          className="panel panel-values"
          ref={valuesPanelRef}
          tabIndex={-1}
          aria-label="Панель данных"
        >
          <div className="panel-head tabs">
            <button
              className={"tab" + (tab === "values" ? " is-on" : "")}
              onClick={() => setTab("values")}
            >
              Все вхождения
            </button>
            <button
              className={"tab" + (tab === "record" ? " is-on" : "")}
              onClick={() => setTab("record")}
            >
              Запись целиком
            </button>
            <button
              className={"tab" + (tab === "errors" ? " is-on" : "")}
              onClick={() => setTab("errors")}
            >
              Ошибки{errors.length > 0 ? ` (${fmtInt(errors.length)})` : ""}
            </button>

            {selected && (
              <div className="panel-sub" title={selected.paths.join("\n")}>
                <b>{selected.key}</b> · {selected.types.join(" | ") || "объект"} ·{" "}
                {fmtInt(selected.count)}
              </div>
            )}
          </div>

          <div className="panel-body" tabIndex={-1}>
            {tab === "values" && (
              <ValuesTab
                node={selected}
                files={files}
                onOpenRecord={(fileId, rec) => {
                  setRecordTarget({ fileId, rec });
                  setTab("record");
                }}
              />
            )}
            {tab === "record" && (
              <RecordTab
                node={selected}
                target={recordTarget}
                onTargetConsumed={() => setRecordTarget(null)}
              />
            )}
            {tab === "errors" && <ErrorsTab rows={errors} />}
          </div>
        </section>
      </main>

      <footer className="statusbar">
        <span>Structuraj</span>
        <span className="statusbar-hint">
          <kbd>F6</kbd> дерево ↔ значения
        </span>
      </footer>
    </div>
  );
}
