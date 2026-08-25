import type { TreeNode } from "./types";

/**
 * Узел — чистый массив: во всех вхождениях это массив и ничего больше.
 * Только у такого узла нечего показать в панели значений.
 *
 * По `isArray` судить нельзя. Флаг ставится на путь при первом же встреченном
 * массиве и больше не снимается, поэтому ключ, который в одних файлах строка,
 * а в других массив, помечен массивом целиком — и его строковые значения
 * становились недостижимы.
 */
export function isPureArray(node: TreeNode): boolean {
  return node.types.length > 0 && node.types.every((t) => t === "array");
}

/** Подпись типа в дереве: «массив», «объект» или перечисление реальных типов. */
export function typeLabel(node: TreeNode): string {
  if (isPureArray(node)) return "массив";
  if (node.types.length === 0) return "объект";
  return node.types.join(" | ");
}
