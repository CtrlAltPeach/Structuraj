//! Сборка дерева структуры из таблицы `paths` и экспорт его в Markdown.
//!
//! Два режима:
//!   * `byPath` — узел = канонический путь. `user.name` и `order.name` разные.
//!   * `byName` — узел = имя ключа. Все `name` из любых мест — один узел,
//!     подвешенный к самому частому родителю; остальные пути видны в `paths`.

use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

use crate::db::{mask_names, M_BOOL, M_NULL, M_NUM, M_STR};
use crate::model::TreeNode;

const SCALAR_MASK: i64 = M_NULL | M_BOOL | M_NUM | M_STR;

struct Row {
    id: i64,
    path: String,
    parent: Option<i64>,
    key: String,
    is_array: bool,
    cnt: i64,
    nonempty: i64,
    mask: i64,
}

fn load_rows(conn: &Connection) -> rusqlite::Result<HashMap<i64, Row>> {
    let mut stmt =
        conn.prepare("SELECT id, path, parent_id, key, is_array, cnt, nonempty, mask FROM paths")?;
    let iter = stmt.query_map([], |r| {
        Ok(Row {
            id: r.get(0)?,
            path: r.get(1)?,
            parent: r.get(2)?,
            key: r.get(3)?,
            is_array: r.get::<_, i64>(4)? != 0,
            cnt: r.get(5)?,
            nonempty: r.get(6)?,
            mask: r.get(7)?,
        })
    })?;
    let mut map = HashMap::new();
    for row in iter {
        let row = row?;
        map.insert(row.id, row);
    }
    Ok(map)
}

struct Shape {
    rows: HashMap<i64, Row>,
    /// Узлы `[]`, которые схлопываются: их дети переезжают к массиву-родителю.
    collapsed: HashSet<i64>,
    /// Эффективная иерархия после схлопывания.
    kids: HashMap<Option<i64>, Vec<i64>>,
}

fn shape(rows: HashMap<i64, Row>) -> Shape {
    let mut raw_kids: HashMap<i64, Vec<i64>> = HashMap::new();
    for row in rows.values() {
        if let Some(p) = row.parent {
            raw_kids.entry(p).or_default().push(row.id);
        }
    }

    // `[]` схлопываем только если элементы — исключительно объекты/массивы.
    // Если в массиве есть и скаляры, и объекты, узел остаётся: иначе скалярные
    // значения было бы негде показать.
    let mut collapsed = HashSet::new();
    for row in rows.values() {
        let has_kids = raw_kids.get(&row.id).map(|v| !v.is_empty()).unwrap_or(false);
        if row.key == "[]" && has_kids && (row.mask & SCALAR_MASK) == 0 {
            collapsed.insert(row.id);
        }
    }

    let eff_parent = |id: i64| -> Option<i64> {
        let mut p = rows.get(&id).and_then(|r| r.parent);
        while let Some(pid) = p {
            if collapsed.contains(&pid) {
                p = rows.get(&pid).and_then(|r| r.parent);
            } else {
                break;
            }
        }
        p
    };

    let mut kids: HashMap<Option<i64>, Vec<i64>> = HashMap::new();
    let mut ids: Vec<i64> = rows.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        if collapsed.contains(&id) {
            continue;
        }
        kids.entry(eff_parent(id)).or_default().push(id);
    }

    Shape {
        rows,
        collapsed,
        kids,
    }
}

/// Ярлык для группировки по имени: элементы массива получают имя вида `tags[]`,
/// иначе все скалярные массивы в проекте слились бы в один узел `[]`.
fn group_name(sh: &Shape, id: i64) -> String {
    let row = &sh.rows[&id];
    if row.key == "[]" {
        let parent_key = row
            .parent
            .and_then(|p| sh.rows.get(&p))
            .map(|r| r.key.clone())
            .unwrap_or_else(|| "$".to_string());
        format!("{parent_key}[]")
    } else {
        row.key.clone()
    }
}

fn build_by_path(sh: &Shape, parent: Option<i64>) -> Vec<TreeNode> {
    let mut out = Vec::new();
    let Some(ids) = sh.kids.get(&parent) else {
        return out;
    };
    for id in ids {
        let row = &sh.rows[id];
        let children = build_by_path(sh, Some(*id));
        let non_empty = row.nonempty + children.iter().map(|c| c.non_empty).sum::<i64>();
        out.push(TreeNode {
            id: format!("p{id}"),
            key: row.key.clone(),
            path: row.path.clone(),
            is_array: row.is_array,
            is_leaf: children.is_empty(),
            count: row.cnt,
            non_empty,
            types: mask_names(row.mask),
            path_ids: vec![*id],
            paths: vec![row.path.clone()],
            children,
        });
    }
    out
}

#[derive(Default)]
struct Group {
    ids: Vec<i64>,
    paths: Vec<String>,
    cnt: i64,
    nonempty: i64,
    mask: i64,
    is_array: bool,
    min_id: i64,
}

