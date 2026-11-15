//! User actions shared by the toolbar, command palette and keyboard shortcuts.

use candy_dbc::layout;
use candy_dbc::lint;
use candy_dbc::naming::{self, Convention};
use candy_dbc::{AttrKind, AttrObject, AttrValue, Message, Signal, set_attribute};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::examples::EXAMPLES;
use crate::persist;
use crate::state::{AppState, DockTab, Sel, ToastKind};

pub const THEMES: &[(&str, &str, &str)] = &[
    ("system", "System", "Follows your OS light/dark setting"),
    ("stone-light", "Stone Light", "Warm paper and amber"),
    ("stone-dark", "Stone Dark", "Warm charcoal and amber"),
    ("midnight", "Midnight", "Deep navy with sky-blue accents"),
    ("paddock", "Paddock", "Carbon black and racing red"),
    ("phosphor", "Phosphor", "Oscilloscope green on black"),
    (
        "solarized-light",
        "Solarized Light",
        "Ethan Schoonover's classic",
    ),
    (
        "solarized-dark",
        "Solarized Dark",
        "Ethan Schoonover's classic",
    ),
    ("contrast", "High Contrast", "Maximum legibility"),
];

pub fn set_theme(app: AppState, id: &str) {
    app.theme.set(id.to_string());
    persist::set("candy.theme", id);
    persist::set_root_attr("data-theme", id);
}

pub fn open_file(app: AppState) {
    if persist::native_fs() {
        leptos::task::spawn_local(async move {
            match persist::open_native().await {
                Ok(Some((name, text))) => app.load(&text, &name),
                Ok(None) => {}
                Err(e) => app.toast(format!("Could not open file: {e}"), ToastKind::Error),
            }
        });
        return;
    }
    // Fallback: hidden file input.
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Ok(input) = doc.create_element("input") else {
        return;
    };
    let input: web_sys::HtmlInputElement = input.unchecked_into();
    input.set_type("file");
    input.set_accept(".dbc,text/plain");
    let inp = input.clone();
    let cb = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
        if let Some(file) = inp.files().and_then(|f| f.get(0)) {
            open_web_file(app, file);
        }
    });
    input.set_onchange(Some(cb.as_ref().unchecked_ref()));
    cb.forget();
    input.click();
}

pub fn open_web_file(app: AppState, file: web_sys::File) {
    persist::forget_handle();
    leptos::task::spawn_local(async move {
        let name = file.name();
        match persist::read_file(&file).await {
            Some(text) => app.load(&text, &name),
            None => app.toast(format!("Could not read {name}"), ToastKind::Error),
        }
    });
}

pub fn save(app: AppState, save_as: bool) {
    let text = app.doc.with_untracked(candy_dbc::write);
    let name = app.file_name.get_untracked();
    if persist::native_fs() {
        leptos::task::spawn_local(async move {
            match persist::save_native(&name, &text, save_as).await {
                Ok(Some(saved)) => {
                    app.file_name.set(saved.clone());
                    app.dirty.set(false);
                    app.toast(format!("Saved {saved}"), ToastKind::Success);
                }
                Ok(None) => {}
                Err(e) => app.toast(format!("Save failed: {e}"), ToastKind::Error),
            }
        });
    } else {
        persist::download(&name, &text, "text/plain");
        app.dirty.set(false);
        app.toast(format!("Downloaded {name}"), ToastKind::Success);
    }
}

pub fn load_example(app: AppState, idx: usize) {
    if let Some(ex) = EXAMPLES.get(idx) {
        persist::forget_handle();
        app.load(ex.text, ex.file);
    }
}

