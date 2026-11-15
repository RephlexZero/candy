use std::collections::{BTreeMap, HashSet};

use candy_dbc::lint::Target;
use candy_dbc::{Dbc, Message, Mux};
use leptos::prelude::*;

use crate::actions::{self, GROUP_ATTR};
use crate::icons::{Ic, Icon};
use crate::state::{Card, Sel, TreeGroup, use_app};
use crate::ui::{fmt_id, hover_in, hover_out, hue_style};

#[derive(Clone, PartialEq)]
enum Kind {
    Section(&'static str, usize, &'static str),
    Folder(String),
    Network,
    Node,
    Message { mux: bool, ext: bool },
    Branch(String),
    Signal { hue: usize, mux: Mux },
    ValueTable,
}

#[derive(Clone, PartialEq)]
struct Row {
    key: String,
    depth: usize,
    kind: Kind,
    label: String,
    meta: String,
    sel: Option<Sel>,
    expandable: bool,
    expanded: bool,
    card: Option<Card>,
    errors: usize,
    warnings: usize,
}

fn matches(f: &str, s: &str) -> bool {
    f.is_empty() || s.to_ascii_lowercase().contains(f)
}

fn message_matches(f: &str, m: &Message, hex: bool) -> bool {
    matches(f, &m.name)
        || matches(f, &fmt_id(m, hex))
        || matches(f, &format!("{:x}", m.can_id()))
        || matches(f, &m.transmitter)
}

#[component]
pub fn Tree() -> impl IntoView {
    let app = use_app();
    let collapsed = RwSignal::new(HashSet::<String>::new());

    let issue_counts = Memo::new(move |_| {
        let mut map: BTreeMap<Target, (usize, usize)> = BTreeMap::new();
        app.issues.with(|is| {
            for i in is {
                let e = map.entry(i.target.clone()).or_default();
                match i.severity {
                    candy_dbc::lint::Severity::Error => e.0 += 1,
                    candy_dbc::lint::Severity::Warning => e.1 += 1,
                    _ => {}
                }
                if let Some(id) = i.target.message_id()
                    && !matches!(i.target, Target::Message(_))
                {
                    let e = map.entry(Target::Message(id)).or_default();
                    match i.severity {
                        candy_dbc::lint::Severity::Error => e.0 += 1,
                        candy_dbc::lint::Severity::Warning => e.1 += 1,
                        _ => {}
                    }
                }
            }
        });
        map
    });

    let rows = Memo::new(move |_| {
        let filter = app.tree_filter.get().trim().to_ascii_lowercase();
        let group = app.tree_group.get();
        let hex = app.hex_ids.get();
        let expanded = app.expanded.get();
        let collapsed = collapsed.get();
        let counts = issue_counts.get();
        let sel = app.sel.get();
        let _ = sel;
        app.doc
            .with(|d| build_rows(d, &filter, group, hex, &expanded, &collapsed, &counts))
    });

    let toggle_msg = move |id: u32| {
        app.expanded.update(|e| {
            if !e.remove(&id) {
                e.insert(id);
            }
        })
    };
    let toggle_folder = move |k: String| {
        collapsed.update(|c| {
            if !c.remove(&k) {
                c.insert(k);
            }
        })
    };

    view! {
        <div class="tree-head">
            <div class="tree-search">
                <Icon icon=Ic::Search class="ic-sm"/>
                <input
                    id="tree-filter"
                    class="input"
                    placeholder="Filter messages, signals, ids…"
                    spellcheck="false"
                    autocomplete="off"
                    prop:value=move || app.tree_filter.get()
                    on:input=move |ev| app.tree_filter.set(event_target_value(&ev))
                    on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                        if ev.key() == "Escape" {
                            app.tree_filter.set(String::new());
                        }
                    }
                />
            </div>
            <div style="display:flex;gap:6px;align-items:center">
                <select
                    class="select"
                    style="height:28px;font-size:12.5px"
                    title="Group messages"
                    on:change=move |ev| {
                        let v = event_target_value(&ev);
                        if let Some(g) = TreeGroup::ALL.iter().find(|g| g.label() == v) {
                            app.tree_group.set(*g);
                        }
                    }
                >
                    {TreeGroup::ALL
                        .iter()
                        .map(|g| {
                            let g = *g;
                            view! { <option prop:selected=move || app.tree_group.get() == g>{g.label()}</option> }
                        })
                        .collect_view()}
                </select>
                <button
                    class="btn btn-sm"
                    title="Show ids in hex or decimal (H)"
                    style="font-family:var(--font-mono);min-width:44px;justify-content:center"
                    on:click=move |_| app.hex_ids.update(|h| *h = !*h)
                >
                    {move || if app.hex_ids.get() { "0x" } else { "123" }}
                </button>
                <button
                    class="btn btn-sm"
                    title="Expand / collapse all messages"
                    on:click=move |_| {
                        let all: HashSet<u32> = app.doc.with_untracked(|d| d.messages.iter().map(|m| m.id).collect());
                        app.expanded.update(|e| if e.is_empty() { *e = all } else { e.clear() });
                    }
                >
                    <Icon icon=Ic::ChevronDown class="ic-sm"/>
                </button>
            </div>
        </div>
        <div class="tree-body" role="tree">
            {move || {
                let rows = rows.get();
                let sel = app.sel.get();
                let filter = app.tree_filter.get().trim().to_string();
                if rows.len() <= 3 && !filter.is_empty() {
                    return view! { <div class="tree-empty">"Nothing matches “" {filter} "”"</div> }.into_any();
                }
                rows.into_iter()
                    .map(|r| {
                        let active = r.sel.as_ref().is_some_and(|s| *s == sel);
                        render_row(r, active, &filter, toggle_msg, toggle_folder)
                    })
                    .collect_view()
                    .into_any()
            }}
        </div>
    }
}

