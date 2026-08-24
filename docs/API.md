# Справочник команд

Граница между фронтендом и Rust. Со стороны JS всё собрано в `src/api.ts`,
со стороны Rust — в `src-tauri/src/lib.rs`.

## Соглашения

* Имя команды в `invoke` совпадает с именем функции в Rust и пишется `snake_case`: `get_record_index`.
* Аргументы Tauri сам переводит из `camelCase` в `snake_case`: `{ pathIds }` в JS приезжает как `path_ids` в Rust.
* Поля структур наоборот: Rust отдаёт `camelCase` благодаря `#[serde(rename_all = "camelCase")]`.
* Любая команда может вернуть строку ошибки. Пока папка не просканирована, всё, кроме `pick_folder` и `scan_folder`, отвечает `«папка ещё не просканирована»`.

## Команды

### `pickFolder(): Promise<string | null>`

Системный диалог выбора папки. `null` — пользователь отменил.

### `scanFolder(path: string): Promise<ScanSummary>`

Полный пересбор индекса. Долгая операция: выполняется вне главного потока,
прогресс идёт событиями. Старая база удаляется и создаётся заново.

Ошибки: `«папка не найдена: …»`, ошибки чтения диска.

### `onScanProgress(cb): Promise<UnlistenFn>`

Подписка на событие `scan:progress`. Приходит не чаще 20 раз в секунду плюс
гарантированно на последнем файле.

```ts
interface ScanProgress {
  filesDone: number;
  filesTotal: number;
  records: number;
  current: string;   // относительный путь текущего файла
}
```

### `getTree(mode): Promise<TreeNode[]>`

`mode` — `"byName"` или `"byPath"`. Возвращает готовое дерево целиком, включая
детей. Пересканирование не нужно: оба режима строятся из одного индекса, так что
переключение мгновенное.

### `getValues(query: ValueQuery): Promise<ValuePage>`

Страница значений по выбранным путям.

| поле | смысл |
|---|---|
| `pathIds` | обязателен; пустой массив вернёт пустую страницу |
| `filter` | подстрока в текстовом представлении значения; `%` и `_` экранируются |
| `fileId` | ограничить одним файлом |
| `sort` | `file` \| `rec` \| `value` \| `type` \| `path` |
| `order` | `asc` \| `desc` |
| `offset` | смещение |
| `limit` | обрезается до диапазона 1…5000 |

Сортировка по `value` ставит сначала числа по возрастанию, затем строки по
алфавиту: `ORDER BY (num IS NULL), num, txt`. `total` не зависит от страницы.

### `getRecords(pathIds, offset, limit): Promise<RecordPage>`

Список записей, в которых встречается хотя бы один из путей. Пустой `pathIds` —
все записи набора. Порядок: по файлу, затем по номеру записи.

### `getRecord(fileId, rec): Promise<RecordView>`

Одна запись целиком, перечитанная из исходного файла. Строки не обрезаны.

Ошибки: `«не открывается: …»`, `«запись N не найдена»`.

### `getRecordIndex(pathIds, fileId, rec): Promise<number>`

Порядковый номер записи внутри выборки по этим путям. Нужен, чтобы прыжок из
таблицы значений встал на правильную позицию листалки.

### `getErrors(): Promise<ErrorRow[]>`

Файлы, которые не разобрались. Не больше 5000 строк.

### `getFiles(): Promise<FileRow[]>`

Список файлов набора — наполняет выпадающий фильтр по файлу.

### `getSummary(): Promise<ScanSummary | null>`

Сводка последнего скана, сохранённая в индексе.

### `exportMd(mode): Promise<string>`

Пишет `STRUCTURE.md` в корень просканированной папки в выбранном режиме дерева.
Возвращает полный путь к файлу. Это **единственная** запись приложения на диск
за пределами своей временной базы.

Ошибки: `«не удалось записать …»` — например, папка только для чтения.

## Типы

```ts
interface ScanSummary {
  root: string;
  filesScanned: number;
  filesFailed: number;   // файлов с хотя бы одной ошибкой
  records: number;
  keys: number;          // уникальных канонических путей
  values: number;
  bytes: number;
  elapsedMs: number;
}

interface TreeNode {
  id: string;            // "p42" в byPath, "n:name" в byName
  key: string;           // подпись узла: "name", "tags[]", "$"
  path: string;          // канонический путь; в byName — первый из paths
  isArray: boolean;      // контейнер, значений не показывает
  isLeaf: boolean;
  count: number;         // вхождений во всех файлах
  nonEmpty: number;      // непустых значений в поддереве; 0 → бейдж «пусто»
  types: string[];       // ["string"] | ["null","number"] | ["object"] …
  pathIds: number[];     // чем запрашивать значения
  paths: string[];       // все слитые пути; length > 1 → бейдж «×N путей»
  children: TreeNode[];
}

interface ValueRow {
  fileId: number;
  file: string;          // путь относительно корня набора
  rec: number;           // номер записи внутри файла, с нуля
  path: string;          // канонический путь именно этого значения
  vtype: "null" | "bool" | "number" | "string" | "object" | "array";
  value: string | null;  // null только при vtype === "null"
  truncated: boolean;    // строка была длиннее 4096 символов
}

interface ValuePage  { total: number; offset: number; rows: ValueRow[] }
interface RecordRef  { fileId: number; file: string; rec: number }
interface RecordPage { total: number; offset: number; rows: RecordRef[] }
interface RecordView { fileId: number; file: string; rec: number; json: unknown; truncated: boolean }
interface ErrorRow   { file: string; line: number | null; message: string }
interface FileRow    { id: number; path: string; records: number }
```

## Добавить новую команду

1. Функция с `#[tauri::command]` в `lib.rs`.
2. Имя в списке `tauri::generate_handler![…]` — про это забывают чаще всего.
3. Типы результата в `model.rs` с `#[serde(rename_all = "camelCase")]`.
4. Зеркало типа в `src/types.ts`.
5. Обёртка в `src/api.ts`.
6. Если команда трогает индекс — тест в `src-tauri/tests/pipeline.rs`.
