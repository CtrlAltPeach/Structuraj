//! Сквозная проверка ядра на фикстурах: скан -> дерево -> значения -> запись.

use structuraj_lib::model::{TreeNode, ValueQuery};
use structuraj_lib::{db, query, scan, tree};
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Служебные дампы кладём ВНЕ папки фикстур: иначе следующий скан посчитает
/// их своими файлами и все счётчики уедут.
fn samples() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/samples");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn scan_fixtures(tag: &str) -> rusqlite::Connection {
    let dbp = std::env::temp_dir().join(format!("structuraj-test-{tag}.sqlite"));
    let conn = db::open_fresh(&dbp).expect("база не создалась");
    scan::scan(&fixtures(), &conn, |_| {}).expect("скан упал");
    conn
}

fn find<'a>(nodes: &'a [TreeNode], key: &str) -> Option<&'a TreeNode> {
    for n in nodes {
        if n.key == key {
            return Some(n);
        }
        if let Some(f) = find(&n.children, key) {
            return Some(f);
        }
    }
    None
}

fn count_nodes(nodes: &[TreeNode]) -> usize {
    nodes.iter().map(|n| 1 + count_nodes(&n.children)).sum()
}

#[test]
fn scan_reads_every_supported_file() {
    let conn = scan_fixtures("scan");
    let s = query::summary(&conn).expect("сводка не сохранилась");

    // a.json (3) + sub/b.jsonl (3) + c.json (1) = 7; broken.json даёт 0 записей.
    assert_eq!(s.files_scanned, 4, "должно найтись 4 файла");
    assert_eq!(s.records, 7, "должно разобраться 7 записей");
    assert_eq!(s.files_failed, 1, "битый файл должен попасть в ошибки");

    let errs = query::errors(&conn).unwrap();
    assert_eq!(errs.len(), 1);
    assert!(errs[0].file.contains("broken.json"));
}

#[test]
fn by_path_keeps_namesakes_apart() {
    let conn = scan_fixtures("bypath");
    let nodes = tree::build(&conn, "byPath").unwrap();

    // user.name, order.name и корневой name — три разных узла.
    let user = find(&nodes, "user").expect("нет узла user");
    let order = find(&nodes, "order").expect("нет узла order");
    assert!(find(&user.children, "name").is_some());
    assert!(find(&order.children, "name").is_some());

    // Корень встречается в двух видах: массив записей (a.json, b.jsonl) и
    // одиночный объект (c.json). Поэтому у него два разных `name`:
    // `$[].name` — 5 вхождений, `$.name` — 1. В byPath они не сливаются.
    let root = &nodes[0];
    let top_names: Vec<&TreeNode> = root.children.iter().filter(|n| n.key == "name").collect();
    assert_eq!(top_names.len(), 2, "byPath не сливает разные пути");
    let total: i64 = top_names.iter().map(|n| n.count).sum();
    assert_eq!(total, 6, "корневых name всего 6");
    assert!(top_names.iter().any(|n| n.path == "$[].name" && n.count == 5));
    assert!(top_names.iter().any(|n| n.path == "$.name" && n.count == 1));
    assert!(top_names.iter().all(|n| n.path_ids.len() == 1));
}

#[test]
fn by_name_merges_namesakes_into_one_node() {
    let conn = scan_fixtures("byname");
    let nodes = tree::build(&conn, "byName").unwrap();

    let name = find(&nodes, "name").expect("нет узла name");
    // Корневой + user.name + order.name = три разных канонических пути.
    assert!(
        name.paths.len() >= 3,
        "в byName все name должны слиться, путей: {:?}",
        name.paths
    );
    assert_eq!(
        name.path_ids.len(),
        name.paths.len(),
        "pathIds и paths должны совпадать по длине"
    );

    // И при этом ключ существует в дереве ровно один раз.
    fn occurrences(nodes: &[TreeNode], key: &str) -> usize {
        nodes
            .iter()
            .map(|n| (n.key == key) as usize + occurrences(&n.children, key))
            .sum()
    }
    assert_eq!(occurrences(&nodes, "name"), 1, "ключ не должен дублироваться");
    assert!(count_nodes(&nodes) > 5);
}