fn render_row(
    r: Row,
    active: bool,
    filter: &str,
    toggle_msg: impl Fn(u32) + Copy + 'static,
    toggle_folder: impl Fn(String) + Copy + 'static,
) -> AnyView {
    let app = use_app();
    if let Kind::Section(title, count, add) = r.kind.clone() {
        return view! {
            <div class="tree-section">
                <span class="eyebrow">{title}" "<span class="subtle">{count}</span></span>
                <button
                    class="icon-btn sm"
                    title=format!("Add {add}")
                    on:click=move |_| match add {
                        "node" => actions::add_node(app),
                        "message" => actions::add_message(app),
                        _ => actions::add_value_table(app),
                    }
                >
                    <Icon icon=Ic::Plus class="ic-sm"/>
                </button>
            </div>
        }
        .into_any();
    }
    let pad = format!("padding-left:{}px", 6 + r.depth * 16);
    let (icon, style) = match &r.kind {
        Kind::Network => (Some(Ic::Network), String::new()),
        Kind::Node => (Some(Ic::Node), String::new()),
        Kind::Message { mux: true, .. } => (Some(Ic::Mux), String::new()),
        Kind::Message { .. } => (Some(Ic::Message), String::new()),
        Kind::Folder(_) => (Some(Ic::Folder), String::new()),
        Kind::ValueTable => (Some(Ic::Table), String::new()),
        Kind::Branch(_) => (None, String::new()),
        Kind::Signal { hue, .. } => (None, hue_style(*hue)),
        Kind::Section(..) => (None, String::new()),
    };
    let sel = r.sel.clone();
    let kind = r.kind.clone();
    let key = r.key.clone();
    let expandable = r.expandable;
    let expanded = r.expanded;
    let msg_id = match &r.sel {
        Some(Sel::Message(id)) => Some(*id),
        _ => None,
    };
    let caret_click = {
        let key = key.clone();
        let kind = kind.clone();
        move |ev: leptos::ev::MouseEvent| {
            ev.stop_propagation();
            match (&kind, msg_id) {
                (Kind::Message { .. }, Some(id)) => toggle_msg(id),
                _ => toggle_folder(key.clone()),
            }
        }
    };
    let row_click = {
        let sel = sel.clone();
        let key = key.clone();
        move |_| match &sel {
            Some(s) => {
                app.select(s.clone());
                if let Sel::Signal(id, name) = s {
                    crate::persist::scroll_into_view(&format!("sigrow-{id}-{name}"));
                }
            }
            None => toggle_folder(key.clone()),
        }
    };
    let card = r.card.clone();
    let (enter, leave) = (card.clone().map(hover_in), hover_out());
    let mux_tag = match &r.kind {
        Kind::Signal {
            mux: Mux::Multiplexor,
            ..
        } => Some("M".to_string()),
        Kind::Signal {
            mux: Mux::MultiplexedMultiplexor(v),
            ..
        } => Some(format!("m{v}M")),
        Kind::Message { ext: true, .. } => Some("EXT".to_string()),
        _ => None,
    };
    let folder = matches!(r.kind, Kind::Folder(_) | Kind::Branch(_));
    view! {
        <div
            class="tree-row"
            class:active=active
            role="treeitem"
            style=format!("{pad};{style}")
            on:click=row_click
            on:mouseenter=move |ev| {
                if let Some(f) = &enter {
                    f(ev)
                }
            }
            on:mouseleave=leave
        >
            {if expandable {
                view! {
                    <span class="tree-caret" on:click=caret_click>
                        <Icon icon=if expanded { Ic::ChevronDown } else { Ic::ChevronRight } class="ic-sm"/>
                    </span>
                }
                .into_any()
            } else {
                view! { <span style="width:16px;flex:none"></span> }.into_any()
            }}
            {match (icon, &r.kind) {
                (Some(i), _) => view! { <span class="tree-icon"><Icon icon=i class="ic-sm"/></span> }.into_any(),
                (None, Kind::Signal { .. }) => view! { <span class="dot"></span> }.into_any(),
                _ => ().into_any(),
            }}
            <span class="tree-label" class:tree-folder=folder>{highlight(&r.label, filter)}</span>
            {mux_tag.map(|t| view! { <span class="badge badge-accent">{t}</span> })}
            {(r.errors > 0).then(|| view! { <span class="badge badge-err">{r.errors}</span> })}
            {(r.errors == 0 && r.warnings > 0).then(|| view! { <span class="badge badge-warn">{r.warnings}</span> })}
            <span class="tree-meta">{r.meta.clone()}</span>
        </div>
    }
    .into_any()
}

