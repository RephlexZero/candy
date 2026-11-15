//! Command palette (Ctrl+K): fuzzy search over actions and everything in the network.

use leptos::prelude::*;

use crate::actions::{self, THEMES};
use crate::icons::{Ic, Icon};
use crate::state::{DockTab, Modal, Sel, use_app};

#[derive(Clone, PartialEq)]
enum Act {
    Jump(Sel),
    Cmd(&'static str),
    Theme(&'static str),
}

#[derive(Clone, PartialEq)]
struct Item {
    group: &'static str,
    label: String,
    hint: String,
    icon: Ic,
    act: Act,
}

const COMMANDS: &[(&str, &str, Ic, &str)] = &[
    ("add-message", "Add message", Ic::Plus, "M"),
    (
        "add-signal",
        "Add signal to selected message",
        Ic::Plus,
        "N",
    ),
    ("add-node", "Add node", Ic::Plus, ""),
    ("add-vt", "Add value table", Ic::Plus, ""),
    ("fix-all", "Fix all auto-fixable problems", Ic::Wand, ""),
    ("naming", "Choose naming convention…", Ic::Tag, ""),
    ("problems", "Show problems", Ic::Warning, "P"),
    ("decode", "Open decode playground", Ic::Zap, "X"),
    ("diff", "Diff & merge", Ic::Diff, ""),
    ("raw", "Show DBC source", Ic::Code, "R"),
    ("save", "Save", Ic::Save, "Ctrl S"),
    ("save-as", "Save as…", Ic::Download, "Ctrl ⇧ S"),
    ("open", "Open file…", Ic::Open, "Ctrl O"),
    ("new", "New network", Ic::FilePlus, ""),
    ("hex", "Toggle hex / decimal ids", Ic::Hash, "H"),
    ("dock", "Toggle bottom panel", Ic::Panel, "Ctrl J"),
    ("shortcuts", "Keyboard shortcuts", Ic::Keyboard, "?"),
    ("duplicate", "Duplicate selection", Ic::Copy, "Ctrl D"),
    ("delete", "Delete selection", Ic::Trash, "Del"),
    ("about", "About CANdy", Ic::Info, ""),
];

/// Subsequence match score (higher is better); None if not all query chars appear in order.
fn score(query: &str, text: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let mut s = 0;
    let mut ti = 0;
    let mut last = None;
    for qc in query.to_lowercase().chars() {
        let mut found = false;
        while ti < t.len() {
            if t[ti] == qc {
                s += 10;
                if last == Some(ti.wrapping_sub(1)) {
                    s += 15;
                }
                if ti == 0 || !t[ti - 1].is_alphanumeric() {
                    s += 8;
                }
                last = Some(ti);
                ti += 1;
                found = true;
                break;
            }
            ti += 1;
        }
        if !found {
            return None;
        }
    }
    if text.to_lowercase().contains(&query.to_lowercase()) {
        s += 40;
    }
    Some(s - t.len() as i32 / 4)
}

fn run(act: Act) {
    let app = use_app();
    app.palette_open.set(false);
    match act {
        Act::Jump(sel) => {
            app.select(sel.clone());
            if let Sel::Signal(id, n) = sel {
                crate::persist::scroll_into_view(&format!("sigrow-{id}-{n}"));
            }
        }
        Act::Theme(t) => actions::set_theme(app, t),
        Act::Cmd(c) => match c {
            "add-message" => actions::add_message(app),
            "add-signal" => actions::add_signal(app, None, None),
            "add-node" => actions::add_node(app),
            "add-vt" => actions::add_value_table(app),
            "fix-all" => actions::fix_all(app, None),
            "naming" => app.modal.set(Some(Modal::Naming)),
            "problems" => actions::open_dock(app, DockTab::Problems),
            "decode" => actions::open_dock(app, DockTab::Decode),
            "diff" => actions::open_dock(app, DockTab::Diff),
            "raw" => actions::open_dock(app, DockTab::Raw),
            "save" => actions::save(app, false),
            "save-as" => actions::save(app, true),
            "open" => actions::open_file(app),
            "new" => app.new_document(),
            "hex" => app.hex_ids.update(|h| *h = !*h),
            "dock" => actions::toggle_dock(app),
            "shortcuts" => app.modal.set(Some(Modal::Shortcuts)),
            "duplicate" => actions::duplicate_selection(app),
            "delete" => actions::delete_selection(app),
            "about" => app.modal.set(Some(Modal::About)),
            _ => {}
        },
    }
}

#[component]
pub fn Palette() -> impl IntoView {
    let app = use_app();
    let query = RwSignal::new(String::new());
    let active = RwSignal::new(0usize);

    let items = Memo::new(move |_| {
        if !app.palette_open.get() {
            return Vec::new();
        }
        let q = query.get();
        let q = q.trim();
        let mut out: Vec<(i32, Item)> = Vec::new();
        let mut push = |s: Option<i32>, item: Item| {
            if let Some(s) = s {
                out.push((s, item));
            }
        };
        for (id, label, icon, key) in COMMANDS {
            push(
                score(q, label),
                Item {
                    group: "Actions",
                    label: label.to_string(),
                    hint: key.to_string(),
                    icon: *icon,
                    act: Act::Cmd(id),
                },
            );
        }
        if !q.is_empty() {
            for (id, name, desc) in THEMES {
                push(
                    score(q, &format!("theme {name}")).map(|s| s - 20),
                    Item {
                        group: "Themes",
                        label: format!("Theme: {name}"),
                        hint: desc.to_string(),
                        icon: Ic::Palette,
                        act: Act::Theme(id),
                    },
                );
            }
        }
        let hex = app.hex_ids.get_untracked();
        app.doc.with(|d| {
            for m in &d.messages {
                let id_s = crate::ui::fmt_id(m, hex);
                let s = score(q, &m.name).or_else(|| score(q, &id_s).map(|s| s + 5));
                push(
                    s.map(|s| s + 4),
                    Item {
                        group: "Messages",
                        label: m.name.clone(),
                        hint: id_s.clone(),
                        icon: Ic::Message,
                        act: Act::Jump(Sel::Message(m.id)),
                    },
                );
                if !q.is_empty() {
                    for sig in &m.signals {
                        let s = score(q, &sig.name).or_else(|| {
                            score(q, &format!("{}.{}", m.name, sig.name)).map(|s| s - 5)
                        });
                        let hint = if sig.unit.is_empty() {
                            m.name.clone()
                        } else {
                            format!("{} · {}", m.name, sig.unit)
                        };
                        push(
                            s,
                            Item {
                                group: "Signals",
                                label: sig.name.clone(),
                                hint,
                                icon: Ic::Signal,
                                act: Act::Jump(Sel::Signal(m.id, sig.name.clone())),
                            },
                        );
                    }
                }
            }
            for n in &d.nodes {
                push(
                    score(q, &n.name),
                    Item {
                        group: "Nodes",
                        label: n.name.clone(),
                        hint: "node".into(),
                        icon: Ic::Node,
                        act: Act::Jump(Sel::Node(n.name.clone())),
                    },
                );
            }
            for t in &d.value_tables {
                push(
                    score(q, &t.name),
                    Item {
                        group: "Value tables",
                        label: t.name.clone(),
                        hint: "value table".into(),
                        icon: Ic::Table,
                        act: Act::Jump(Sel::ValueTable(t.name.clone())),
                    },
                );
            }
        });
        if q.is_empty() {
            // Keep the natural order: actions first, then messages.
            out.truncate(60);
            return out.into_iter().map(|x| x.1).collect();
        }
        out.sort_by_key(|a| std::cmp::Reverse(a.0));
        out.truncate(60);
        out.into_iter().map(|x| x.1).collect::<Vec<_>>()
    });

    Effect::new(move |_| {
        query.track();
        active.set(0);
    });
    Effect::new(move |_| {
        if app.palette_open.get() {
            query.set(String::new());
            set_timeout(
                || crate::persist::focus("palette-input"),
                std::time::Duration::from_millis(10),
            );
        }
    });

    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        let n = items.with_untracked(|i| i.len());
        match ev.key().as_str() {
            "ArrowDown" => {
                ev.prevent_default();
                if n > 0 {
                    active.update(|a| *a = (*a + 1) % n);
                }
                crate::persist::scroll_into_view(&format!("pal-{}", active.get_untracked()));
            }
            "ArrowUp" => {
                ev.prevent_default();
                if n > 0 {
                    active.update(|a| *a = (*a + n - 1) % n);
                }
                crate::persist::scroll_into_view(&format!("pal-{}", active.get_untracked()));
            }
            "Enter" => {
                ev.prevent_default();
                if let Some(it) = items.with_untracked(|i| i.get(active.get_untracked()).cloned()) {
                    run(it.act);
                }
            }
            "Escape" => app.palette_open.set(false),
            _ => {}
        }
    };

    view! {
        <Show when=move || app.palette_open.get()>
            <div class="overlay" on:click=move |_| app.palette_open.set(false)>
                <div class="palette" on:click=|ev| ev.stop_propagation()>
                    <div class="palette-input">
                        <Icon icon=Ic::Search/>
                        <input
                            id="palette-input"
                            placeholder="Jump to a message, signal or node — or run a command"
                            autocomplete="off"
                            spellcheck="false"
                            prop:value=move || query.get()
                            on:input=move |ev| query.set(event_target_value(&ev))
                            on:keydown=on_key
                        />
                    </div>
                    <div class="palette-list">
                        {move || {
                            let list = items.get();
                            if list.is_empty() {
                                return view! { <div class="empty-state">"Nothing matches."</div> }.into_any();
                            }
                            let mut last_group = "";
                            list.into_iter()
                                .enumerate()
                                .map(|(i, it)| {
                                    let header = (it.group != last_group).then(|| view! { <div class="pal-group eyebrow">{it.group}</div> });
                                    last_group = it.group;
                                    let act = it.act.clone();
                                    view! {
                                        {header}
                                        <div
                                            class="pal-item"
                                            id=format!("pal-{i}")
                                            class:on=move || active.get() == i
                                            on:mouseenter=move |_| active.set(i)
                                            on:click=move |_| run(act.clone())
                                        >
                                            <span class="tree-icon"><Icon icon=it.icon class="ic-sm"/></span>
                                            <span class="grow mono">{it.label}</span>
                                            <span class="hint">{it.hint}</span>
                                        </div>
                                    }
                                })
                                .collect_view()
                                .into_any()
                        }}
                    </div>
                    <div class="palette-foot">
                        <span><span class="kbd">"↑"</span><span class="kbd">"↓"</span>" navigate"</span>
                        <span><span class="kbd">"↵"</span>" open"</span>
                        <span><span class="kbd">"Esc"</span>" close"</span>
                    </div>
                </div>
            </div>
        </Show>
    }
}