#[test]
fn arrays_of_objects_show_keys_directly() {
    let conn = scan_fixtures("arrays");
    let nodes = tree::build(&conn, "byPath").unwrap();

    // items — массив объектов: sku и qty висят прямо под ним, без узла [].
    let items = find(&nodes, "items").expect("нет узла items");
    assert!(items.is_array);
    assert!(find(&items.children, "sku").is_some());
    assert!(find(&items.children, "qty").is_some());
    assert!(
        items.children.iter().all(|c| c.key != "[]"),
        "узел [] должен схлопываться для массивов объектов"
    );

    // tags — массив строк: скалярные значения живут в дочернем узле [].
    let tags = find(&nodes, "tags").expect("нет узла tags");
    assert!(tags.is_array);
    let elem = find(&tags.children, "[]").expect("нет узла элементов tags");
    assert!(elem.types.contains(&"string".to_string()));
}

#[test]
fn values_paginate_filter_and_sort() {
    let conn = scan_fixtures("values");
    let nodes = tree::build(&conn, "byName").unwrap();
    let name = find(&nodes, "name").unwrap();

    let all = query::values(
        &conn,
        &ValueQuery {
            path_ids: name.path_ids.clone(),
            filter: None,
            file_id: None,
            sort: "value".into(),
            order: "asc".into(),
            offset: 0,
            limit: 100,
        },
    )
    .unwrap();
    assert!(all.total >= 7);
    let sorted: Vec<_> = all.rows.iter().filter_map(|r| r.value.clone()).collect();
    let mut expect = sorted.clone();
    expect.sort();
    assert_eq!(sorted, expect, "сортировка по значению не работает");

    let page = query::values(
        &conn,
        &ValueQuery {
            path_ids: name.path_ids.clone(),
            filter: None,
            file_id: None,
            sort: "value".into(),
            order: "asc".into(),
            offset: 2,
            limit: 3,
        },
    )
    .unwrap();
    assert_eq!(page.rows.len(), 3, "страница должна быть ровно 3 строки");
    assert_eq!(page.total, all.total, "total не зависит от страницы");

    let filtered = query::values(
        &conn,
        &ValueQuery {
            path_ids: name.path_ids.clone(),
            filter: Some("ord-".into()),
            file_id: None,
            sort: "rec".into(),
            order: "asc".into(),
            offset: 0,
            limit: 100,
        },
    )
    .unwrap();
    assert_eq!(filtered.total, 2, "под фильтр ord- подходят два значения");
}

#[test]
fn record_is_read_back_from_source_file() {
    let conn = scan_fixtures("record");
    let nodes = tree::build(&conn, "byName").unwrap();
    let name = find(&nodes, "name").unwrap();

    let recs = query::records(&conn, &name.path_ids, 0, 10).unwrap();
    assert_eq!(recs.total, 7, "name есть в семи записях");

    let first = &recs.rows[0];
    let view = query::record(&conn, first.file_id, first.rec).expect("запись не прочиталась");
    assert!(view.json.is_object());

    // Позиция записи в выборке должна совпадать с её номером в списке.
    let idx = query::record_index(&conn, &name.path_ids, first.file_id, first.rec).unwrap();
    assert_eq!(idx, 0);
    let third = &recs.rows[2];
    let idx3 = query::record_index(&conn, &name.path_ids, third.file_id, third.rec).unwrap();
    assert_eq!(idx3, 2);
}

