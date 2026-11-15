//! Lint rules with explanations and one-click fixes.

use crate::layout::{self, FD_SIZES};
use crate::model::*;
use crate::naming::{self, Convention};
use std::collections::{HashMap, HashSet};

/// A reference to one object in a DBC, used to jump to it in the UI and to apply fixes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Target {
    Network,
    Node(String),
    Message(u32),
    Signal(u32, String),
    ValueTable(String),
    EnvVar(String),
    ValueLabel(u32, String, i64),
    SignalGroup(u32, String),
}

impl Target {
    pub fn message_id(&self) -> Option<u32> {
        match self {
            Target::Message(id)
            | Target::Signal(id, _)
            | Target::ValueLabel(id, _, _)
            | Target::SignalGroup(id, _) => Some(*id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "Error",
            Severity::Warning => "Warning",
            Severity::Info => "Info",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Fix {
    Rename {
        target: Target,
        new_name: String,
    },
    SetMessageSize {
        id: u32,
        size: u32,
    },
    SetSignalSize {
        id: u32,
        signal: String,
        size: u32,
    },
    SetFactor {
        id: u32,
        signal: String,
        factor: f64,
    },
    SetRange {
        id: u32,
        signal: String,
        min: f64,
        max: f64,
    },
    MoveSignal {
        id: u32,
        signal: String,
        start_bit: u32,
    },
    AddNode(String),
    RemoveNode(String),
    RemoveValueTable(String),
    RemoveGroupSignal {
        id: u32,
        group: String,
        signal: String,
    },
    SetCanId {
        id: u32,
        name: String,
        new_id: u32,
    },
}

impl Fix {
    pub fn label(&self) -> String {
        match self {
            Fix::Rename { new_name, .. } => format!("Rename to {new_name}"),
            Fix::SetMessageSize { size, .. } => format!("Set length to {size} bytes"),
            Fix::SetSignalSize { size, .. } => format!("Set size to {size} bits"),
            Fix::SetFactor { factor, .. } => format!("Set factor to {}", fmt_num(*factor)),
            Fix::SetRange { min, max, .. } => {
                format!("Set range to [{} … {}]", fmt_num(*min), fmt_num(*max))
            }
            Fix::MoveSignal { start_bit, .. } => format!("Move to free bits at {start_bit}"),
            Fix::AddNode(n) => format!("Add node {n}"),
            Fix::RemoveNode(n) => format!("Remove node {n}"),
            Fix::RemoveValueTable(n) => format!("Remove value table {n}"),
            Fix::RemoveGroupSignal { signal, .. } => format!("Remove {signal} from group"),
            Fix::SetCanId { new_id, .. } => format!("Change id to 0x{:X}", new_id & !EXTENDED_FLAG),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub rule: &'static str,
    pub severity: Severity,
    pub target: Target,
    pub message: String,
    pub fix: Option<Fix>,
}

/// Rule id, title and a longer explanation shown in tooltips.
pub const RULES: &[(&str, &str, &str)] = &[
    (
        "duplicate-id",
        "Duplicate message id",
        "Two messages share an identifier. Receivers cannot tell them apart and most tools refuse the file.",
    ),
    (
        "duplicate-name",
        "Duplicate name",
        "Names must be unique within their scope (messages and nodes per file, signals per message).",
    ),
    (
        "id-range",
        "Identifier out of range",
        "Standard ids are 11-bit (≤ 0x7FF); extended ids are 29-bit (≤ 0x1FFFFFFF).",
    ),
    (
        "invalid-name",
        "Invalid identifier",
        "DBC names must match [A-Za-z_][A-Za-z0-9_]*. Other characters break most parsers and code generators.",
    ),
    (
        "long-name",
        "Name longer than 32 characters",
        "Classic CANdb++ and many ECU toolchains truncate names at 32 characters.",
    ),
    (
        "payload-size",
        "Invalid payload length",
        "Classic CAN carries 0–8 bytes; CAN FD allows 12, 16, 20, 24, 32, 48 or 64.",
    ),
    (
        "signal-outside-payload",
        "Signal outside payload",
        "Some of the signal's bits are beyond the message length, so they are never transmitted.",
    ),
    (
        "signal-overlap",
        "Overlapping signals",
        "Two signals that can be present at the same time use the same bits, so writing one corrupts the other.",
    ),
    (
        "signal-size",
        "Invalid signal size",
        "Signals must be 1–64 bits; float signals must be exactly 32 (single) or 64 (double) bits.",
    ),
    (
        "zero-factor",
        "Zero factor",
        "A factor of 0 maps every raw value to the offset, which loses all information and cannot be inverted.",
    ),
    (
        "range-inverted",
        "Minimum above maximum",
        "The declared minimum is greater than the maximum.",
    ),
    (
        "range-unreachable",
        "Range not representable",
        "The declared physical range goes beyond what the raw bits can encode with this factor and offset.",
    ),
    (
        "range-unset",
        "Range not set",
        "Minimum and maximum are both 0. Tools treat this as 'unknown', which disables range checks.",
    ),
    (
        "missing-unit",
        "Scaled signal without unit",
        "The signal is scaled (factor or offset) but has no unit, which makes values hard to interpret.",
    ),
    (
        "unknown-node",
        "Undeclared node",
        "A transmitter or receiver is not listed in BU_. Strict tools reject the file.",
    ),
    (
        "unused-node",
        "Unused node",
        "The node neither sends nor receives anything.",
    ),
    (
        "unused-value-table",
        "Unused value table",
        "This VAL_TABLE_ is not referenced by any signal.",
    ),
    (
        "value-out-of-range",
        "Value label outside raw range",
        "A value description uses a raw value the signal can never carry.",
    ),
    (
        "mux-no-switch",
        "Multiplexed signal without switch",
        "The message has multiplexed signals but no multiplexor (M) signal to select them.",
    ),
    (
        "mux-many-switches",
        "Several multiplexor switches",
        "Without SG_MUL_VAL_ extended multiplexing, a message may only have one top-level switch.",
    ),
    (
        "mux-selector-range",
        "Selector value unreachable",
        "The mux value is larger than the switch signal can encode.",
    ),
    (
        "ext-mux-switch",
        "Unknown extended mux switch",
        "SG_MUL_VAL_ names a switch signal that is not in the message.",
    ),
    (
        "empty-message",
        "Message without signals",
        "The message carries no signals.",
    ),
    (
        "group-unknown-signal",
        "Signal group references unknown signal",
        "A SIG_GROUP_ lists a signal that does not exist in its message.",
    ),
    (
        "naming",
        "Naming convention",
        "The name does not follow the active naming convention.",
    ),
];

pub fn rule_info(id: &str) -> Option<(&'static str, &'static str)> {
    RULES.iter().find(|r| r.0 == id).map(|r| (r.1, r.2))
}

struct Ctx {
    out: Vec<Issue>,
}

impl Ctx {
    fn push(
        &mut self,
        rule: &'static str,
        severity: Severity,
        target: Target,
        message: String,
        fix: Option<Fix>,
    ) {
        self.out.push(Issue {
            rule,
            severity,
            target,
            message,
            fix,
        });
    }
}

pub fn lint(dbc: &Dbc, convention: Option<&Convention>) -> Vec<Issue> {
    use Severity::*;
    let mut c = Ctx { out: Vec::new() };

    let declared: HashSet<&str> = dbc.nodes.iter().map(|n| n.name.as_str()).collect();
    let mut used_nodes: HashSet<&str> = HashSet::new();
    let mut missing_nodes: HashSet<String> = HashSet::new();

    // Nodes.
    let mut seen = HashSet::new();
    for n in &dbc.nodes {
        if !seen.insert(n.name.as_str()) {
            c.push(
                "duplicate-name",
                Error,
                Target::Node(n.name.clone()),
                format!("Node {} is declared twice", n.name),
                None,
            );
        }
        check_ident(
            &mut c,
            Target::Node(n.name.clone()),
            &n.name,
            convention.is_none(),
        );
    }

    // Messages.
    let mut ids: HashMap<u32, &str> = HashMap::new();
    let mut names: HashMap<&str, u32> = HashMap::new();
    for m in &dbc.messages {
        let t = Target::Message(m.id);
        if m.is_pseudo() {
            continue;
        }
        if let Some(other) = ids.insert(m.id, &m.name) {
            let free = dbc.free_id(m.can_id(), m.is_extended());
            c.push(
                "duplicate-id",
                Error,
                t.clone(),
                format!("{} uses id {} already used by {other}", m.name, m.id_hex()),
                dbc.message(free).is_none().then(|| Fix::SetCanId {
                    id: m.id,
                    name: m.name.clone(),
                    new_id: free,
                }),
            );
        }
        if let Some(_other) = names.insert(&m.name, m.id) {
            c.push(
                "duplicate-name",
                Error,
                t.clone(),
                format!("Message name {} is used twice", m.name),
                None,
            );
        }
        if !m.is_extended() && m.can_id() > 0x7FF {
            c.push(
                "id-range",
                Error,
                t.clone(),
                format!("{}: standard id {} exceeds 0x7FF", m.name, m.id_hex()),
                None,
            );
        }
        if m.is_extended() && m.can_id() > 0x1FFF_FFFF {
            c.push(
                "id-range",
                Error,
                t.clone(),
                format!("{}: extended id exceeds 29 bits", m.name),
                None,
            );
        }
        check_ident(&mut c, t.clone(), &m.name, convention.is_none());
        if !FD_SIZES.contains(&m.size) {
            c.push(
                "payload-size",
                Error,
                t.clone(),
                format!(
                    "{}: {} bytes is not a valid CAN/CAN FD length",
                    m.name, m.size
                ),
                Some(Fix::SetMessageSize {
                    id: m.id,
                    size: layout::round_up_size(m.size),
                }),
            );
        }
        if m.signals.is_empty() {
            c.push(
                "empty-message",
                Info,
                t.clone(),
                format!("{} has no signals", m.name),
                None,
            );
        }

        used_nodes.insert(&m.transmitter);
        for tx in std::iter::once(&m.transmitter).chain(&m.extra_transmitters) {
            if tx != NO_NODE && !declared.contains(tx.as_str()) && missing_nodes.insert(tx.clone())
            {
                c.push(
                    "unknown-node",
                    Warning,
                    t.clone(),
                    format!("Transmitter {tx} of {} is not declared in BU_", m.name),
                    Some(Fix::AddNode(tx.clone())),
                );
            }
            used_nodes.insert(tx);
        }

        // Signals.
        let mut sig_names = HashSet::new();
        for s in &m.signals {
            let st = Target::Signal(m.id, s.name.clone());
            if !sig_names.insert(s.name.as_str()) {
                c.push(
                    "duplicate-name",
                    Error,
                    st.clone(),
                    format!("Signal {} appears twice in {}", s.name, m.name),
                    None,
                );
            }
            check_ident(&mut c, st.clone(), &s.name, convention.is_none());
            check_signal(&mut c, m, s);
            for r in &s.receivers {
                used_nodes.insert(r);
                if r != NO_NODE && !declared.contains(r.as_str()) && missing_nodes.insert(r.clone())
                {
                    c.push(
                        "unknown-node",
                        Warning,
                        st.clone(),
                        format!(
                            "Receiver {r} of {}.{} is not declared in BU_",
                            m.name, s.name
                        ),
                        Some(Fix::AddNode(r.clone())),
                    );
                }
            }
        }

        // Overlaps.
        for (i, j, bits) in layout::overlaps(m) {
            let (a, b) = (&m.signals[i], &m.signals[j]);
            let fix = layout::find_free_bits(m, b.size, b.mux.selector())
                .filter(|_| b.byte_order == ByteOrder::LittleEndian)
                .map(|start| Fix::MoveSignal {
                    id: m.id,
                    signal: b.name.clone(),
                    start_bit: start,
                });
            c.push(
                "signal-overlap",
                Error,
                Target::Signal(m.id, b.name.clone()),
                format!(
                    "{} overlaps {} in {} ({} bit{})",
                    b.name,
                    a.name,
                    m.name,
                    bits.len(),
                    if bits.len() == 1 { "" } else { "s" }
                ),
                fix,
            );
        }

        // Multiplexing.
        let has_ext = m.signals.iter().any(|s| !s.ext_mux.is_empty());
        let switches: Vec<&Signal> = m
            .signals
            .iter()
            .filter(|s| s.mux == Mux::Multiplexor)
            .collect();
        if m.signals.iter().any(|s| s.mux.selector().is_some()) && switches.is_empty() && !has_ext {
            c.push(
                "mux-no-switch",
                Error,
                t.clone(),
                format!("{} has multiplexed signals but no multiplexor", m.name),
                None,
            );
        }
        if switches.len() > 1 && !has_ext {
            c.push(
                "mux-many-switches",
                Warning,
                t.clone(),
                format!("{} has {} multiplexor switches", m.name, switches.len()),
                None,
            );
        }
        if let Some(sw) = switches.first() {
            let max = sw.raw_range().1;
            for s in &m.signals {
                if let Some(v) = s.mux.selector()
                    && s.ext_mux.is_empty()
                    && v as f64 > max
                {
                    c.push(
                        "mux-selector-range",
                        Warning,
                        Target::Signal(m.id, s.name.clone()),
                        format!(
                            "{} is selected by {}={v}, but {} only reaches {}",
                            s.name, sw.name, sw.name, max
                        ),
                        None,
                    );
                }
            }
        }
        for s in &m.signals {
            for e in &s.ext_mux {
                if m.signal(&e.switch).is_none() {
                    c.push(
                        "ext-mux-switch",
                        Error,
                        Target::Signal(m.id, s.name.clone()),
                        format!("{} is switched by unknown signal {}", s.name, e.switch),
                        None,
                    );
                }
            }
        }
    }

    // Unused nodes.
    for n in &dbc.nodes {
        let in_env = dbc.env_vars.iter().any(|e| e.nodes.contains(&n.name));
        if !used_nodes.contains(n.name.as_str()) && !in_env {
            c.push(
                "unused-node",
                Info,
                Target::Node(n.name.clone()),
                format!("Node {} sends and receives nothing", n.name),
                Some(Fix::RemoveNode(n.name.clone())),
            );
        }
    }

    // Value tables: a table counts as used when any signal's descriptions match it exactly.
    for vt in &dbc.value_tables {
        let used = dbc
            .messages
            .iter()
            .flat_map(|m| &m.signals)
            .any(|s| s.value_descriptions == vt.entries)
            || dbc
                .attributes
                .iter()
                .chain(dbc.messages.iter().flat_map(|m| &m.attributes))
                .any(|a| a.value.as_str() == Some(&vt.name));
        if !used {
            c.push(
                "unused-value-table",
                Info,
                Target::ValueTable(vt.name.clone()),
                format!("Value table {} is not used by any signal", vt.name),
                Some(Fix::RemoveValueTable(vt.name.clone())),
            );
        }
    }

    // Signal groups.
    for g in &dbc.signal_groups {
        if let Some(m) = dbc.message(g.message_id) {
            for s in &g.signals {
                if m.signal(s).is_none() {
                    c.push(
                        "group-unknown-signal",
                        Warning,
                        Target::SignalGroup(g.message_id, g.name.clone()),
                        format!("Group {} lists unknown signal {s}", g.name),
                        Some(Fix::RemoveGroupSignal {
                            id: g.message_id,
                            group: g.name.clone(),
                            signal: s.clone(),
                        }),
                    );
                }
            }
        }
    }

    // Naming conventions.
    if let Some(conv) = convention {
        for n in naming::check_dbc(dbc, conv) {
            c.push(
                "naming",
                Warning,
                n.target.clone(),
                format!("{} {}: {}", n.entity.label(), n.name, n.reasons.join(", ")),
                Some(Fix::Rename {
                    target: n.target,
                    new_name: n.suggestion,
                }),
            );
        }
    }

    c.out.sort_by_key(|a| a.severity);
    c.out
}

fn check_ident(c: &mut Ctx, target: Target, name: &str, check_len: bool) {
    if !naming::is_identifier(name) {
        c.push(
            "invalid-name",
            Severity::Error,
            target.clone(),
            format!("'{name}' is not a valid DBC identifier"),
            Some(Fix::Rename {
                target,
                new_name: naming::sanitize(name),
            }),
        );
    } else if check_len && name.len() > 32 {
        c.push(
            "long-name",
            Severity::Info,
            target.clone(),
            format!("{name} is {} characters long", name.len()),
            Some(Fix::Rename {
                target,
                new_name: naming::shorten(name, 32, naming::Case::Any),
            }),
        );
    }
}

fn check_signal(c: &mut Ctx, m: &Message, s: &Signal) {
    use Severity::*;
    let t = Target::Signal(m.id, s.name.clone());
    let sig = || s.name.clone();
    match s.value_type {
        ValueType::Float32 if s.size != 32 => c.push(
            "signal-size",
            Error,
            t.clone(),
            format!("{} is a float but {} bits wide", s.name, s.size),
            Some(Fix::SetSignalSize {
                id: m.id,
                signal: sig(),
                size: 32,
            }),
        ),
        ValueType::Float64 if s.size != 64 => c.push(
            "signal-size",
            Error,
            t.clone(),
            format!("{} is a double but {} bits wide", s.name, s.size),
            Some(Fix::SetSignalSize {
                id: m.id,
                signal: sig(),
                size: 64,
            }),
        ),
        _ if s.size == 0 || s.size > 64 => c.push(
            "signal-size",
            Error,
            t.clone(),
            format!("{} is {} bits wide (must be 1–64)", s.name, s.size),
            Some(Fix::SetSignalSize {
                id: m.id,
                signal: sig(),
                size: s.size.clamp(1, 64),
            }),
        ),
        _ => {}
    }
    if s.size > 0 && !layout::fits(s, m.size) {
        let need = layout::bytes_needed(s);
        c.push(
            "signal-outside-payload",
            Error,
            t.clone(),
            format!(
                "{} needs {need} bytes but {} is {} bytes long",
                s.name, m.name, m.size
            ),
            (need <= 64).then(|| Fix::SetMessageSize {
                id: m.id,
                size: layout::round_up_size(need),
            }),
        );
    }
    if s.factor == 0.0 {
        c.push(
            "zero-factor",
            Error,
            t.clone(),
            format!("{} has factor 0", s.name),
            Some(Fix::SetFactor {
                id: m.id,
                signal: sig(),
                factor: 1.0,
            }),
        );
    }
    let (plo, phi) = s.physical_range();
    if s.min > s.max {
        c.push(
            "range-inverted",
            Warning,
            t.clone(),
            format!(
                "{}: minimum {} is above maximum {}",
                s.name,
                fmt_num(s.min),
                fmt_num(s.max)
            ),
            Some(Fix::SetRange {
                id: m.id,
                signal: sig(),
                min: s.max,
                max: s.min,
            }),
        );
    } else if s.min == 0.0 && s.max == 0.0 {
        if s.size > 1 && !s.value_type.is_float() {
            c.push(
                "range-unset",
                Info,
                t.clone(),
                format!("{} has no min/max range", s.name),
                Some(Fix::SetRange {
                    id: m.id,
                    signal: sig(),
                    min: round_sig(plo),
                    max: round_sig(phi),
                }),
            );
        }
    } else if !s.value_type.is_float() && s.factor != 0.0 {
        let tol = s.factor.abs() * 0.5 + 1e-9;
        if s.min < plo - tol || s.max > phi + tol {
            c.push(
                "range-unreachable",
                Warning,
                t.clone(),
                format!(
                    "{}: range [{} … {}] exceeds what {} bits can encode [{} … {}]",
                    s.name,
                    fmt_num(s.min),
                    fmt_num(s.max),
                    s.size,
                    fmt_num(round_sig(plo)),
                    fmt_num(round_sig(phi))
                ),
                Some(Fix::SetRange {
                    id: m.id,
                    signal: sig(),
                    min: s.min.max(round_sig(plo)),
                    max: s.max.min(round_sig(phi)),
                }),
            );
        }
    }
    if (s.factor != 1.0 || s.offset != 0.0) && s.unit.trim().is_empty() {
        c.push(
            "missing-unit",
            Info,
            t.clone(),
            format!("{} is scaled but has no unit", s.name),
            None,
        );
    }
    if !s.value_type.is_float() {
        let (rlo, rhi) = s.raw_range();
        for e in &s.value_descriptions {
            if (e.value as f64) < rlo || (e.value as f64) > rhi {
                c.push(
                    "value-out-of-range",
                    Warning,
                    Target::ValueLabel(m.id, s.name.clone(), e.value),
                    format!(
                        "{}: value {} \"{}\" cannot be encoded in {} bits",
                        s.name, e.value, e.label, s.size
                    ),
                    None,
                );
            }
        }
    }
}

/// Round to 10 significant digits to hide floating-point noise (0.1 × 255 = 25.500000000000004).
pub fn round_sig(v: f64) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return v;
    }
    let mag = v.abs().log10().floor() as i32;
    let scale = 10f64.powi(9 - mag);
    (v * scale).round() / scale
}

/// Apply a fix. Returns false when the target no longer exists.
pub fn apply_fix(dbc: &mut Dbc, fix: &Fix) -> bool {
    match fix {
        Fix::Rename { target, new_name } => rename(dbc, target, new_name),
        Fix::SetMessageSize { id, size } => dbc.message_mut(*id).map(|m| m.size = *size).is_some(),
        Fix::SetSignalSize { id, signal, size } => dbc
            .message_mut(*id)
            .and_then(|m| m.signal_mut(signal))
            .map(|s| s.size = *size)
            .is_some(),
        Fix::SetFactor { id, signal, factor } => dbc
            .message_mut(*id)
            .and_then(|m| m.signal_mut(signal))
            .map(|s| s.factor = *factor)
            .is_some(),
        Fix::SetRange {
            id,
            signal,
            min,
            max,
        } => dbc
            .message_mut(*id)
            .and_then(|m| m.signal_mut(signal))
            .map(|s| {
                s.min = *min;
                s.max = *max;
            })
            .is_some(),
        Fix::MoveSignal {
            id,
            signal,
            start_bit,
        } => dbc
            .message_mut(*id)
            .and_then(|m| m.signal_mut(signal))
            .map(|s| s.start_bit = *start_bit)
            .is_some(),
        Fix::AddNode(n) => {
            if dbc.node(n).is_none() {
                dbc.nodes.push(Node {
                    name: n.clone(),
                    ..Default::default()
                });
            }
            true
        }
        Fix::RemoveNode(n) => {
            dbc.nodes.retain(|x| &x.name != n);
            true
        }
        Fix::RemoveValueTable(n) => {
            dbc.value_tables.retain(|x| &x.name != n);
            true
        }
        Fix::RemoveGroupSignal { id, group, signal } => {
            if let Some(g) = dbc
                .signal_groups
                .iter_mut()
                .find(|g| g.message_id == *id && &g.name == group)
            {
                g.signals.retain(|s| s != signal);
            }
            true
        }
        Fix::SetCanId { id, name, new_id } => {
            let Some(m) = dbc
                .messages
                .iter_mut()
                .find(|m| m.id == *id && &m.name == name)
            else {
                return false;
            };
            m.id = *new_id;
            // Groups are keyed by id; only move them when the id is no longer ambiguous.
            if dbc.message(*id).is_none() {
                for g in &mut dbc.signal_groups {
                    if g.message_id == *id {
                        g.message_id = *new_id;
                    }
                }
            }
            true
        }
    }
}

/// Rename any object, updating every reference to it.
pub fn rename(dbc: &mut Dbc, target: &Target, new_name: &str) -> bool {
    match target {
        Target::Node(old) => {
            if dbc.node(old).is_none() {
                return false;
            }
            dbc.rename_node(old, new_name);
            true
        }
        Target::Message(id) => dbc
            .message_mut(*id)
            .map(|m| m.name = new_name.into())
            .is_some(),
        Target::Signal(id, old) => {
            if dbc.message(*id).and_then(|m| m.signal(old)).is_none() {
                return false;
            }
            dbc.rename_signal(*id, old, new_name);
            true
        }
        Target::ValueTable(old) => dbc
            .value_tables
            .iter_mut()
            .find(|v| &v.name == old)
            .map(|v| v.name = new_name.into())
            .is_some(),
        Target::EnvVar(old) => dbc
            .env_vars
            .iter_mut()
            .find(|v| &v.name == old)
            .map(|v| v.name = new_name.into())
            .is_some(),
        Target::ValueLabel(id, sig, value) => dbc
            .message_mut(*id)
            .and_then(|m| m.signal_mut(sig))
            .and_then(|s| s.value_descriptions.iter_mut().find(|e| e.value == *value))
            .map(|e| e.label = new_name.into())
            .is_some(),
        Target::SignalGroup(id, old) => dbc
            .signal_groups
            .iter_mut()
            .find(|g| g.message_id == *id && &g.name == old)
            .map(|g| g.name = new_name.into())
            .is_some(),
        Target::Network => false,
    }
}

/// Apply every available fix for issues of the given rule (or all rules). Renames are applied
/// last and re-checked so cascading changes stay consistent.
pub fn fix_all(dbc: &mut Dbc, issues: &[Issue], rule: Option<&str>) -> usize {
    let mut n = 0;
    let (renames, others): (Vec<&Issue>, Vec<&Issue>) = issues
        .iter()
        .filter(|i| rule.is_none_or(|r| i.rule == r))
        .partition(|i| matches!(i.fix, Some(Fix::Rename { .. })));
    for i in others {
        if let Some(f) = &i.fix
            && apply_fix(dbc, f)
        {
            n += 1;
        }
    }
    // Rename signals before messages before nodes so targets stay addressable.
    let mut renames = renames;
    renames.sort_by_key(|i| match i.target {
        Target::ValueLabel(..) => 0,
        Target::Signal(..) => 1,
        Target::Message(..) => 2,
        _ => 3,
    });
    for i in renames {
        if let Some(f) = &i.fix
            && apply_fix(dbc, f)
        {
            n += 1;
        }
    }
    n
}
