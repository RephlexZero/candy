//! Structural diff between two DBC files, and cherry-pick merging.
//!
//! Messages are matched by id, signals by name within a message, everything else by name.
//! Each [`Change`] is self-contained so the UI can apply any subset of them.

use crate::model::*;
use crate::writer::signal_line;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Removed,
    Modified,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Node(String),
    Message(u32),
    Signal(u32, String),
    ValueTable(String),
    AttrDef(String),
    Network,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub kind: ChangeKind,
    pub item: Item,
    /// Short label, e.g. `EngineData.EngSpeed`.
    pub label: String,
    /// Field-level details for modifications: (field, old, new).
    pub details: Vec<(String, String, String)>,
}

fn opt(s: &Option<String>) -> String {
    s.clone().unwrap_or_default()
}

fn attrs(a: &[Attribute]) -> String {
    a.iter().map(|a| format!("{}={}", a.name, a.value.display())).collect::<Vec<_>>().join(", ")
}

fn vals(v: &[ValueEntry]) -> String {
    v.iter().map(|e| format!("{}=\"{}\"", e.value, e.label)).collect::<Vec<_>>().join(", ")
}

fn field(out: &mut Vec<(String, String, String)>, name: &str, a: String, b: String) {
    if a != b {
        out.push((name.to_string(), a, b));
    }
}

fn message_details(a: &Message, b: &Message) -> Vec<(String, String, String)> {
    let mut d = Vec::new();
    field(&mut d, "name", a.name.clone(), b.name.clone());
    field(&mut d, "length", a.size.to_string(), b.size.to_string());
    field(&mut d, "transmitter", a.transmitter.clone(), b.transmitter.clone());
    field(&mut d, "extra transmitters", a.extra_transmitters.join(","), b.extra_transmitters.join(","));
    field(&mut d, "comment", opt(&a.comment), opt(&b.comment));
    field(&mut d, "attributes", attrs(&a.attributes), attrs(&b.attributes));
    d
}

fn signal_details(a: &Signal, b: &Signal) -> Vec<(String, String, String)> {
    let mut d = Vec::new();
    field(&mut d, "definition", signal_line(a), signal_line(b));
    field(&mut d, "value type", format!("{:?}", a.value_type), format!("{:?}", b.value_type));
    field(&mut d, "comment", opt(&a.comment), opt(&b.comment));
    field(&mut d, "attributes", attrs(&a.attributes), attrs(&b.attributes));
    field(&mut d, "values", vals(&a.value_descriptions), vals(&b.value_descriptions));
    field(
        &mut d,
        "extended mux",
        format!("{:?}", a.ext_mux),
        format!("{:?}", b.ext_mux),
    );
    d
}

/// Changes needed to turn `left` into `right`.
pub fn diff(left: &Dbc, right: &Dbc) -> Vec<Change> {
    let mut out = Vec::new();

    // Network.
    let mut nd = Vec::new();
    field(&mut nd, "version", left.version.clone(), right.version.clone());
    field(&mut nd, "comment", opt(&left.comment), opt(&right.comment));
    field(&mut nd, "attributes", attrs(&left.attributes), attrs(&right.attributes));
    if !nd.is_empty() {
        out.push(Change { kind: ChangeKind::Modified, item: Item::Network, label: "Network".into(), details: nd });
    }

    // Nodes.
    for n in &right.nodes {
        match left.node(&n.name) {
            None => out.push(Change { kind: ChangeKind::Added, item: Item::Node(n.name.clone()), label: n.name.clone(), details: vec![] }),
            Some(l) => {
                let mut d = Vec::new();
                field(&mut d, "comment", opt(&l.comment), opt(&n.comment));
                field(&mut d, "attributes", attrs(&l.attributes), attrs(&n.attributes));
                if !d.is_empty() {
                    out.push(Change { kind: ChangeKind::Modified, item: Item::Node(n.name.clone()), label: n.name.clone(), details: d });
                }
            }
        }
    }
    for n in &left.nodes {
        if right.node(&n.name).is_none() {
            out.push(Change { kind: ChangeKind::Removed, item: Item::Node(n.name.clone()), label: n.name.clone(), details: vec![] });
        }
    }

    // Value tables.
    for v in &right.value_tables {
        match left.value_tables.iter().find(|x| x.name == v.name) {
            None => out.push(Change { kind: ChangeKind::Added, item: Item::ValueTable(v.name.clone()), label: v.name.clone(), details: vec![] }),
            Some(l) if l != v => out.push(Change {
                kind: ChangeKind::Modified,
                item: Item::ValueTable(v.name.clone()),
                label: v.name.clone(),
                details: vec![("values".into(), vals(&l.entries), vals(&v.entries))],
            }),
            _ => {}
        }
    }
    for v in &left.value_tables {
        if !right.value_tables.iter().any(|x| x.name == v.name) {
            out.push(Change { kind: ChangeKind::Removed, item: Item::ValueTable(v.name.clone()), label: v.name.clone(), details: vec![] });
        }
    }

    // Attribute definitions.
    for a in &right.attr_defs {
        match left.attr_def(&a.name) {
            None => out.push(Change { kind: ChangeKind::Added, item: Item::AttrDef(a.name.clone()), label: a.name.clone(), details: vec![] }),
            Some(l) if l != a || left.attr_default(&a.name) != right.attr_default(&a.name) => out.push(Change {
                kind: ChangeKind::Modified,
                item: Item::AttrDef(a.name.clone()),
                label: a.name.clone(),
                details: vec![(
                    "definition".into(),
                    format!("{:?} default {:?}", l.kind, left.attr_default(&a.name)),
                    format!("{:?} default {:?}", a.kind, right.attr_default(&a.name)),
                )],
            }),
            _ => {}
        }
    }
    for a in &left.attr_defs {
        if right.attr_def(&a.name).is_none() {
            out.push(Change { kind: ChangeKind::Removed, item: Item::AttrDef(a.name.clone()), label: a.name.clone(), details: vec![] });
        }
    }

    // Messages and signals.
    for m in &right.messages {
        match left.message(m.id) {
            None => out.push(Change {
                kind: ChangeKind::Added,
                item: Item::Message(m.id),
                label: format!("{} ({})", m.name, m.id_hex()),
                details: vec![("signals".into(), String::new(), m.signals.len().to_string())],
            }),
            Some(l) => {
                let d = message_details(l, m);
                if !d.is_empty() {
                    out.push(Change { kind: ChangeKind::Modified, item: Item::Message(m.id), label: format!("{} ({})", m.name, m.id_hex()), details: d });
                }
                for s in &m.signals {
                    let label = format!("{}.{}", m.name, s.name);
                    match l.signal(&s.name) {
                        None => out.push(Change { kind: ChangeKind::Added, item: Item::Signal(m.id, s.name.clone()), label, details: vec![] }),
                        Some(ls) => {
                            let d = signal_details(ls, s);
                            if !d.is_empty() {
                                out.push(Change { kind: ChangeKind::Modified, item: Item::Signal(m.id, s.name.clone()), label, details: d });
                            }
                        }
                    }
                }
                for s in &l.signals {
                    if m.signal(&s.name).is_none() {
                        out.push(Change {
                            kind: ChangeKind::Removed,
                            item: Item::Signal(m.id, s.name.clone()),
                            label: format!("{}.{}", l.name, s.name),
                            details: vec![],
                        });
                    }
                }
            }
        }
    }
    for m in &left.messages {
        if right.message(m.id).is_none() {
            out.push(Change { kind: ChangeKind::Removed, item: Item::Message(m.id), label: format!("{} ({})", m.name, m.id_hex()), details: vec![] });
        }
    }
    out
}