#[test]
fn markdown_export_lists_keys_with_counts() {
    let conn = scan_fixtures("md");
    let summary = query::summary(&conn).unwrap();
    let nodes = tree::build(&conn, "byName").unwrap();
    let md = tree::to_markdown(&nodes, "C:/тест", "byName", &summary);

    assert!(md.starts_with("# Структура JSON"));
    assert!(md.contains("C:/тест"));
    assert!(md.contains("**name**"), "в md нет ключа name");
    assert!(md.contains("**id**"), "в md нет ключа id");
    assert!(md.contains("массив"), "в md нет отметки массива");
    assert!(md.contains("путей: "), "в md нет счётчика слитых путей");
}

/// Служебный прогон: кладёт STRUCTURE.md в tests/samples, чтобы можно было
/// глазами посмотреть формат. Запуск: cargo test --test pipeline -- --ignored
#[test]
#[ignore]
fn dump_markdown_sample() {
    let conn = scan_fixtures("dump");
    let summary = query::summary(&conn).unwrap();
    for mode in ["byName", "byPath"] {
        let nodes = tree::build(&conn, mode).unwrap();
        let md = tree::to_markdown(&nodes, &fixtures().to_string_lossy(), mode, &summary);
        let out = samples().join(if mode == "byName" {
            "STRUCTURE-BY-NAME.md"
        } else {
            "STRUCTURE-BY-PATH.md"
        });
        std::fs::write(&out, md).unwrap();
        println!("написано: {}", out.display());
    }
}

/// Служебный прогон: выгружает реальные ответы команд в JSON — их вставляем
/// в бриф для дизайнера, чтобы вёрстка делалась под настоящие данные.
/// Пишем в tests/samples — в папке фикстур посторонним файлам не место.
#[test]
#[ignore]
fn dump_api_samples() {
    let conn = scan_fixtures("api");
    let nodes = tree::build(&conn, "byName").unwrap();
    let name = find(&nodes, "name").unwrap();

    let values = query::values(
        &conn,
        &ValueQuery {
            path_ids: name.path_ids.clone(),
            filter: None,
            file_id: None,
            sort: "rec".into(),
            order: "asc".into(),
            offset: 0,
            limit: 4,
        },
    )
    .unwrap();
    let recs = query::records(&conn, &name.path_ids, 0, 3).unwrap();
    let rec = query::record(&conn, recs.rows[0].file_id, recs.rows[0].rec).unwrap();

    let bundle = serde_json::json!({
        "getSummary": query::summary(&conn),
        "getTree_byName": nodes,
        "getValues": values,
        "getRecords": recs,
        "getRecord": rec,
        "getErrors": query::errors(&conn).unwrap(),
    });
    let out = samples().join("API-SAMPLES.json");
    std::fs::write(&out, serde_json::to_string_pretty(&bundle).unwrap()).unwrap();
    println!("написано: {}", out.display());
}

#[test]
fn always_empty_keys_are_marked() {
    let conn = scan_fixtures("empty");
    let nodes = tree::build(&conn, "byPath").unwrap();

    // Ключ есть в каждой записи, но значение всегда null.
    let note = find(&nodes, "note").expect("нет узла note");
    assert_eq!(note.count, 3);
    assert_eq!(note.non_empty, 0, "note должен считаться пустым");

    // Строка из одних пробелов — тоже пусто.
    let blank = find(&nodes, "blank").expect("нет узла blank");
    assert_eq!(blank.non_empty, 0, "строка из пробелов — пустая");

    // Объект, внутри которого только пустые значения, пуст целиком.
    let boxed = find(&nodes, "emptyBox").expect("нет узла emptyBox");
    assert_eq!(boxed.non_empty, 0, "объект с одними null пуст целиком");

    // А обычные ключи — нет.
    let id = find(&nodes, "id").expect("нет узла id");
    assert!(id.non_empty > 0, "id не может быть пустым");

    // Родитель непустого ключа тоже считается непустым: иначе фильтр срезал бы ветку.
    let user = find(&nodes, "user").expect("нет узла user");
    assert!(user.non_empty > 0, "user содержит непустой name");
}
