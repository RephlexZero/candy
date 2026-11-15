//! Decoding and encoding frames, plus parsers for common CAN log formats.

use crate::layout::signal_bits;
use crate::model::{Dbc, EXTENDED_FLAG, Message, Mux, Signal, ValueType};

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedSignal {
    pub name: String,
    pub raw: u64,
    /// Raw value interpreted with the signal's signedness (or float bits).
    pub raw_value: f64,
    pub physical: f64,
    pub unit: String,
    pub label: Option<String>,
    /// Bits in the payload, LSB first (for highlighting).
    pub bits: Vec<u32>,
    /// False when a multiplexed signal is not selected by the current switch value.
    pub active: bool,
    pub in_range: bool,
}

fn get_bit(data: &[u8], pos: u32) -> bool {
    data.get((pos / 8) as usize)
        .is_some_and(|b| b >> (pos % 8) & 1 == 1)
}

fn set_bit(data: &mut [u8], pos: u32, v: bool) {
    if let Some(b) = data.get_mut((pos / 8) as usize) {
        if v {
            *b |= 1 << (pos % 8);
        } else {
            *b &= !(1 << (pos % 8));
        }
    }
}

/// Raw bits of a signal as an unsigned integer.
pub fn extract_raw(sig: &Signal, data: &[u8]) -> u64 {
    let mut raw = 0u64;
    for (i, pos) in signal_bits(sig).into_iter().enumerate().take(64) {
        if get_bit(data, pos) {
            raw |= 1 << i;
        }
    }
    raw
}

/// Interpret raw bits according to the value type.
pub fn raw_to_value(sig: &Signal, raw: u64) -> f64 {
    let n = sig.size.min(64);
    match sig.value_type {
        ValueType::Unsigned => raw as f64,
        ValueType::Signed => {
            if n == 0 {
                0.0
            } else if n >= 64 {
                raw as i64 as f64
            } else if raw >> (n - 1) & 1 == 1 {
                (raw | (!0u64 << n)) as i64 as f64
            } else {
                raw as f64
            }
        }
        ValueType::Float32 => f32::from_bits(raw as u32) as f64,
        ValueType::Float64 => f64::from_bits(raw),
    }
}

/// Inverse of [`raw_to_value`]: value → raw bits (rounded and clamped).
pub fn value_to_raw(sig: &Signal, value: f64) -> u64 {
    let n = sig.size.min(64);
    let mask = if n >= 64 { !0u64 } else { (1u64 << n) - 1 };
    match sig.value_type {
        ValueType::Float32 => (value as f32).to_bits() as u64,
        ValueType::Float64 => value.to_bits(),
        ValueType::Unsigned => {
            let (lo, hi) = sig.raw_range();
            value.round().clamp(lo, hi) as u64 & mask
        }
        ValueType::Signed => {
            let (lo, hi) = sig.raw_range();
            (value.round().clamp(lo, hi) as i64 as u64) & mask
        }
    }
}

pub fn insert_raw(sig: &Signal, data: &mut [u8], raw: u64) {
    for (i, pos) in signal_bits(sig).into_iter().enumerate().take(64) {
        set_bit(data, pos, raw >> i & 1 == 1);
    }
}

/// Write a physical value into a payload.
pub fn encode_physical(sig: &Signal, data: &mut [u8], physical: f64) {
    let raw = value_to_raw(sig, sig.to_raw(physical));
    insert_raw(sig, data, raw);
}

/// Decode every signal of a message, resolving (nested and extended) multiplexing.
pub fn decode_message(msg: &Message, data: &[u8]) -> Vec<DecodedSignal> {
    let raws: Vec<u64> = msg.signals.iter().map(|s| extract_raw(s, data)).collect();
    msg.signals
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let raw = raws[i];
            let raw_value = raw_to_value(s, raw);
            let physical = s.to_physical(raw_value);
            let label = if s.value_type.is_float() {
                None
            } else {
                s.value_label(raw_value as i64).map(String::from)
            };
            let in_range = (s.min == 0.0 && s.max == 0.0)
                || (physical >= s.min - 1e-9 && physical <= s.max + 1e-9);
            DecodedSignal {
                name: s.name.clone(),
                raw,
                raw_value,
                physical,
                unit: s.unit.clone(),
                label,
                bits: signal_bits(s),
                active: is_active(msg, i, &raws, 0),
                in_range,
            }
        })
        .collect()
}

fn is_active(msg: &Message, idx: usize, raws: &[u64], depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    let s = &msg.signals[idx];
    let (switch_idx, ranges): (usize, Vec<(u64, u64)>) = if let Some(em) = s.ext_mux.first() {
        match msg.signals.iter().position(|x| x.name == em.switch) {
            Some(p) => (p, em.ranges.clone()),
            None => return false,
        }
    } else if let Some(v) = s.mux.selector() {
        match msg
            .signals
            .iter()
            .position(|x| matches!(x.mux, Mux::Multiplexor))
        {
            Some(p) => (p, vec![(v, v)]),
            None => return false,
        }
    } else {
        return true;
    };
    if switch_idx == idx {
        return true;
    }
    let sel = raws[switch_idx];
    ranges.iter().any(|(lo, hi)| (*lo..=*hi).contains(&sel))
        && is_active(msg, switch_idx, raws, depth + 1)
}

/// One frame read from a log or typed by the user.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// Bus id (without the extended flag).
    pub id: u32,
    pub extended: bool,
    pub data: Vec<u8>,
    pub timestamp: Option<f64>,
}

impl Frame {
    /// Raw DBC id used to look the message up.
    pub fn dbc_id(&self) -> u32 {
        if self.extended {
            self.id | EXTENDED_FLAG
        } else {
            self.id
        }
    }
}

