//! Canonical DBC writer. Output follows the section order Vector CANdb++ uses, so files diff
//! cleanly and open in every tool.

use crate::model::*;
use std::fmt::Write as _;

pub const DEFAULT_NEW_SYMBOLS: &[&str] = &[
    "NS_DESC_",
    "CM_",
    "BA_DEF_",
    "BA_",
    "VAL_",
    "CAT_DEF_",
    "CAT_",
    "FILTER",
    "BA_DEF_DEF_",
    "EV_DATA_",
    "ENVVAR_DATA_",
    "SGTYPE_",
    "SGTYPE_VAL_",
    "BA_DEF_SGTYPE_",
    "BA_SGTYPE_",
    "SIG_TYPE_REF_",
    "VAL_TABLE_",
    "SIG_GROUP_",
    "SIG_VALTYPE_",
    "SIGTYPE_VALTYPE_",
    "BO_TX_BU_",
    "BA_DEF_REL_",
    "BA_REL_",
    "BA_DEF_DEF_REL_",
    "BU_SG_REL_",
    "BU_EV_REL_",
    "BU_BO_REL_",
    "SG_MUL_VAL_",
];

pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

fn attr_value(v: &AttrValue) -> String {
    match v {
        AttrValue::Int(i) => i.to_string(),
        AttrValue::Float(f) => {
            let s = fmt_num(*f);
            // Keep floats recognisable as floats so they parse back the same way.
            if s.contains(['.', 'e', 'E']) { s } else { format!("{s}.0") }
        }
        AttrValue::Str(s) => quote(s),
    }
}

fn values(entries: &[ValueEntry]) -> String {
    let mut s = String::new();
    for e in entries {
        let _ = write!(s, " {} {}", e.value, quote(&e.label));
    }
    s
}

/// Format one `SG_` line (without leading indentation or newline).
pub fn signal_line(s: &Signal) -> String {
    let mux = match s.mux {
        Mux::None => String::new(),
        m => format!(" {}", m.token()),
    };
    format!(
        "SG_ {}{} : {}|{}@{}{} ({},{}) [{}|{}] {} {}",
        s.name,
        mux,
        s.start_bit,
        s.size,
        if s.byte_order == ByteOrder::LittleEndian { '1' } else { '0' },
        if s.value_type.is_signed() { '-' } else { '+' },
        fmt_num(s.factor),
        fmt_num(s.offset),
        fmt_num(s.min),
        fmt_num(s.max),
        quote(&s.unit),
        if s.receivers.is_empty() { NO_NODE.to_string() } else { s.receivers.join(",") },
    )
}