/// Add a message with a free id and sensible defaults; select it.
pub fn add_message(app: AppState) {
    let mut new_id = 0;
    app.edit("add message", |d| {
        let after = d
            .messages
            .iter()
            .filter(|m| !m.is_extended())
            .map(|m| m.can_id() + 1)
            .max()
            .unwrap_or(0x100);
        new_id = d.free_id(after, false);
        let mut n = d.messages.len() + 1;
        let mut name = format!("NewMessage_{n}");
        while d.message_by_name(&name).is_some() {
            n += 1;
            name = format!("NewMessage_{n}");
        }
        let transmitter = d
            .nodes
            .first()
            .map(|n| n.name.clone())
            .unwrap_or_else(|| candy_dbc::NO_NODE.into());
        d.messages.push(Message {
            id: new_id,
            name,
            size: 8,
            transmitter,
            ..Default::default()
        });
    });
    app.select(Sel::Message(new_id));
}

/// Add a signal to the selected message in the first free bits (or at `at`).
pub fn add_signal(app: AppState, msg_id: Option<u32>, at: Option<(u32, u32)>) {
    let Some(id) = msg_id.or_else(|| app.sel.get_untracked().message_id()) else {
        app.toast("Select a message first".into(), ToastKind::Info);
        return;
    };
    let branch = app.mux_branch.get_untracked();
    let mut created = None;
    app.edit("add signal", |d| {
        let Some(m) = d.message_mut(id) else { return };
        let (start, size) = match at {
            Some(x) => x,
            None => {
                let size = 8.min(m.size * 8).max(1);
                match layout::find_free_bits(m, size, branch)
                    .or_else(|| layout::find_free_bits(m, 1, branch))
                {
                    Some(s) => (s, size.min(m.size * 8 - s)),
                    None => (0, 1),
                }
            }
        };
        let mut n = m.signals.len() + 1;
        let mut name = format!("NewSignal_{n}");
        while m.signal(&name).is_some() {
            n += 1;
            name = format!("NewSignal_{n}");
        }
        let mux = match (branch, m.multiplexor()) {
            (Some(b), Some(_)) => candy_dbc::Mux::Multiplexed(b),
            _ => candy_dbc::Mux::None,
        };
        let max = 2f64.powi(size as i32) - 1.0;
        m.signals.push(Signal {
            name: name.clone(),
            start_bit: start,
            size,
            mux,
            max,
            ..Default::default()
        });
        created = Some(name);
    });
    if let Some(name) = created {
        app.select(Sel::Signal(id, name));
    }
}

pub fn add_node(app: AppState) {
    let mut name = String::new();
    app.edit("add node", |d| {
        let mut n = d.nodes.len() + 1;
        name = format!("NODE_{n}");
        while d.node(&name).is_some() {
            n += 1;
            name = format!("NODE_{n}");
        }
        d.nodes.push(candy_dbc::Node {
            name: name.clone(),
            ..Default::default()
        });
    });
    app.select(Sel::Node(name));
}

pub fn add_value_table(app: AppState) {
    let mut name = String::new();
    app.edit("add value table", |d| {
        let mut n = d.value_tables.len() + 1;
        name = format!("VT_Table{n}");
        while d.value_tables.iter().any(|v| v.name == name) {
            n += 1;
            name = format!("VT_Table{n}");
        }
        d.value_tables.push(candy_dbc::ValueTable {
            name: name.clone(),
            entries: vec![
                candy_dbc::ValueEntry::new(0, "Off"),
                candy_dbc::ValueEntry::new(1, "On"),
            ],
        });
    });
    app.select(Sel::ValueTable(name));
}

pub fn delete_selection(app: AppState) {
    match app.sel.get_untracked() {
        Sel::Signal(id, name) => {
            app.edit(&format!("delete {name}"), |d| d.remove_signal(id, &name));
            app.select(Sel::Message(id));
        }
        Sel::Message(id) => {
            let name = app
                .doc
                .with_untracked(|d| d.message(id).map(|m| m.name.clone()))
                .unwrap_or_default();
            app.edit(&format!("delete {name}"), |d| d.remove_message(id));
            app.select(Sel::Network);
        }
        Sel::Node(n) => {
            app.edit(&format!("delete node {n}"), |d| {
                d.nodes.retain(|x| x.name != n)
            });
            app.select(Sel::Network);
        }
        Sel::ValueTable(n) => {
            app.edit(&format!("delete {n}"), |d| {
                d.value_tables.retain(|x| x.name != n)
            });
            app.select(Sel::Network);
        }
        Sel::Network => {}
    }
}

