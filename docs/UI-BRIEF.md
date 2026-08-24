# Бриф на UI: Structuraj

Вставь этот файл в ChatGPT целиком. Он самодостаточный: в нём есть контракт данных,
реальные ответы бэкенда и список того, что менять нельзя.

---

## 1. Что за приложение

Десктопное приложение (Tauri v2, Windows) для разведки больших наборов JSON/JSONL.
Пользователь выбирает папку — приложение обходит все вложенные `.json`, `.jsonl`,
`.ndjson`, строит **единую структуру ключей по всем файлам сразу** и даёт по любому
ключу посмотреть все его значения: листать, сортировать, фильтровать.

Типичный набор данных: 100 МБ – 2 ГБ, десятки-сотни файлов, миллионы однотипных
объектов с частично различающимися ключами. Всё только на чтение — приложение
никогда не меняет исходные файлы. Единственная запись на диск — `STRUCTURE.md`
в целевую папку по кнопке.

Главное, что должен делать интерфейс: **быстро отвечать на вопрос «а что вообще
лежит в этих файлах и какие там бывают значения»**.

---

## 2. Стек и жёсткие ограничения

| | |
|---|---|
| Оболочка | Tauri v2, движок WebView2 (Chromium, свежий) |
| Фронт | React 19 + TypeScript + Vite |
| Данные | Rust + SQLite, приходят через `invoke` |
| Разрешение | окно от 1100×700, должно жить и на 2560×1440 |

Ограничения, которые нельзя обойти:

1. **Только один HTML-документ.** Никаких внешних CDN, шрифтов с сети, картинок по URL. Всё инлайном или локальным файлом в `src/`.
2. **Никаких новых npm-зависимостей** без явного согласования. Ни UI-китов, ни icon-паков, ни tailwind. CSS пишем руками в `src/styles.css`.
3. **Иконки — инлайновый SVG** прямо в JSX или CSS-символами.
4. Тёмная и светлая темы через `prefers-color-scheme`. Сейчас палитра — CSS-переменные в `:root`; сохрани этот приём.
5. Плотность интерфейса — как в дев-инструментах: базовый шрифт 13px, строка таблицы ~22px. Это рабочий инструмент, а не лендинг.

---

## 3. Экраны и состояния

Приложение одноэкранное. Шапка + две панели рядом (не отдельные окна).

```
┌──────────────────────────────────────────────────────────────────────┐
│ [Выбрать папку]  C:\data\dump   файлов 12 · записей 12 480 · …        │
│                                 [по имени|по пути]  [Экспорт .md]     │
├──────────────────────────────────────────────────────────────────────┤
│ прогресс-бар (виден только во время скана)                            │
├───────────────────────────┬──────────────────────────────────────────┤
│ [поиск ключа…]            │ [Все вхождения][Запись целиком][Ошибки]   │
│                           │                        name · string · 11 │
│ ▾ $            массив   5 ├──────────────────────────────────────────┤
│   · id       number   12k │ [фильтр] [все файлы ▾]  1–200 из 11  ←  → │
│   · name     string   11  │ ┌──────┬────────┬──────┬──────┬─────────┐ │
│   ▾ user     object   3   │ │ Файл │ №      │ Путь │ Тип  │ Значение│ │
│     · age    number   1   │ └──────┴────────┴──────┴──────┴─────────┘ │
└───────────────────────────┴──────────────────────────────────────────┘
```

Состояния, которые обязательно нужно нарисовать:

1. **Пусто** — папка не выбрана. Крупный призыв к действию, объяснение в одну фразу.
2. **Скан идёт** — прогресс-бар с «12 / 340 файлов · 1 204 500 записей · текущий файл». Кнопка выбора папки заблокирована.
3. **Готово** — дерево слева, панель справа, ключ не выбран → подсказка «выбери ключ».
4. **Выбран массив** — панель значений пуста по смыслу: массивы значений не показывают, они есть только в структуре. Нужен внятный текст, а не пустой экран.
5. **Ничего не найдено** — фильтр не дал строк.
6. **Ошибка** — красная плашка с текстом от бэкенда (моноширинный).

---

## 4. Контракт данных

Фронт разговаривает с Rust **только** через `src/api.ts`. Эти сигнатуры менять нельзя.

```ts
pickFolder(): Promise<string | null>              // системный диалог, null = отмена
scanFolder(path: string): Promise<ScanSummary>    // долго! слушать onScanProgress
onScanProgress(cb: (p: ScanProgress) => void): Promise<UnlistenFn>

getTree(mode: "byName" | "byPath"): Promise<TreeNode[]>
getValues(query: ValueQuery): Promise<ValuePage>
getRecords(pathIds: number[], offset: number, limit: number): Promise<RecordPage>
getRecord(fileId: number, rec: number): Promise<RecordView>
getRecordIndex(pathIds: number[], fileId: number, rec: number): Promise<number>
getErrors(): Promise<ErrorRow[]>
getFiles(): Promise<FileRow[]>
getSummary(): Promise<ScanSummary | null>
exportMd(mode: "byName" | "byPath"): Promise<string>   // вернёт путь к STRUCTURE.md
```

Типы (полный список — в `src/types.ts`):

