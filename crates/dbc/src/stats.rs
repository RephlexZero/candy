//! Network statistics: bus load and summary counts.

use crate::model::{Dbc, Message};

/// Worst-case bits on the wire for one frame, including stuff bits (classic CAN).
pub fn frame_bits(msg: &Message) -> f64 {
    let payload = (msg.size.min(64) * 8) as f64;
    let (overhead, stuffable) = if msg.is_extended() {
        (67.0, 54.0)
    } else {
        (47.0, 34.0)
    };
    // Stuff bits: one per four bits of the stuffable region, worst case.
    overhead + payload + ((stuffable + payload - 1.0) / 4.0).floor()
}

/// Bus load contribution of one message at the given bitrate, as a fraction (0–1).
pub fn message_load(dbc: &Dbc, msg: &Message, bitrate: f64) -> Option<f64> {
    let cycle = dbc.cycle_time_ms(msg)?;
    Some(frame_bits(msg) * (1000.0 / cycle) / bitrate)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    pub nodes: usize,
    pub messages: usize,
    pub signals: usize,
    pub multiplexed_messages: usize,
    pub extended_messages: usize,
    pub fd_messages: usize,
    pub value_tables: usize,
    pub attributes: usize,
    /// Estimated bus load at the given bitrate (only messages with a cycle time).
    pub bus_load: f64,
    pub messages_with_cycle: usize,
}

pub fn summary(dbc: &Dbc, bitrate: f64) -> Summary {
    let mut s = Summary {
        nodes: dbc.nodes.len(),
        value_tables: dbc.value_tables.len(),
        attributes: dbc.attr_defs.len(),
        ..Default::default()
    };
    for m in dbc.messages.iter().filter(|m| !m.is_pseudo()) {
        s.messages += 1;
        s.signals += m.signals.len();
        s.multiplexed_messages += m.is_multiplexed() as usize;
        s.extended_messages += m.is_extended() as usize;
        s.fd_messages += (m.size > 8) as usize;
        if let Some(l) = message_load(dbc, m, bitrate) {
            s.bus_load += l;
            s.messages_with_cycle += 1;
        }
    }
    s
}

/// Bitrate from the `Baudrate` network attribute, defaulting to 500 kbit/s.
pub fn bitrate(dbc: &Dbc) -> f64 {
    dbc.network_attribute("Baudrate")
        .and_then(|v| v.as_f64())
        .filter(|v| *v > 0.0)
        .unwrap_or(500_000.0)
}
