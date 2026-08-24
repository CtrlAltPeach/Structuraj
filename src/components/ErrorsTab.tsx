import { fmtInt } from "../format";
import type { ErrorRow } from "../types";

export function ErrorsTab({ rows }: { rows: ErrorRow[] }) {
  if (rows.length === 0) {
    return <div className="empty">Все файлы разобраны без ошибок.</div>;
  }
  return (
    <div className="table-wrap" tabIndex={0} role="region" aria-label="Ошибки чтения файлов">
      <table className="table">
        <thead>
          <tr>
            <th>Файл</th>
            <th>Строка</th>
            <th>Что не так</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r, i) => (
            <tr key={i}>
              <td className="cell-file" title={r.file}>
                {r.file}
              </td>
              <td className="cell-num">{r.line === null ? "—" : fmtInt(r.line)}</td>
              <td className="cell-msg">{r.message}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
