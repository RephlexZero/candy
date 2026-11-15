//! Global application state, shared through Leptos context.
//!
//! Every field is a signal, so `AppState` is `Copy` and can be captured freely by closures.
//! All document edits go through [`AppState::edit`], which records undo history.

use candy_dbc::lint::{self, Issue, Target};
use candy_dbc::naming::{self, Convention};
use candy_dbc::{Dbc, ParseError};
use leptos::prelude::*;

use crate::persist;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Sel {
    Network,
    Node(String),
    Message(u32),
    Signal(u32, String),
    ValueTable(String),
}

impl Sel {
    pub fn message_id(&self) -> Option<u32> {
        match self {
            Sel::Message(id) | Sel::Signal(id, _) => Some(*id),
            _ => None,
        }
    }
    pub fn from_target(t: &Target) -> Sel {
        match t {
            Target::Network => Sel::Network,
            Target::Node(n) => Sel::Node(n.clone()),
            Target::Message(id) | Target::SignalGroup(id, _) => Sel::Message(*id),
            Target::Signal(id, s) | Target::ValueLabel(id, s, _) => Sel::Signal(*id, s.clone()),
            Target::ValueTable(v) => Sel::ValueTable(v.clone()),
            Target::EnvVar(_) => Sel::Network,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockTab {
    Problems,
    Decode,
    Diff,
    Raw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeGroup {
    Id,
    Transmitter,
    Group,
    Name,
}

impl TreeGroup {
    pub const ALL: [TreeGroup; 4] = [
        TreeGroup::Id,
        TreeGroup::Transmitter,
        TreeGroup::Group,
        TreeGroup::Name,
    ];
    pub fn label(self) -> &'static str {
        match self {
            TreeGroup::Id => "By ID",
            TreeGroup::Transmitter => "By sender",
            TreeGroup::Group => "By folder",
            TreeGroup::Name => "A–Z",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Modal {
    Naming,
    Shortcuts,
    About,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub text: String,
    pub kind: ToastKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

/// What a hover card shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Card {
    Signal(u32, String),
    Message(u32),
    Node(String),
    Rule(&'static str),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hover {
    pub card: Card,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy)]
pub struct AppState {
    pub doc: RwSignal<Dbc>,
    pub file_name: RwSignal<String>,
    /// The document as it was when opened, for "changes since open".
    pub baseline: RwSignal<Dbc>,
    pub undo: StoredValue<Vec<(String, Dbc)>>,
    pub redo: StoredValue<Vec<(String, Dbc)>>,
    pub history_len: RwSignal<(usize, usize)>,
    /// Key of the last coalescible edit and when it happened (ms).
    pub last_edit: StoredValue<(String, f64)>,
    pub sel: RwSignal<Sel>,
    pub mux_branch: RwSignal<Option<u64>>,
    pub parse_errors: RwSignal<Vec<ParseError>>,
    pub dock_tab: RwSignal<DockTab>,
    pub dock_open: RwSignal<bool>,
    pub palette_open: RwSignal<bool>,
    pub modal: RwSignal<Option<Modal>>,
    pub toasts: RwSignal<Vec<Toast>>,
    pub tree_group: RwSignal<TreeGroup>,
    pub tree_filter: RwSignal<String>,
    pub expanded: RwSignal<std::collections::HashSet<u32>>,
    pub theme: RwSignal<String>,
    pub hex_ids: RwSignal<bool>,
    pub hover: RwSignal<Option<Hover>>,
    pub dirty: RwSignal<bool>,
    pub has_doc: RwSignal<bool>,
    pub convention: Memo<Option<Convention>>,
    pub issues: Memo<Vec<Issue>>,
    /// Signal → colour index, stable per message.
    pub decode_input: RwSignal<String>,
    pub diff_other: RwSignal<Option<(String, Dbc)>>,
}

impl AppState {
    pub fn new() -> Self {
        let doc = RwSignal::new(Dbc::new());
        let convention = Memo::new(move |_| doc.with(naming::from_dbc));
        let issues = Memo::new(move |_| {
            let conv = convention.get();
            doc.with(|d| lint::lint(d, conv.as_ref()))
        });
        Self {
            doc,
            file_name: RwSignal::new("untitled.dbc".into()),
            baseline: RwSignal::new(Dbc::new()),
            undo: StoredValue::new(Vec::new()),
            redo: StoredValue::new(Vec::new()),
            history_len: RwSignal::new((0, 0)),
            last_edit: StoredValue::new((String::new(), 0.0)),
            sel: RwSignal::new(Sel::Network),
            mux_branch: RwSignal::new(None),
            parse_errors: RwSignal::new(Vec::new()),
            dock_tab: RwSignal::new(DockTab::Problems),
            dock_open: RwSignal::new(persist::get("candy.dock").as_deref() != Some("0")),
            palette_open: RwSignal::new(false),
            modal: RwSignal::new(None),
            toasts: RwSignal::new(Vec::new()),
            tree_group: RwSignal::new(TreeGroup::Id),
            tree_filter: RwSignal::new(String::new()),
            expanded: RwSignal::new(Default::default()),
            theme: RwSignal::new(persist::get("candy.theme").unwrap_or_else(|| "system".into())),
            hex_ids: RwSignal::new(true),
            hover: RwSignal::new(None),
            dirty: RwSignal::new(false),
            has_doc: RwSignal::new(false),
            convention,
            issues,
            decode_input: RwSignal::new(String::new()),
            diff_other: RwSignal::new(None),
        }
    }

    fn push_undo(&self, label: &str) {
        let snapshot = self.doc.get_untracked();
        self.undo.update_value(|u| {
            u.push((label.to_string(), snapshot));
            if u.len() > 200 {
                u.remove(0);
            }
        });
        self.redo.update_value(|r| r.clear());
        self.sync_history();
    }

    fn sync_history(&self) {
        let u = self.undo.with_value(|u| u.len());
        let r = self.redo.with_value(|r| r.len());
        self.history_len.set((u, r));
    }

    /// Apply an edit to the document with an undo step.
    pub fn edit(&self, label: &str, f: impl FnOnce(&mut Dbc)) {
        self.last_edit.set_value((String::new(), 0.0));
        self.push_undo(label);
        self.doc.update(f);
        self.dirty.set(true);
    }

    /// Like [`edit`], but consecutive edits with the same key within 1.5 s share one undo step
    /// (typing into a field produces one step, not one per keystroke).
    pub fn edit_coalesce(&self, key: &str, label: &str, f: impl FnOnce(&mut Dbc)) {
        let now = js_sys::Date::now();
        let (last_key, at) = self.last_edit.get_value();
        if last_key != key || now - at > 1500.0 {
            self.push_undo(label);
        }
        self.last_edit.set_value((key.to_string(), now));
        self.doc.update(f);
        self.dirty.set(true);
    }

    pub fn undo(&self) {
        let Some((label, prev)) = self.undo.try_update_value(|u| u.pop()).flatten() else {
            return;
        };
        let cur = self.doc.get_untracked();
        self.redo.update_value(|r| r.push((label.clone(), cur)));
        self.doc.set(prev);
        self.last_edit.set_value((String::new(), 0.0));
        self.sync_history();
        self.fix_selection();
        self.toast(format!("Undid {label}"), ToastKind::Info);
    }

    pub fn redo(&self) {
        let Some((label, next)) = self.redo.try_update_value(|r| r.pop()).flatten() else {
            return;
        };
        let cur = self.doc.get_untracked();
        self.undo.update_value(|u| u.push((label.clone(), cur)));
        self.doc.set(next);
        self.sync_history();
        self.fix_selection();
        self.toast(format!("Redid {label}"), ToastKind::Info);
    }

    pub fn undo_label(&self) -> Option<String> {
        self.history_len.track();
        self.undo.with_value(|u| u.last().map(|x| x.0.clone()))
    }

    pub fn redo_label(&self) -> Option<String> {
        self.history_len.track();
        self.redo.with_value(|r| r.last().map(|x| x.0.clone()))
    }

    /// If the selected object disappeared (undo, delete, rename), select its parent.
    pub fn fix_selection(&self) {
        let sel = self.sel.get_untracked();
        let ok = self.doc.with_untracked(|d| match &sel {
            Sel::Network => true,
            Sel::Node(n) => d.node(n).is_some(),
            Sel::Message(id) => d.message(*id).is_some(),
            Sel::Signal(id, s) => d.message(*id).and_then(|m| m.signal(s)).is_some(),
            Sel::ValueTable(v) => d.value_tables.iter().any(|t| &t.name == v),
        });
        if !ok {
            let parent = match sel {
                Sel::Signal(id, _) if self.doc.with_untracked(|d| d.message(id).is_some()) => {
                    Sel::Message(id)
                }
                _ => Sel::Network,
            };
            self.sel.set(parent);
        }
    }

    /// Replace the whole document (open a file, load an example).
    pub fn load(&self, text: &str, name: &str) {
        let out = candy_dbc::parse(text);
        let n_err = out.errors.len();
        self.undo.update_value(|u| u.clear());
        self.redo.update_value(|r| r.clear());
        self.sync_history();
        self.baseline.set(out.dbc.clone());
        self.doc.set(out.dbc);
        self.parse_errors.set(out.errors);
        self.file_name.set(name.to_string());
        self.sel.set(Sel::Network);
        self.mux_branch.set(None);
        self.expanded.set(Default::default());
        self.dirty.set(false);
        self.has_doc.set(true);
        self.diff_other.set(None);
        if n_err > 0 {
            self.dock_tab.set(DockTab::Problems);
            self.dock_open.set(true);
            self.toast(
                format!(
                    "Opened {name} with {n_err} parse problem{}",
                    if n_err == 1 { "" } else { "s" }
                ),
                ToastKind::Error,
            );
        } else {
            let (m, s) = self
                .doc
                .with_untracked(|d| (d.messages.len(), d.signal_count()));
            self.toast(
                format!("Opened {name}: {m} messages, {s} signals"),
                ToastKind::Success,
            );
        }
    }

    pub fn new_document(&self) {
        let mut d = Dbc::new();
        d.nodes.push(candy_dbc::Node {
            name: "ECU".into(),
            ..Default::default()
        });
        let text = candy_dbc::write(&d);
        self.load(&text, "untitled.dbc");
        self.toasts.update(|t| t.clear());
        self.toast("New network created".into(), ToastKind::Success);
    }

    pub fn toast(&self, text: String, kind: ToastKind) {
        let id = js_sys::Date::now() as u64 ^ (js_sys::Math::random() * 1e6) as u64;
        self.toasts.update(|t| {
            t.push(Toast { id, text, kind });
            if t.len() > 4 {
                t.remove(0);
            }
        });
        let toasts = self.toasts;
        set_timeout(
            move || toasts.update(|t| t.retain(|x| x.id != id)),
            std::time::Duration::from_millis(if kind == ToastKind::Error { 6000 } else { 3200 }),
        );
    }

    pub fn select(&self, sel: Sel) {
        if let Some(id) = sel.message_id() {
            if self.sel.get_untracked().message_id() != Some(id) {
                self.mux_branch.set(None);
            }
            self.expanded.update(|e| {
                e.insert(id);
            });
        }
        // Show the branch a multiplexed signal lives in.
        if let Sel::Signal(id, name) = &sel {
            let branch = self.doc.with_untracked(|d| {
                d.message(*id)
                    .and_then(|m| m.signal(name).map(|s| (m, s)))
                    .and_then(|(m, s)| {
                        candy_dbc::layout::active_selectors(m, s).map(|(sw, r)| {
                            (
                                sw == m.multiplexor().map(|x| x.name.clone()).unwrap_or_default(),
                                r,
                            )
                        })
                    })
            });
            if let Some((true, ranges)) = branch
                && let Some((lo, _)) = ranges.first()
            {
                self.mux_branch.set(Some(*lo));
            }
        }
        self.sel.set(sel);
    }

    pub fn issues_for(&self, pred: impl Fn(&Target) -> bool) -> (usize, usize) {
        self.issues.with(|is| {
            let mut e = 0;
            let mut w = 0;
            for i in is.iter().filter(|i| pred(&i.target)) {
                match i.severity {
                    lint::Severity::Error => e += 1,
                    lint::Severity::Warning => w += 1,
                    _ => {}
                }
            }
            (e, w)
        })
    }
}

pub fn use_app() -> AppState {
    expect_context::<AppState>()
}
