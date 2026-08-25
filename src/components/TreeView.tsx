import { useEffect, useMemo, useRef, useState } from "react";
import type { TreeNode } from "../types";
import { fmtInt } from "../format";
import { isPureArray, typeLabel } from "../nodes";

interface Props {
  nodes: TreeNode[];
  selectedId: string | null;
  onSelect: (node: TreeNode) => void;
  filter: string;
  /** Прячет ключи, у которых во всём поддереве нет ни одного непустого значения. */
  hideEmpty: boolean;
}

/** Строка развёрнутого дерева — то, по чему бегают стрелки. */
interface Row {
  node: TreeNode;
  depth: number;
  hasKids: boolean;
  open: boolean;
}

function selfMatches(node: TreeNode, needle: string): boolean {
  return (
    node.key.toLowerCase().includes(needle) || node.path.toLowerCase().includes(needle)
  );
}

/** Узел остаётся, если проходит сам или если проходит кто-то из потомков. */
function keep(node: TreeNode, needle: string, hideEmpty: boolean): boolean {
  // nonEmpty — сумма по всему поддереву, так что пустую ветку отсекаем целиком.
  if (hideEmpty && node.nonEmpty === 0) return false;
  if (!needle) return true;
  if (selfMatches(node, needle)) return true;
  return node.children.some((c) => keep(c, needle, hideEmpty));
}

function flatten(
  nodes: TreeNode[],
  expanded: Set<string>,
  needle: string,
  hideEmpty: boolean,
  forceOpen: boolean,
  depth = 0,
  out: Row[] = [],
): Row[] {
  for (const node of nodes) {
    if (!keep(node, needle, hideEmpty)) continue;
    const kids = node.children.filter((c) => keep(c, needle, hideEmpty));
    const hasKids = kids.length > 0;
    const open = hasKids && (forceOpen || expanded.has(node.id));
    out.push({ node, depth, hasKids, open });
    if (open) flatten(kids, expanded, needle, hideEmpty, forceOpen, depth + 1, out);
  }
  return out;
}

function seedExpanded(nodes: TreeNode[], acc = new Set<string>()): Set<string> {
  // После скана сразу показываем всю найденную структуру.
  for (const n of nodes) {
    if (n.children.length === 0) continue;
    acc.add(n.id);
    seedExpanded(n.children, acc);
  }
  return acc;
}

export function TreeView({ nodes, selectedId, onSelect, filter, hideEmpty }: Props) {
  const [expanded, setExpanded] = useState<Set<string>>(() => seedExpanded(nodes));
  const rootRef = useRef<HTMLDivElement>(null);

  // Новое дерево (скан или смена режима) — заново раскрываем все уровни.
  useEffect(() => setExpanded(seedExpanded(nodes)), [nodes]);

  const needle = filter.trim().toLowerCase();
  const rows = useMemo(
    () => flatten(nodes, expanded, needle, hideEmpty, needle.length > 0),
    [nodes, expanded, needle, hideEmpty],
  );

  const cursor = rows.findIndex((r) => r.node.id === selectedId);

  // Выбранная строка всегда должна оставаться в поле зрения.
  useEffect(() => {
    if (!selectedId) return;
    rootRef.current
      ?.querySelector(`[data-node-id="${CSS.escape(selectedId)}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [selectedId, rows.length]);

  const toggle = (id: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (!next.delete(id)) next.add(id);
      return next;
    });

  const moveTo = (i: number) => {
    const row = rows[Math.min(Math.max(i, 0), rows.length - 1)];
    if (row) onSelect(row.node);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (rows.length === 0) return;
    const row = cursor >= 0 ? rows[cursor] : null;

    switch (e.key) {
      case "ArrowDown":
        moveTo(cursor < 0 ? 0 : cursor + 1);
        break;
      case "ArrowUp":
        moveTo(cursor < 0 ? 0 : cursor - 1);
        break;
      case "Home":
        moveTo(0);
        break;
      case "End":
        moveTo(rows.length - 1);
        break;
      case "ArrowRight":
        if (!row) moveTo(0);
        else if (row.hasKids && !row.open) toggle(row.node.id);
        else moveTo(cursor + 1);
        break;
      case "ArrowLeft":
        if (!row) moveTo(0);
        else if (row.hasKids && row.open) toggle(row.node.id);
        else {
          // Прыжок к родителю: ближайшая строка выше с меньшим отступом.
          for (let i = cursor - 1; i >= 0; i--) {
            if (rows[i].depth < row.depth) {
              moveTo(i);
              break;
            }
          }
        }
        break;
      case "Enter":
      case " ":
        if (row?.hasKids) toggle(row.node.id);
        break;
      default:
        return;
    }
    // Иначе браузер прокрутит панель вместо перемещения по дереву.
    e.preventDefault();
  };

  if (rows.length === 0) {
    return (
      <div className="empty">
        {hideEmpty
          ? "Все ключи под фильтром пустые. Сними «скрыть пустые», чтобы их увидеть."
          : "Ничего не найдено."}
      </div>
    );
  }

  return (
    <div
      className="tree-root"
      ref={rootRef}
      tabIndex={0}
      role="tree"
      aria-label="Структура ключей"
      aria-activedescendant={
        cursor >= 0 ? `tree-row-${rows[cursor].node.id}` : undefined
      }
      onFocus={(e) => {
        // F6 должен сразу дать активную строку, а не просто фокус прокрутке панели.
        if (e.target === e.currentTarget && cursor < 0) moveTo(0);
      }}
      onKeyDown={onKeyDown}
    >
      {rows.map((row) => (
        <div
          id={`tree-row-${row.node.id}`}
          key={row.node.id}
          data-node-id={row.node.id}
          role="treeitem"
          aria-selected={row.node.id === selectedId}
          aria-expanded={row.hasKids ? row.open : undefined}
          className={
            "tree-row" +
            (row.node.id === selectedId ? " is-selected" : "") +
            (isPureArray(row.node) ? " is-container" : "")
          }
          style={{ paddingLeft: 3 + row.depth * 14 }}
          onClick={() => {
            onSelect(row.node);
            if (isPureArray(row.node) && row.hasKids) toggle(row.node.id);
            rootRef.current?.focus();
          }}
        >
          <button
            type="button"
            className={"tree-toggle" + (row.hasKids ? "" : " is-empty")}
            onClick={(e) => {
              e.stopPropagation();
              if (row.hasKids) toggle(row.node.id);
              rootRef.current?.focus();
            }}
            tabIndex={-1}
            aria-label={row.open ? "свернуть" : "развернуть"}
          >
            {row.hasKids ? (row.open ? "▾" : "▸") : "·"}
          </button>

          <span className="tree-key">{row.node.key}</span>

          <span className="tree-type">
            {typeLabel(row.node)}
          </span>

          {row.node.nonEmpty === 0 && (
            <span className="tree-empty" title="во всех записях null, пустая строка, [] или {}">
              пусто
            </span>
          )}

          <span className="tree-count">{fmtInt(row.node.count)}</span>

          {row.node.paths.length > 1 && (
            <span className="tree-badge" title={row.node.paths.join("\n")}>
              ×{row.node.paths.length} путей
            </span>
          )}
        </div>
      ))}
    </div>
  );
}