pub fn write(d: &Dbc) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "VERSION {}", quote(&d.version));
    o.push_str("\n\nNS_ :\n");
    for s in &d.new_symbols {
        let _ = writeln!(o, "\t{s}");
    }
    let _ = writeln!(o, "\nBS_:{}", if d.bit_timing.is_empty() { String::new() } else { format!(" {}", d.bit_timing) });
    let names: Vec<&str> = d.nodes.iter().map(|n| n.name.as_str()).collect();
    let _ = writeln!(o, "\nBU_: {}", names.join(" "));
    for vt in &d.value_tables {
        let _ = writeln!(o, "VAL_TABLE_ {}{} ;", vt.name, values(&vt.entries));
    }
    o.push('\n');

    for m in &d.messages {
        let _ = writeln!(o, "\nBO_ {} {}: {} {}", m.id, m.name, m.size, m.transmitter);
        for s in &m.signals {
            let _ = writeln!(o, " {}", signal_line(s));
        }
    }
    o.push_str("\n\n");

    for m in &d.messages {
        if !m.extra_transmitters.is_empty() {
            let _ = writeln!(o, "BO_TX_BU_ {} : {};", m.id, m.extra_transmitters.join(","));
        }
    }

    for e in &d.env_vars {
        let _ = writeln!(
            o,
            "\nEV_ {}: {} [{}|{}] {} {} {} {} {};",
            e.name,
            e.var_type,
            fmt_num(e.min),
            fmt_num(e.max),
            quote(&e.unit),
            fmt_num(e.initial),
            e.ev_id,
            e.access,
            if e.nodes.is_empty() { NO_NODE.to_string() } else { e.nodes.join(",") }
        );
    }
    for e in &d.env_vars {
        if let Some(size) = e.data_size {
            let _ = writeln!(o, "ENVVAR_DATA_ {}: {};", e.name, size);
        }
    }

    // Comments.
    if let Some(c) = &d.comment {
        let _ = writeln!(o, "CM_ {};", quote(c));
    }
    for n in &d.nodes {
        if let Some(c) = &n.comment {
            let _ = writeln!(o, "CM_ BU_ {} {};", n.name, quote(c));
        }
    }
    for m in &d.messages {
        if let Some(c) = &m.comment {
            let _ = writeln!(o, "CM_ BO_ {} {};", m.id, quote(c));
        }
        for s in &m.signals {
            if let Some(c) = &s.comment {
                let _ = writeln!(o, "CM_ SG_ {} {} {};", m.id, s.name, quote(c));
            }
        }
    }
    for e in &d.env_vars {
        if let Some(c) = &e.comment {
            let _ = writeln!(o, "CM_ EV_ {} {};", e.name, quote(c));
        }
    }

    // Attribute definitions and defaults.
    for a in &d.attr_defs {
        let kind = match &a.kind {
            AttrKind::Int { min, max } => format!("INT {min} {max}"),
            AttrKind::Hex { min, max } => format!("HEX {min} {max}"),
            AttrKind::Float { min, max } => format!("FLOAT {} {}", fmt_num(*min), fmt_num(*max)),
            AttrKind::String => "STRING ".into(),
            AttrKind::Enum(opts) => {
                format!("ENUM  {}", opts.iter().map(|s| quote(s)).collect::<Vec<_>>().join(","))
            }
        };
        if a.object.is_relation() {
            let _ = writeln!(o, "BA_DEF_REL_ {}  {} {};", a.object.keyword(), quote(&a.name), kind);
        } else if a.object == AttrObject::Network {
            let _ = writeln!(o, "BA_DEF_  {} {};", quote(&a.name), kind);
        } else {
            let _ = writeln!(o, "BA_DEF_ {}  {} {};", a.object.keyword(), quote(&a.name), kind);
        }
    }
    for a in &d.attr_defaults {
        let kw = if a.relation { "BA_DEF_DEF_REL_" } else { "BA_DEF_DEF_" };
        let _ = writeln!(o, "{kw}  {} {};", quote(&a.name), attr_value(&a.value));
    }

    // Attribute values.
    for a in &d.attributes {
        let _ = writeln!(o, "BA_ {} {};", quote(&a.name), attr_value(&a.value));
    }
    for n in &d.nodes {
        for a in &n.attributes {
            let _ = writeln!(o, "BA_ {} BU_ {} {};", quote(&a.name), n.name, attr_value(&a.value));
        }
    }
    for m in &d.messages {
        for a in &m.attributes {
            let _ = writeln!(o, "BA_ {} BO_ {} {};", quote(&a.name), m.id, attr_value(&a.value));
        }
        for s in &m.signals {
            for a in &s.attributes {
                let _ = writeln!(o, "BA_ {} SG_ {} {} {};", quote(&a.name), m.id, s.name, attr_value(&a.value));
            }
        }
    }
    for e in &d.env_vars {
        for a in &e.attributes {
            let _ = writeln!(o, "BA_ {} EV_ {} {};", quote(&a.name), e.name, attr_value(&a.value));
        }
    }

    for x in &d.extra {
        let _ = writeln!(o, "{x}");
    }

    // Value descriptions.
    for m in &d.messages {
        for s in &m.signals {
            if !s.value_descriptions.is_empty() {
                let _ = writeln!(o, "VAL_ {} {}{} ;", m.id, s.name, values(&s.value_descriptions));
            }
        }
    }
    for e in &d.env_vars {
        if !e.value_descriptions.is_empty() {
            let _ = writeln!(o, "VAL_ {}{} ;", e.name, values(&e.value_descriptions));
        }
    }

    for g in &d.signal_groups {
        let _ = writeln!(o, "SIG_GROUP_ {} {} {} : {};", g.message_id, g.name, g.repetitions, g.signals.join(" "));
    }

    for m in &d.messages {
        for s in &m.signals {
            let t = match s.value_type {
                ValueType::Float32 => 1,
                ValueType::Float64 => 2,
                _ => continue,
            };
            let _ = writeln!(o, "SIG_VALTYPE_ {} {} : {};", m.id, s.name, t);
        }
    }

    for m in &d.messages {
        for s in &m.signals {
            for e in &s.ext_mux {
                let ranges: Vec<String> = e.ranges.iter().map(|(a, b)| format!("{a}-{b}")).collect();
                let _ = writeln!(o, "SG_MUL_VAL_ {} {} {} {};", m.id, s.name, e.switch, ranges.join(", "));
            }
        }
    }
    o
}