/// Apply one change (computed by `diff(base, other)`) to `base`, copying state from `other`.
pub fn apply(base: &mut Dbc, other: &Dbc, change: &Change) {
    match (&change.item, change.kind) {
        (Item::Network, _) => {
            base.version = other.version.clone();
            base.comment = other.comment.clone();
            base.attributes = other.attributes.clone();
        }
        (Item::Node(n), ChangeKind::Removed) => base.nodes.retain(|x| &x.name != n),
        (Item::Node(n), _) => {
            if let Some(src) = other.node(n).cloned() {
                match base.nodes.iter_mut().find(|x| &x.name == n) {
                    Some(dst) => *dst = src,
                    None => base.nodes.push(src),
                }
            }
        }
        (Item::ValueTable(n), ChangeKind::Removed) => base.value_tables.retain(|x| &x.name != n),
        (Item::ValueTable(n), _) => {
            if let Some(src) = other.value_tables.iter().find(|x| &x.name == n).cloned() {
                match base.value_tables.iter_mut().find(|x| &x.name == n) {
                    Some(dst) => *dst = src,
                    None => base.value_tables.push(src),
                }
            }
        }
        (Item::AttrDef(n), ChangeKind::Removed) => {
            base.attr_defs.retain(|x| &x.name != n);
            base.attr_defaults.retain(|x| &x.name != n);
        }
        (Item::AttrDef(n), _) => {
            if let Some(src) = other.attr_def(n).cloned() {
                match base.attr_defs.iter_mut().find(|x| &x.name == n) {
                    Some(dst) => *dst = src,
                    None => base.attr_defs.push(src),
                }
            }
            base.attr_defaults.retain(|x| &x.name != n);
            base.attr_defaults.extend(other.attr_defaults.iter().filter(|x| &x.name == n).cloned());
        }
        (Item::Message(id), ChangeKind::Removed) => base.remove_message(*id),
        (Item::Message(id), ChangeKind::Added) => {
            if let Some(src) = other.message(*id).cloned() {
                base.messages.push(src);
                base.signal_groups.extend(other.signal_groups.iter().filter(|g| g.message_id == *id).cloned());
            }
        }
        (Item::Message(id), ChangeKind::Modified) => {
            if let (Some(src), Some(dst)) = (other.message(*id), base.message_mut(*id)) {
                dst.name = src.name.clone();
                dst.size = src.size;
                dst.transmitter = src.transmitter.clone();
                dst.extra_transmitters = src.extra_transmitters.clone();
                dst.comment = src.comment.clone();
                dst.attributes = src.attributes.clone();
            }
        }
        (Item::Signal(id, s), ChangeKind::Removed) => base.remove_signal(*id, s),
        (Item::Signal(id, s), _) => {
            if let Some(src) = other.message(*id).and_then(|m| m.signal(s)).cloned()
                && let Some(dst) = base.message_mut(*id)
            {
                match dst.signal_mut(s) {
                    Some(d) => *d = src,
                    None => dst.signals.push(src),
                }
            }
        }
    }
}

/// Merge: apply the selected changes (by index into `diff(base, other)`).
pub fn merge(base: &Dbc, other: &Dbc, selected: &[usize]) -> Dbc {
    let changes = diff(base, other);
    let mut out = base.clone();
    for &i in selected {
        if let Some(c) = changes.get(i) {
            apply(&mut out, other, c);
        }
    }
    out
}