```ts
interface TreeNode {
  id: string;          // стабильный ключ для React
  key: string;         // подпись узла: "name", "tags[]", "$"
  path: string;        // канонический путь: "$[].user.name"
  isArray: boolean;    // массив — контейнер, значений не показывает
  isLeaf: boolean;
  count: number;       // сколько раз ключ встретился во всех файлах
  nonEmpty: number;    // непустые значения в поддереве; 0 → бейдж «пусто»
  types: string[];     // ["string"] | ["null","number"] | ["object"] …
  pathIds: number[];   // чем запрашивать значения
  paths: string[];     // все слитые пути; length > 1 → показать бейдж
  children: TreeNode[];
}

interface ValueQuery {
  pathIds: number[];
  filter?: string | null;                              // подстрока в значении
  fileId?: number | null;                              // ограничить одним файлом
  sort: "file" | "rec" | "value" | "type" | "path";
  order: "asc" | "desc";
  offset: number;
  limit: number;                                       // сейчас страница 200
}

interface ValueRow {
  fileId: number; file: string; rec: number; path: string;
  vtype: "null" | "bool" | "number" | "string" | "object" | "array";
  value: string | null;      // null только при vtype === "null"
  truncated: boolean;        // строка длиннее 4096 символов — обрезана
}
```

### Два режима дерева

Тумблер в шапке. Это не украшение, а разная модель данных:

* **по имени** (по умолчанию) — все одноимённые ключи слиты в один узел. `paths.length > 1` означает, что ключ живёт в нескольких местах; их надо показать (бейдж + tooltip + колонка «Путь» в таблице).
* **по пути** — `user.name` и `order.name` разные узлы, бейджей нет.

---

## 5. Реальные ответы бэкенда

Не выдумывай поля — вот настоящие данные с тестового набора.

`getSummary()`:

```json
{"root":"C:\\data\\dump","filesScanned":4,"filesFailed":1,"records":7,
 "keys":22,"values":33,"bytes":791,"elapsedMs":3}
```

`getTree("byName")` — корень и один из детей:

```json
{"id":"n:$","key":"$","path":"$","isArray":true,"isLeaf":false,"count":5,
 "types":["object","array"],"pathIds":[1],"paths":["$"],"children":[ … ]}

{"id":"n:name","key":"name","path":"$[].name","isArray":false,"isLeaf":true,
 "count":11,"types":["string"],
 "pathIds":[4,10,17,22],
 "paths":["$[].name","$[].user.name","$[].order.name","$.name"],
 "children":[]}
```

`getValues(...)`:

```json
{"total":11,"offset":0,"rows":[
 {"fileId":1,"file":"a.json","rec":0,"path":"$[].name","vtype":"string","value":"alpha","truncated":false},
 {"fileId":1,"file":"a.json","rec":0,"path":"$[].user.name","vtype":"string","value":"ann","truncated":false},
 {"fileId":1,"file":"a.json","rec":1,"path":"$[].name","vtype":"string","value":"beta","truncated":false}
]}
```

`getRecord(1, 0)`:

```json
{"fileId":1,"file":"a.json","rec":0,"truncated":false,
 "json":{"id":1,"name":"alpha","price":10.5,"tags":["x","y"],"user":{"age":30,"name":"ann"}}}
```

`getErrors()`:

```json
[{"file":"broken.json","line":1,"message":"expected value at line 1 column 18"}]
```

---

## 6. Что уже есть в разметке

Компоненты живут в `src/`: `App.tsx`, `components/TreeView.tsx`,
`components/ValuesTab.tsx`, `components/RecordTab.tsx`, `components/ErrorsTab.tsx`,
стили — `src/styles.css`. Классы сейчас такие:

**Каркас:** `app` · `topbar` `topbar-path` `topbar-stats` · `seg` `seg-btn.is-on` ·
`btn` `btn-primary` `btn-ghost` · `progress` `progress-bar` `progress-text` ·
`note` · `error-box` `error-top` · `split` · `panel` `panel-tree` `panel-values` ·
`panel-head` `panel-body` `panel-sub` · `tabs` `tab.is-on` · `empty` ·
`input` `input-num`

**Дерево:** `tree-root` (фокусируемый контейнер, плоский список строк) ·
`tree-row.is-selected .is-container` · `tree-toggle.is-empty` ·
`tree-key` `tree-type` `tree-count` `tree-badge` `tree-empty` · `check` (чекбокс в шапке)

Отступ уровня задаётся инлайновым `padding-left` — в CSS его не переопределять.

**Таблица значений:** `values` `values-toolbar` `values-count` `pager` ·
`table-wrap` `table` · `th.sortable.is-sorted` `caret` ·
`cell-file` `cell-num` `cell-path` `cell-type` `cell-value` `cell-msg` `cell-actions` ·
`null` `trunc`

**Запись целиком:** `record` `record-toolbar` `record-count` `record-src` ·
`record-json` `json-line.is-hit`

---

## 7. Чего делать нельзя

1. Не менять `src/api.ts` и `src/types.ts` — это граница с Rust.
2. Не выбрасывать обработчики: сортировка по клику на заголовок, дебаунс фильтра 250 мс, кнопка «запись» в строке таблицы прыгает на вкладку «Запись целиком».
3. Клавиатура в дереве: ↑/↓ двигают выделение, ←/→ сворачивают и разворачивают, Home/End прыгают на края, Enter и пробел переключают узел. Вне дерева ←/→ листают записи. `tree-root` обязан оставаться `tabIndex={0}`, иначе всё это отвалится.
4. Не превращать таблицу в карточки: пользователь сравнивает значения глазами по колонкам.
5. Не добавлять анимации длиннее 150 мс и не анимировать смену страницы таблицы.
6. Не прятать счётчики вхождений — это главный сигнал «этот ключ есть везде / этот встречается трижды».

---

## 8. Что вернуть

**Режим A (безопасный, по умолчанию):** полностью переписанный `src/styles.css` плюс
список новых классов, если они нужны, с указанием, куда их поставить в JSX.

**Режим B (если просят редизайн):** новые `App.tsx` и файлы из `components/`
целиком — но вся логика (useState, useEffect, вызовы из `api.ts`) переносится
без изменений, меняется только разметка и классы.

В обоих случаях: один ответ = готовые файлы целиком, без «…остальное без изменений».