/// Look up a frame's message: exact match first, then J1939-style match ignoring source address
/// and priority for extended ids.
pub fn find_message<'a>(dbc: &'a Dbc, frame: &Frame) -> Option<&'a Message> {
    if let Some(m) = dbc.message(frame.dbc_id()) {
        return Some(m);
    }
    if frame.extended {
        let key = |id: u32| (id >> 8) & 0x3FFFF;
        return dbc
            .messages
            .iter()
            .find(|m| m.is_extended() && key(m.can_id()) == key(frame.id));
    }
    None
}

fn hex_bytes(s: &str) -> Option<Vec<u8>> {
    let clean: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '.' && *c != ',')
        .collect();
    if !clean.len().is_multiple_of(2) || clean.len() > 128 {
        return None;
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).ok())
        .collect()
}

fn parse_id(s: &str) -> Option<(u32, bool)> {
    let s = s.trim();
    let (body, ext_hint) = match s.strip_suffix(['x', 'X']) {
        Some(b) if !b.starts_with("0x") || b.len() > 2 => (b, true),
        _ => (s, false),
    };
    let body = body.trim_start_matches("0x").trim_start_matches("0X");
    let id = u32::from_str_radix(body, 16).ok()?;
    Some((id, ext_hint || body.len() > 4 || id > 0x7FF))
}

/// Parse one log line. Supports:
/// - candump: `(1699999999.123456) can0 1A0#DEADBEEF` and `can0  1A0   [4]  DE AD BE EF`
/// - Vector ASC: `0.012345 1  1A0  Rx   d 4 DE AD BE EF`
/// - PCAN TRC: `1)  12.3  Rx  01A0  4  DE AD BE EF`
/// - bare: `1A0#DEADBEEF`, `0x1A0 DE AD BE EF`, `1A0: de ad be ef`
pub fn parse_line(line: &str) -> Option<Frame> {
    let line = line.trim();
    if line.is_empty() || line.starts_with(';') || line.starts_with("//") {
        return None;
    }
    let mut timestamp = None;
    let mut rest = line;
    if let Some(r) = rest.strip_prefix('(')
        && let Some(end) = r.find(')')
    {
        timestamp = r[..end].trim().parse().ok();
        rest = r[end + 1..].trim();
    }

    // `ID#DATA` (candump -L / cansend syntax); `##` marks CAN FD with a flags nibble.
    if let Some(tok) = rest.split_whitespace().find(|t| t.contains('#')) {
        let (id_s, data_s) = tok.split_once('#')?;
        let data_s = if let Some(fd) = data_s.strip_prefix('#') {
            fd.get(1..).unwrap_or("")
        } else {
            data_s
        };
        let (id, ext) = parse_id(id_s)?;
        let data = if data_s.starts_with('R') {
            Vec::new()
        } else {
            hex_bytes(data_s)?
        };
        return Some(Frame {
            id,
            extended: ext,
            data,
            timestamp,
        });
    }

    let toks: Vec<&str> = rest.split_whitespace().collect();

    // candump default: `can0  1A0   [4]  DE AD BE EF`
    if let Some(p) = toks
        .iter()
        .position(|t| t.starts_with('[') && t.ends_with(']'))
        && p >= 1
    {
        let (id, ext) = parse_id(toks[p - 1])?;
        let n: usize = toks[p].trim_matches(['[', ']']).parse().ok()?;
        let data = hex_bytes(&toks[p + 1..].iter().take(n).copied().collect::<String>())?;
        return Some(Frame {
            id,
            extended: ext,
            data,
            timestamp,
        });
    }

    // Vector ASC: `t chan id dir d len bytes…`
    if let Some(p) = toks.iter().position(|t| t.eq_ignore_ascii_case("d"))
        && p >= 3
        && matches!(toks.get(p - 1).map(|s| s.to_ascii_lowercase()), Some(ref s) if s == "rx" || s == "tx")
    {
        let (id, ext) = parse_id(toks[p - 2])?;
        let n: usize = toks.get(p + 1)?.parse().ok()?;
        let data = hex_bytes(&toks[p + 2..].iter().take(n).copied().collect::<String>())?;
        let ts = toks.first().and_then(|t| t.parse().ok());
        return Some(Frame {
            id,
            extended: ext,
            data,
            timestamp: timestamp.or(ts),
        });
    }

    // PCAN TRC: `N)  time  Rx|Tx  ID  len  bytes…`
    if toks.first().is_some_and(|t| t.ends_with(')'))
        && let Some(p) = toks
            .iter()
            .position(|t| t.eq_ignore_ascii_case("rx") || t.eq_ignore_ascii_case("tx"))
    {
        let (id, ext) = parse_id(toks.get(p + 1)?)?;
        let n: usize = toks.get(p + 2)?.parse().ok()?;
        let data = hex_bytes(&toks[p + 3..].iter().take(n).copied().collect::<String>())?;
        let ts = toks
            .get(1)
            .and_then(|t| t.parse::<f64>().ok())
            .map(|ms| ms / 1000.0);
        return Some(Frame {
            id,
            extended: ext || toks[p + 1].len() > 4,
            data,
            timestamp: ts,
        });
    }

    // Bare: `ID data…` or `ID: data…`
    let first = toks.first()?.trim_end_matches(':');
    let (id, ext) = parse_id(first)?;
    let data = hex_bytes(&toks[1..].concat())?;
    Some(Frame {
        id,
        extended: ext,
        data,
        timestamp,
    })
}

pub fn parse_log(text: &str) -> Vec<Frame> {
    text.lines().filter_map(parse_line).collect()
}