fn build_by_name(sh: &Shape) -> Vec<TreeNode> {
    let mut groups: HashMap<String, Group> = HashMap::new();
    // Голоса за родителя: имя -> (имя родителя -> суммарный счётчик).
    let mut votes: HashMap<String, HashMap<String, i64>> = HashMap::new();

    let mut ids: Vec<i64> = sh
        .rows
        .keys()
        .copied()
        .filter(|id| !sh.collapsed.contains(id))
        .collect();
    ids.sort_unstable();

    // Обратный индекс: у кого какой эффективный родитель.
    let mut eff_parent_of: HashMap<i64, Option<i64>> = HashMap::new();
    for (parent, kids) in &sh.kids {
        for k in kids {
            eff_parent_of.insert(*k, *parent);
        }
    }

    for id in &ids {
        let row = &sh.rows[id];
        let name = group_name(sh, *id);
        let g = groups.entry(name.clone()).or_insert_with(|| Group {
            min_id: i64::MAX,
            ..Default::default()
        });
        g.ids.push(*id);
        g.paths.push(row.path.clone());
        g.cnt += row.cnt;
        g.nonempty += row.nonempty;
        g.mask |= row.mask;
        g.is_array |= row.is_array;
        g.min_id = g.min_id.min(*id);

        let parent_name = eff_parent_of
            .get(id)
            .copied()
            .flatten()
            .map(|p| group_name(sh, p))
            .unwrap_or_default();
        *votes
            .entry(name)
            .or_default()
            .entry(parent_name)
            .or_insert(0) += row.cnt.max(1);
    }

    // Доминирующий родитель — тот, под которым ключ встречается чаще.
    let mut parent_of: HashMap<String, String> = HashMap::new();
    for (name, v) in &votes {
        let best = v
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
            .map(|(k, _)| k.clone())
            .unwrap_or_default();
        parent_of.insert(name.clone(), best);
    }

    // Циклы (a внутри b и b внутри a) рвём: такой ключ переезжает в корень.
    let names: Vec<String> = groups.keys().cloned().collect();
    for name in &names {
        let mut seen = HashSet::new();
        let mut cur = name.clone();
        loop {
            if !seen.insert(cur.clone()) {
                parent_of.insert(name.clone(), String::new());
                break;
            }
            match parent_of.get(&cur) {
                Some(p) if !p.is_empty() => cur = p.clone(),
                _ => break,
            }
        }
    }

    let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
    for name in &names {
        let p = parent_of.get(name).cloned().unwrap_or_default();
        children_of.entry(p).or_default().push(name.clone());
    }
    for v in children_of.values_mut() {
        v.sort_by_key(|n| groups[n].min_id);
    }

    fn make(
        name: &str,
        groups: &HashMap<String, Group>,
        children_of: &HashMap<String, Vec<String>>,
        guard: &mut HashSet<String>,
    ) -> TreeNode {
        let g = &groups[name];
        let mut children = Vec::new();
        if guard.insert(name.to_string()) {
            if let Some(kids) = children_of.get(name) {
                for k in kids {
                    children.push(make(k, groups, children_of, guard));
                }
            }
            guard.remove(name);
        }
        let non_empty = g.nonempty + children.iter().map(|c| c.non_empty).sum::<i64>();
        TreeNode {
            id: format!("n:{name}"),
            key: name.to_string(),
            path: g.paths.first().cloned().unwrap_or_default(),
            is_array: g.is_array,
            is_leaf: children.is_empty(),
            count: g.cnt,
            non_empty,
            types: mask_names(g.mask),
            path_ids: g.ids.clone(),
            paths: g.paths.clone(),
            children,
        }
    }

    let mut guard = HashSet::new();
    let mut roots = Vec::new();
    if let Some(top) = children_of.get("") {
        for name in top {
            roots.push(make(name, &groups, &children_of, &mut guard));
        }
    }
    roots
}

pub fn build(conn: &Connection, mode: &str) -> rusqlite::Result<Vec<TreeNode>> {
    let sh = shape(load_rows(conn)?);
    Ok(if mode == "byPath" {
        build_by_path(&sh, None)
    } else {
        build_by_name(&sh)
    })
}

fn fmt_int(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('\u{202f}');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

fn render(nodes: &[TreeNode], depth: usize, out: &mut String) {
    for n in nodes {
        let pad = "  ".repeat(depth);
        let kind = if n.is_array {
            "массив".to_string()
        } else if n.types.is_empty() {
            "объект".to_string()
        } else {
            n.types.join(" | ")
        };
        out.push_str(&format!(
            "{pad}- **{}** — `{}` · {}",
            n.key,
            kind,
            fmt_int(n.count)
        ));
        if n.paths.len() > 1 {
            out.push_str(&format!(" · путей: {}", n.paths.len()));
        }
        if n.non_empty == 0 {
            out.push_str(" · пусто");
        }
        out.push('\n');
        render(&n.children, depth + 1, out);
    }
}

pub fn to_markdown(
    nodes: &[TreeNode],
    root: &str,
    mode: &str,
    summary: &crate::model::ScanSummary,
) -> String {
    let mut out = String::new();
    out.push_str("# Структура JSON\n\n");
    out.push_str(&format!("Папка: `{root}`\n\n"));
    out.push_str(&format!(
        "Файлов: {} · записей: {} · уникальных ключей: {} · значений: {}\n\n",
        fmt_int(summary.files_scanned as i64),
        fmt_int(summary.records as i64),
        fmt_int(summary.keys as i64),
        fmt_int(summary.values as i64)
    ));
    out.push_str(&format!(
        "Режим дерева: {}\n\n",
        if mode == "byPath" {
            "по полному пути"
        } else {
            "по имени ключа (дубликаты слиты)"
        }
    ));
    out.push_str("---\n\n");
    render(nodes, 0, &mut out);
    out
}