fn highlight(label: &str, filter: &str) -> AnyView {
    let f = filter.to_ascii_lowercase();
    if f.is_empty() {
        return label.to_string().into_any();
    }
    match label.to_ascii_lowercase().find(&f) {
        Some(i) => {
            let (a, rest) = label.split_at(i);
            let (b, c) = rest.split_at(f.len().min(rest.len()));
            view! { {a.to_string()}<mark>{b.to_string()}</mark>{c.to_string()} }.into_any()
        }
        None => label.to_string().into_any(),
    }
}

fn build_rows(
    d: &Dbc,
    filter: &str,
    group: TreeGroup,
    hex: bool,
    expanded: &HashSet<u32>,
    collapsed: &HashSet<String>,
    counts: &BTreeMap<Target, (usize, usize)>,
) -> Vec<Row> {
    let mut rows = Vec::new();
    let count = |t: &Target| counts.get(t).copied().unwrap_or_default();
    let net = count(&Target::Network);
    rows.push(Row {
        key: "net".into(),
        depth: 0,
        kind: Kind::Network,
        label: "Network overview".into(),
        meta: String::new(),
        sel: Some(Sel::Network),
        expandable: false,
        expanded: false,
        card: None,
        errors: net.0,
        warnings: net.1,
    });

    // Nodes.
    let nodes: Vec<_> = d
        .nodes
        .iter()
        .filter(|n| matches(filter, &n.name))
        .collect();
    rows.push(section("Nodes", nodes.len(), "node"));
    if !collapsed.contains("sec:nodes") {
        for n in nodes {
            let (e, w) = count(&Target::Node(n.name.clone()));
            let tx = d
                .messages
                .iter()
                .filter(|m| m.transmitter == n.name)
                .count();
            rows.push(Row {
                key: format!("node:{}", n.name),
                depth: 0,
                kind: Kind::Node,
                label: n.name.clone(),
                meta: if tx > 0 {
                    format!("{tx} tx")
                } else {
                    String::new()
                },
                sel: Some(Sel::Node(n.name.clone())),
                expandable: false,
                expanded: false,
                card: Some(Card::Node(n.name.clone())),
                errors: e,
                warnings: w,
            });
        }
    }

    // Messages.
    let mut msgs: Vec<&Message> = d
        .messages
        .iter()
        .filter(|m| {
            message_matches(filter, m, hex)
                || m.signals
                    .iter()
                    .any(|s| matches(filter, &s.name) || matches(filter, &s.unit))
        })
        .collect();
    match group {
        TreeGroup::Name => msgs.sort_by_key(|m| m.name.to_ascii_lowercase()),
        _ => msgs.sort_by_key(|m| (m.is_extended(), m.can_id())),
    }
    rows.push(section("Messages", msgs.len(), "message"));

    let folder_of = |m: &Message| -> Option<String> {
        match group {
            TreeGroup::Transmitter => Some(m.transmitter.clone()),
            TreeGroup::Group => Some(
                m.attribute(GROUP_ATTR)
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or("Ungrouped")
                    .to_string(),
            ),
            _ => None,
        }
    };

    let mut current_folders: Vec<String> = Vec::new();
    if matches!(group, TreeGroup::Transmitter | TreeGroup::Group) {
        msgs.sort_by_key(|m| {
            (
                folder_of(m).unwrap_or_default() == "Ungrouped",
                folder_of(m),
                m.is_extended(),
                m.can_id(),
            )
        });
    }
    for m in msgs {
        let mut depth = 0;
        if let Some(path) = folder_of(m) {
            // Folders can nest with '/'.
            let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
            let mut hidden = false;
            for (i, part) in parts.iter().enumerate() {
                let key = format!("folder:{}", parts[..=i].join("/"));
                if current_folders.get(i) != Some(&key) {
                    current_folders.truncate(i);
                    current_folders.push(key.clone());
                    if !hidden {
                        rows.push(Row {
                            key: key.clone(),
                            depth: i,
                            kind: Kind::Folder(part.to_string()),
                            label: part.to_string(),
                            meta: String::new(),
                            sel: None,
                            expandable: true,
                            expanded: !collapsed.contains(&key),
                            card: None,
                            errors: 0,
                            warnings: 0,
                        });
                    }
                }
                if collapsed.contains(&key) {
                    hidden = true;
                }
            }
            if hidden {
                continue;
            }
            depth = parts.len();
        }
        let (e, w) = count(&Target::Message(m.id));
        let sig_match = !filter.is_empty() && m.signals.iter().any(|s| matches(filter, &s.name));
        let open = expanded.contains(&m.id) || sig_match;
        rows.push(Row {
            key: format!("msg:{}", m.id),
            depth,
            kind: Kind::Message {
                mux: m.is_multiplexed(),
                ext: m.is_extended(),
            },
            label: m.name.clone(),
            meta: fmt_id(m, hex),
            sel: Some(Sel::Message(m.id)),
            expandable: !m.signals.is_empty(),
            expanded: open,
            card: Some(Card::Message(m.id)),
            errors: e,
            warnings: w,
        });
        if !open {
            continue;
        }
        let only_matching = sig_match && !message_matches(filter, m, hex);
        let sig_row = |i: usize, depth: usize| -> Row {
            let s = &m.signals[i];
            let (e, w) = count(&Target::Signal(m.id, s.name.clone()));
            Row {
                key: format!("sig:{}:{}", m.id, s.name),
                depth,
                kind: Kind::Signal { hue: i, mux: s.mux },
                label: s.name.clone(),
                meta: format!("{}|{}", s.start_bit, s.size),
                sel: Some(Sel::Signal(m.id, s.name.clone())),
                expandable: false,
                expanded: false,
                card: Some(Card::Signal(m.id, s.name.clone())),
                errors: e,
                warnings: w,
            }
        };
        let visible = |i: usize| !only_matching || matches(filter, &m.signals[i].name);
        // Plain signals and switches first, then one folder per mux value.
        let mut branches: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
        let top = m.multiplexor().map(|x| x.name.clone());
        for (i, s) in m.signals.iter().enumerate() {
            // Follow nested switches up to the top-level one, so deeper branches land in its folder.
            let sel = top.as_ref().and_then(|t| {
                candy_dbc::layout::conditions(m, s)
                    .into_iter()
                    .find(|(sw, _)| sw == t)
            });
            match sel {
                Some((_, ranges)) if s.mux != Mux::Multiplexor => {
                    branches
                        .entry(ranges.first().map(|r| r.0).unwrap_or(0))
                        .or_default()
                        .push(i);
                }
                _ => {
                    if visible(i) {
                        rows.push(sig_row(i, depth + 1));
                    }
                }
            }
        }
        let switch = m.multiplexor().map(|s| s.name.clone()).unwrap_or_default();
        for (v, idxs) in branches {
            let idxs: Vec<usize> = idxs.into_iter().filter(|i| visible(*i)).collect();
            if idxs.is_empty() {
                continue;
            }
            let key = format!("branch:{}:{v}", m.id);
            let label = m
                .multiplexor()
                .and_then(|s| s.value_label(v as i64))
                .map(|l| format!("{switch} = {v} · {l}"))
                .unwrap_or_else(|| format!("{switch} = {v}"));
            let open = !collapsed.contains(&key);
            rows.push(Row {
                key: key.clone(),
                depth: depth + 1,
                kind: Kind::Branch(label.clone()),
                label,
                meta: format!("{}", idxs.len()),
                sel: None,
                expandable: true,
                expanded: open,
                card: None,
                errors: 0,
                warnings: 0,
            });
            if open {
                for i in idxs {
                    rows.push(sig_row(i, depth + 2));
                }
            }
        }
    }

    // Value tables.
    let vts: Vec<_> = d
        .value_tables
        .iter()
        .filter(|v| matches(filter, &v.name))
        .collect();
    rows.push(section("Value tables", vts.len(), "value table"));
    for v in vts {
        let (e, w) = count(&Target::ValueTable(v.name.clone()));
        rows.push(Row {
            key: format!("vt:{}", v.name),
            depth: 0,
            kind: Kind::ValueTable,
            label: v.name.clone(),
            meta: format!("{}", v.entries.len()),
            sel: Some(Sel::ValueTable(v.name.clone())),
            expandable: false,
            expanded: false,
            card: None,
            errors: e,
            warnings: w,
        });
    }
    rows
}

fn section(title: &'static str, count: usize, add: &'static str) -> Row {
    Row {
        key: format!("sec:{}", title.to_ascii_lowercase()),
        depth: 0,
        kind: Kind::Section(title, count, add),
        label: title.into(),
        meta: String::new(),
        sel: None,
        expandable: false,
        expanded: true,
        card: None,
        errors: 0,
        warnings: 0,
    }
}
