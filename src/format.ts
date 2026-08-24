/** 12480 -> "12 480" (узкий неразрывный пробел, чтобы число не рвалось). */
export function fmtInt(n: number): string {
  return n.toLocaleString('ru-RU');
}

export function fmtBytes(n: number): string {
  const units = ["Б", "КБ", "МБ", "ГБ", "ТБ"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v < 10 && i > 0 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

export function fmtMs(ms: number): string {
  if (ms < 1000) return `${ms} мс`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)} с`;
  return `${Math.floor(s / 60)} мин ${Math.round(s % 60)} с`;
}
