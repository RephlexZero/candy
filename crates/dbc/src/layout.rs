//! Bit-level layout: where each signal lives in the payload.
//!
//! Bits are numbered "linearly": byte `n`, bit `b` (0 = least significant) is position
//! `n * 8 + b`. This matches the DBC start-bit convention for both byte orders:
//! Intel start bits name the LSB, Motorola start bits name the MSB (Vector "sawtooth" numbering).

use crate::model::{ByteOrder, Message, Mux, Signal};

/// Positions occupied by a signal, least significant bit first.
pub fn signal_bits(sig: &Signal) -> Vec<u32> {
    let n = sig.size.min(512) as usize;
    let mut bits = Vec::with_capacity(n);
    match sig.byte_order {
        ByteOrder::LittleEndian => {
            for i in 0..n as u32 {
                bits.push(sig.start_bit + i);
            }
        }
        ByteOrder::BigEndian => {
            let mut pos = sig.start_bit as i64;
            for _ in 0..n {
                bits.push(pos.max(0) as u32);
                if pos % 8 == 0 {
                    pos += 15;
                } else {
                    pos -= 1;
                }
            }
            bits.reverse();
        }
    }
    bits
}

/// Least and most significant bit positions.
pub fn lsb_msb(sig: &Signal) -> (u32, u32) {
    let b = signal_bits(sig);
    (
        b.first().copied().unwrap_or(0),
        b.last().copied().unwrap_or(0),
    )
}

/// Number of payload bytes the signal needs to fit.
pub fn bytes_needed(sig: &Signal) -> u32 {
    signal_bits(sig).iter().max().map_or(0, |m| m / 8 + 1)
}

pub fn fits(sig: &Signal, size_bytes: u32) -> bool {
    sig.size > 0 && bytes_needed(sig) <= size_bytes
}

/// Start bit that places a signal's MSB at `msb` (Motorola) or LSB at `lsb` (Intel).
/// Converts between the two conventions so a drag in the grid can keep the visual anchor.
pub fn motorola_start_from_lsb(lsb: u32, size: u32) -> u32 {
    // Walk "upwards" from the LSB size-1 steps in sawtooth order.
    let mut pos = lsb as i64;
    for _ in 1..size {
        if pos % 8 == 7 {
            pos -= 15;
        } else {
            pos += 1;
        }
    }
    pos.max(0) as u32
}

/// Two signals can be on the bus at the same time (so overlap matters). Walks the full chain of
/// (possibly nested) switches: if both depend on the same switch with disjoint values, they never
/// coexist.
pub fn coexist(msg: &Message, a: &Signal, b: &Signal) -> bool {
    let ca = conditions(msg, a);
    let cb = conditions(msg, b);
    for (sw, ra) in &ca {
        if let Some((_, rb)) = cb.iter().find(|(s, _)| s == sw)
            && !ra
                .iter()
                .any(|r| rb.iter().any(|q| r.0 <= q.1 && q.0 <= r.1))
        {
            return false;
        }
    }
    true
}

/// Every (switch, ranges) condition that must hold for the signal to be present, innermost first.
pub fn conditions(msg: &Message, sig: &Signal) -> Vec<(String, Vec<(u64, u64)>)> {
    let mut out = Vec::new();
    let mut cur = sig;
    for _ in 0..16 {
        let Some((sw, ranges)) = active_selectors(msg, cur) else {
            break;
        };
        if out.iter().any(|(s, _): &(String, _)| *s == sw) || sw == cur.name {
            break;
        }
        out.push((sw.clone(), ranges));
        match msg.signal(&sw) {
            Some(next) => cur = next,
            None => break,
        }
    }
    out
}

/// For a multiplexed signal: (switch name, selector ranges) under which it is present.
pub fn active_selectors(msg: &Message, sig: &Signal) -> Option<(String, Vec<(u64, u64)>)> {
    if let Some(em) = sig.ext_mux.first() {
        return Some((em.switch.clone(), em.ranges.clone()));
    }
    let v = sig.mux.selector()?;
    // Simple multiplexing: the parent is the message's top-level switch.
    let switch = msg
        .signals
        .iter()
        .find(|s| matches!(s.mux, Mux::Multiplexor))?;
    Some((switch.name.clone(), vec![(v, v)]))
}

/// Pairs of signals (indices) whose bits overlap while both can be present.
pub fn overlaps(msg: &Message) -> Vec<(usize, usize, Vec<u32>)> {
    let bits: Vec<Vec<u32>> = msg.signals.iter().map(signal_bits).collect();
    let mut out = Vec::new();
    for i in 0..msg.signals.len() {
        for j in i + 1..msg.signals.len() {
            if !coexist(msg, &msg.signals[i], &msg.signals[j]) {
                continue;
            }
            let common: Vec<u32> = bits[i]
                .iter()
                .filter(|b| bits[j].contains(b))
                .copied()
                .collect();
            if !common.is_empty() {
                out.push((i, j, common));
            }
        }
    }
    out
}

/// Distinct values of the top-level switch used in a message (for the branch tabs in the grid).
pub fn mux_values(msg: &Message) -> Vec<u64> {
    let Some(top) = msg.multiplexor() else {
        return Vec::new();
    };
    let mut v: Vec<u64> = msg
        .signals
        .iter()
        .filter_map(|s| {
            conditions(msg, s)
                .into_iter()
                .find(|(sw, _)| *sw == top.name)
        })
        .flat_map(|(_, ranges)| ranges.into_iter().flat_map(|(a, b)| a..=b.min(a + 255)))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Is the signal present when the top-level switch equals `branch` (None = show all)?
pub fn in_branch(msg: &Message, sig: &Signal, branch: Option<u64>) -> bool {
    let Some(b) = branch else { return true };
    let Some(top) = msg.multiplexor() else {
        return true;
    };
    match conditions(msg, sig)
        .into_iter()
        .find(|(sw, _)| *sw == top.name)
    {
        None => true,
        Some((_, ranges)) => ranges.iter().any(|(lo, hi)| (*lo..=*hi).contains(&b)),
    }
}

/// First free run of `size` bits (Intel order) in a message, honouring a mux branch.
pub fn find_free_bits(msg: &Message, size: u32, branch: Option<u64>) -> Option<u32> {
    let total = msg.size * 8;
    let mut used = vec![false; total as usize];
    for s in &msg.signals {
        if !in_branch(msg, s, branch) {
            continue;
        }
        for b in signal_bits(s) {
            if let Some(u) = used.get_mut(b as usize) {
                *u = true;
            }
        }
    }
    let mut run = 0;
    for i in 0..total {
        if used[i as usize] {
            run = 0;
        } else {
            run += 1;
            if run == size {
                return Some(i + 1 - size);
            }
        }
    }
    None
}

/// Fraction of payload bits in use (union over all signals).
pub fn utilisation(msg: &Message) -> f64 {
    let total = (msg.size * 8) as usize;
    if total == 0 {
        return 0.0;
    }
    let mut used = vec![false; total];
    for s in &msg.signals {
        for b in signal_bits(s) {
            if let Some(u) = used.get_mut(b as usize) {
                *u = true;
            }
        }
    }
    used.iter().filter(|u| **u).count() as f64 / total as f64
}

/// Valid CAN FD payload lengths.
pub const FD_SIZES: [u32; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 16, 20, 24, 32, 48, 64];

/// Smallest valid payload size that holds `bytes`.
pub fn round_up_size(bytes: u32) -> u32 {
    FD_SIZES.iter().copied().find(|s| *s >= bytes).unwrap_or(64)
}
