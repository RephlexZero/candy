//! Floating UI: hover cards and toasts.

use candy_dbc::layout;
use candy_dbc::{ByteOrder, Mux};
use leptos::prelude::*;

use crate::icons::{Ic, Icon};
use crate::state::{Card, ToastKind, use_app};
use crate::ui::{hue_style, num};

#[component]
pub fn HoverCard() -> impl IntoView {
    let app = use_app();
    move || {
        let h = app.hover.get()?;
        let (vw, vh) = web_sys::window()
            .map(|w| {
                (
                    w.inner_width()
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(1200.0),
                    w.inner_height()
                        .ok()
                        .and_then(|v| v.as_f64())
                        .unwrap_or(800.0),
                )
            })
            .unwrap_or((1200.0, 800.0));
        let x = if h.x + 340.0 > vw {
            (h.x - 334.0).max(8.0)
        } else {
            h.x + 14.0
        };
        let y = if h.y + 260.0 > vh {
            (h.y - 250.0).max(8.0)
        } else {
            h.y + 16.0
        };
        let body = card_body(&h.card)?;
        Some(view! { <div class="hovercard" style=format!("left:{x}px;top:{y}px")>{body}</div> })
    }
}

fn card_body(card: &Card) -> Option<AnyView> {
    let app = use_app();
    match card {
        Card::Signal(id, name) => app.doc.with_untracked(|d| {
            let m = d.message(*id)?;
            let idx = m.signals.iter().position(|s| &s.name == name)?;
            let s = &m.signals[idx];
            let bits = layout::signal_bits(s);
            let (lsb, msb) = layout::lsb_msb(s);
            let (lo, hi) = s.physical_range();
            let role = match s.mux {
                Mux::None => None,
                Mux::Multiplexor => Some("Multiplexor switch".to_string()),
                Mux::Multiplexed(v) => Some(format!("Present when {} = {v}", m.multiplexor().map(|x| x.name.as_str()).unwrap_or("switch"))),
                Mux::MultiplexedMultiplexor(v) => Some(format!("Nested switch, present when parent = {v}")),
            };
            let rx: Vec<String> = s.receivers.iter().filter(|r| *r != candy_dbc::NO_NODE).cloned().collect();
            let total = (m.size * 8) as usize;
            Some(
                view! {
                    <h4><span class="dot" style=hue_style(idx)></span><span class="mono">{s.name.clone()}</span></h4>
                    <div class="sub">{format!("{} · bits {lsb}…{msb} · {} {}", m.name, if s.byte_order == ByteOrder::BigEndian { "Motorola" } else { "Intel" }, if s.value_type.is_float() { "float" } else if s.value_type.is_signed() { "signed" } else { "unsigned" })}</div>
                    <div class="mini-bits" style=hue_style(idx)>
                        {(0..total.min(64 * 8)).map(|i| {
                            let bit = (i / 8) * 8 + 7 - i % 8;
                            view! { <div class:on=bits.contains(&(bit as u32))></div> }
                        }).collect_view()}
                    </div>
                    <dl class="kv">
                        <dt>"Formula"</dt><dd>{s.formula()}</dd>
                        <dt>"Range"</dt><dd>{format!("{} … {} {}", num(s.min), num(s.max), s.unit)}</dd>
                        <dt>"Representable"</dt><dd>{format!("{} … {}", num(lo), num(hi))}</dd>
                        {role.map(|r| view! { <dt>"Mux"</dt><dd>{r}</dd> })}
                        {(!rx.is_empty()).then(|| view! { <dt>"Receivers"</dt><dd>{rx.join(", ")}</dd> })}
                    </dl>
                    {(!s.value_descriptions.is_empty()).then(|| view! {
                        <div class="mini-vals">
                            {s.value_descriptions.iter().take(10).map(|v| view! { <span class="chip mono">{format!("{} {}", v.value, v.label)}</span> }).collect_view()}
                            {(s.value_descriptions.len() > 10).then(|| view! { <span class="hint">{format!("+{}", s.value_descriptions.len() - 10)}</span> })}
                        </div>
                    })}
                    {s.comment.clone().map(|c| view! { <div class="comment">{c}</div> })}
                }
                .into_any(),
            )
        }),
        Card::Message(id) => app.doc.with_untracked(|d| {
            let m = d.message(*id)?;
            let j = m.j1939();
            let cycle = d.cycle_time_ms(m);
            Some(
                view! {
                    <h4><Icon icon=Ic::Message class="ic-sm"/><span class="mono">{m.name.clone()}</span></h4>
                    <div class="sub">{format!("{} · {} · {} bytes", m.id_hex(), if m.is_extended() { "extended" } else { "standard" }, m.size)}</div>
                    <dl class="kv">
                        <dt>"Decimal"</dt><dd>{m.can_id().to_string()}</dd>
                        <dt>"Sender"</dt><dd>{m.transmitter.clone()}</dd>
                        <dt>"Signals"</dt><dd>{m.signals.len().to_string()}</dd>
                        {cycle.map(|c| view! { <dt>"Cycle"</dt><dd>{format!("{} ms", num(c))}</dd> })}
                        {j.map(|j| view! { <dt>"J1939"</dt><dd>{format!("PGN {} · SA 0x{:02X} · P{}", j.pgn, j.source, j.priority)}</dd> })}
                        <dt>"Payload used"</dt><dd>{format!("{:.0}%", layout::utilisation(m) * 100.0)}</dd>
                    </dl>
                    {m.comment.clone().map(|c| view! { <div class="comment">{c}</div> })}
                }
                .into_any(),
            )
        }),
        Card::Node(n) => app.doc.with_untracked(|d| {
            let node = d.node(n)?;
            let tx = d.messages.iter().filter(|m| &m.transmitter == n).count();
            let rx = d.messages.iter().flat_map(|m| &m.signals).filter(|s| s.receivers.contains(n)).count();
            Some(
                view! {
                    <h4><Icon icon=Ic::Node class="ic-sm"/><span class="mono">{n.clone()}</span></h4>
                    <div class="sub">{format!("Sends {tx} messages · receives {rx} signals")}</div>
                    {node.comment.clone().map(|c| view! { <div class="comment">{c}</div> })}
                }
                .into_any(),
            )
        }),
        Card::Rule(r) => {
            let (title, text) = candy_dbc::lint::rule_info(r)?;
            Some(
                view! {
                    <h4><Icon icon=Ic::Info class="ic-sm"/>{title}</h4>
                    <div class="sub mono">{*r}</div>
                    <div>{text}</div>
                }
                .into_any(),
            )
        }
    }
}

#[component]
pub fn Toasts() -> impl IntoView {
    let app = use_app();
    view! {
        <div class="toasts">
            <For each=move || app.toasts.get() key=|t| t.id let:t>
                <div class="toast" class:error={t.kind == ToastKind::Error} class:success={t.kind == ToastKind::Success}>
                    <Icon icon=match t.kind { ToastKind::Error => Ic::Error, ToastKind::Success => Ic::Check, ToastKind::Info => Ic::Info } class="ic-sm"/>
                    <span>{t.text.clone()}</span>
                </div>
            </For>
        </div>
    }
}