pub fn duplicate_selection(app: AppState) {
    match app.sel.get_untracked() {
        Sel::Signal(id, name) => {
            let mut new_name = String::new();
            app.edit(&format!("duplicate {name}"), |d| {
                let branch = app.mux_branch.get_untracked();
                let Some(m) = d.message_mut(id) else { return };
                let Some(mut s) = m.signal(&name).cloned() else {
                    return;
                };
                let mut k = 2;
                new_name = format!("{name}_{k}");
                while m.signal(&new_name).is_some() {
                    k += 1;
                    new_name = format!("{name}_{k}");
                }
                s.name = new_name.clone();
                if s.byte_order == candy_dbc::ByteOrder::LittleEndian
                    && let Some(start) = layout::find_free_bits(m, s.size, branch)
                {
                    s.start_bit = start;
                }
                m.signals.push(s);
            });
            app.select(Sel::Signal(id, new_name));
        }
        Sel::Message(id) => {
            let mut new_id = 0;
            app.edit("duplicate message", |d| {
                let Some(mut m) = d.message(id).cloned() else {
                    return;
                };
                new_id = d.free_id(m.can_id() + 1, m.is_extended());
                m.id = new_id;
                m.name = format!("{}_Copy", m.name);
                d.messages.push(m);
            });
            app.select(Sel::Message(new_id));
        }
        _ => {}
    }
}

pub fn set_convention(app: AppState, conv: Option<&Convention>) {
    app.edit("change naming convention", |d| match conv {
        Some(c) => {
            d.ensure_attr_def(
                AttrObject::Network,
                naming::CONVENTION_ATTR,
                AttrKind::String,
                AttrValue::Str(String::new()),
            );
            set_attribute(
                &mut d.attributes,
                naming::CONVENTION_ATTR,
                Some(AttrValue::Str(naming::to_attr_string(c))),
            );
        }
        None => set_attribute(&mut d.attributes, naming::CONVENTION_ATTR, None),
    });
    match conv {
        Some(c) => app.toast(format!("Naming convention: {}", c.name), ToastKind::Success),
        None => app.toast("Naming convention turned off".into(), ToastKind::Info),
    }
}

pub fn fix_all(app: AppState, rule: Option<&'static str>) {
    let issues = app.issues.get_untracked();
    let fixable = issues
        .iter()
        .filter(|i| i.fix.is_some() && rule.is_none_or(|r| i.rule == r))
        .count();
    if fixable == 0 {
        app.toast("Nothing to fix automatically".into(), ToastKind::Info);
        return;
    }
    let mut n = 0;
    app.edit("fix all", |d| n = lint::fix_all(d, &issues, rule));
    app.fix_selection();
    app.toast(
        format!("Applied {n} fix{}", if n == 1 { "" } else { "es" }),
        ToastKind::Success,
    );
}

pub fn open_dock(app: AppState, tab: DockTab) {
    app.dock_tab.set(tab);
    app.dock_open.set(true);
    persist::set("candy.dock", "1");
}

pub fn toggle_dock(app: AppState) {
    let open = !app.dock_open.get_untracked();
    app.dock_open.set(open);
    persist::set("candy.dock", if open { "1" } else { "0" });
}

/// Ensure a message attribute definition exists before setting a value.
pub fn ensure_msg_attr(d: &mut candy_dbc::Dbc, name: &str, kind: AttrKind) {
    let default = match &kind {
        AttrKind::String => AttrValue::Str(String::new()),
        AttrKind::Float { .. } => AttrValue::Float(0.0),
        _ => AttrValue::Int(0),
    };
    d.ensure_attr_def(AttrObject::Message, name, kind, default);
}

pub const GROUP_ATTR: &str = "CANdy_Group";
