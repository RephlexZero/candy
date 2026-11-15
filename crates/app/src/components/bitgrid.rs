//! Interactive bit-layout grid: one row per byte, bit 7 on the left (Vector convention).
//! Drag a signal to move it, drag its end handle to resize, drag across empty cells to create one.

use candy_dbc::layout::{self, signal_bits};
use candy_dbc::{ByteOrder, Message, Signal};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::actions;
use crate::icons::{Ic, Icon};
use crate::state::{Card, Sel, use_app};
use crate::ui::{hover_in, hover_out, hue_style};

#[derive(Clone, Copy, PartialEq, Debug)]
enum DragKind {
    Move { grab: usize },
    Resize,
    Create { anchor: u32 },
}

#[derive(Clone, PartialEq, Debug)]
struct Drag {
    kind: DragKind,
    sig: Option<usize>,
    preview: Option<Signal>,
    create_to: Option<u32>,
    moved: bool,
}

/// Effective branch: the selected one, or the first mux value for multiplexed messages.
pub fn effective_branch(msg: &Message, chosen: Option<u64>) -> Option<u64> {
    let vals = layout::mux_values(msg);
    match chosen {
        Some(v) if vals.contains(&v) => Some(v),
        _ => vals.first().copied(),
    }
}

fn bit_at(x: f64, y: f64) -> Option<u32> {
    let doc = web_sys::window()?.document()?;
    let mut el = doc.element_from_point(x as f32, y as f32)?;
    // Walk up to the element carrying the bit data.
    for _ in 0..4 {
        if el.has_attribute("data-bit") {
            break;
        }
        el = el.parent_element()?;
    }
    let left: u32 = el.get_attribute("data-bit")?.parse().ok()?;
    let span: u32 = el
        .get_attribute("data-span")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let rect = el.get_bounding_client_rect();
    let w = rect.width() / span as f64;
    let off = ((x - rect.left()) / w)
        .floor()
        .clamp(0.0, span as f64 - 1.0) as u32;
    left.checked_sub(off)
}

fn far_end(s: &Signal) -> u32 {
    let b = signal_bits(s);
    match s.byte_order {
        ByteOrder::LittleEndian => *b.last().unwrap_or(&0),
        ByteOrder::BigEndian => *b.first().unwrap_or(&0),
    }
}

fn moved_to(orig: &Signal, grab: usize, target: u32, size_bytes: u32) -> Option<Signal> {
    let max = size_bytes * 8;
    let try_start = |start: u32| {
        let mut c = orig.clone();
        c.start_bit = start;
        (signal_bits(&c).get(grab) == Some(&target) && layout::fits(&c, size_bytes)).then_some(c)
    };
    if orig.byte_order == ByteOrder::LittleEndian {
        return target.checked_sub(grab as u32).and_then(try_start);
    }
    (0..max).find_map(try_start)
}

fn resized_to(orig: &Signal, target: u32, size_bytes: u32) -> Option<Signal> {
    (1..=64u32).find_map(|size| {
        let mut c = orig.clone();
        c.size = size;
        (far_end(&c) == target && layout::fits(&c, size_bytes)).then_some(c)
    })
}

#[component]
pub fn BitGrid(msg_id: u32) -> impl IntoView {
    let app = use_app();
    let drag = RwSignal::new(None::<Drag>);

    let msg = Memo::new(move |_| app.doc.with(|d| d.message(msg_id).cloned()));
    let branch = Memo::new(move |_| {
        msg.with(|m| {
            m.as_ref()
                .and_then(|m| effective_branch(m, app.mux_branch.get()))
        })
    });

    // The message as drawn: with the drag preview applied.
    let shown = Memo::new(move |_| {
        let mut m = msg.get()?;
        if let Some(Drag {
            sig: Some(i),
            preview: Some(p),
            ..
        }) = drag.get()
            && let Some(s) = m.signals.get_mut(i)
        {
            *s = p;
        }
        Some(m)
    });

    let on_down = move |ev: leptos::ev::PointerEvent| {
        if ev.button() != 0 {
            return;
        }
        let Some(m) = msg.get_untracked() else { return };
        let Some(bit) = bit_at(ev.client_x() as f64, ev.client_y() as f64) else {
            return;
        };
        let target: web_sys::Element = event_target(&ev);
        let is_handle = target.class_list().contains("bg-handle");
        let sig_idx: Option<usize> = target
            .closest(".bg-seg")
            .ok()
            .flatten()
            .and_then(|e| e.get_attribute("data-sig"))
            .and_then(|s| s.parse().ok());
        let kind = match sig_idx {
            Some(i) if is_handle => {
                let _ = i;
                DragKind::Resize
            }
            Some(i) => {
                let grab = signal_bits(&m.signals[i])
                    .iter()
                    .position(|b| *b == bit)
                    .unwrap_or(0);
                DragKind::Move { grab }
            }
            None => DragKind::Create { anchor: bit },
        };
        if let Some(i) = sig_idx {
            app.select(Sel::Signal(msg_id, m.signals[i].name.clone()));
        }
        if let Some(grid) = ev
            .current_target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        {
            let _ = grid.set_pointer_capture(ev.pointer_id());
        }
        ev.prevent_default();
        drag.set(Some(Drag {
            kind,
            sig: sig_idx,
            preview: None,
            create_to: None,
            moved: false,
        }));
    };

    let on_move = move |ev: leptos::ev::PointerEvent| {
        let Some(mut d) = drag.get_untracked() else {
            return;
        };
        let Some(m) = msg.get_untracked() else { return };
        let Some(bit) = bit_at(ev.client_x() as f64, ev.client_y() as f64) else {
            return;
        };
        match d.kind {
            DragKind::Move { grab } => {
                let Some(i) = d.sig else { return };
                if let Some(p) = moved_to(&m.signals[i], grab, bit, m.size) {
                    d.moved = d.moved || p.start_bit != m.signals[i].start_bit;
                    d.preview = Some(p);
                }
            }
            DragKind::Resize => {
                let Some(i) = d.sig else { return };
                if let Some(p) = resized_to(&m.signals[i], bit, m.size) {
                    d.moved = true;
                    d.preview = Some(p);
                }
            }
            DragKind::Create { .. } => {
                d.moved = true;
                d.create_to = Some(bit);
            }
        }
        if Some(&d) != drag.get_untracked().as_ref() {
            drag.set(Some(d));
        }
    };

    let on_up = move |_ev: leptos::ev::PointerEvent| {
        let Some(d) = drag.get_untracked() else {
            return;
        };
        drag.set(None);
        match d.kind {
            DragKind::Create { anchor } => {
                let end = d.create_to.unwrap_or(anchor);
                let (lo, hi) = (anchor.min(end), anchor.max(end));
                actions::add_signal(app, Some(msg_id), Some((lo, hi - lo + 1)));
            }
            _ => {
                if let (Some(i), Some(p), true) = (d.sig, d.preview, d.moved) {
                    let label = if matches!(d.kind, DragKind::Resize) {
                        "resize signal"
                    } else {
                        "move signal"
                    };
                    app.edit(label, |doc| {
                        if let Some(m) = doc.message_mut(msg_id)
                            && let Some(s) = m.signals.get_mut(i)
                        {
                            s.start_bit = p.start_bit;
                            s.size = p.size;
                        }
                    });
                }
            }
        }
    };

    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        let Sel::Signal(id, name) = app.sel.get_untracked() else {
            return;
        };
        if id != msg_id {
            return;
        }
        let delta: i64 = match ev.key().as_str() {
            "ArrowLeft" => 1,
            "ArrowRight" => -1,
            "ArrowUp" => -8,
            "ArrowDown" => 8,
            _ => return,
        };
        ev.prevent_default();
        app.edit_coalesce(&format!("nudge:{id}:{name}"), "move signal", |d| {
            let Some(m) = d.message_mut(id) else { return };
            let size = m.size;
            if let Some(s) = m.signal_mut(&name) {
                let mut c = s.clone();
                let start = c.start_bit as i64 + delta;
                if start >= 0 {
                    c.start_bit = start as u32;
                    if layout::fits(&c, size) {
                        *s = c;
                    }
                }
            }
        });
    };

    view! {
        {move || {
            let m = msg.get()?;
            let vals = layout::mux_values(&m);
            if vals.is_empty() {
                return None;
            }
            let sw = m.multiplexor().cloned()?;
            Some(view! {
                <div class="seg" style="flex-wrap:wrap" title="Multiplexor branch">
                    {vals
                        .into_iter()
                        .map(|v| {
                            let label = sw.value_label(v as i64).map(|l| format!("{v} · {l}")).unwrap_or_else(|| format!("{} = {v}", sw.name));
                            view! {
                                <button class:on=move || branch.get() == Some(v) on:click=move |_| app.mux_branch.set(Some(v))>
                                    {label}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
            })
        }}
        <div
            class="bitgrid"
            tabindex="0"
            style="margin-top:12px"
            on:pointerdown=on_down
            on:pointermove=on_move
            on:pointerup=on_up
            on:pointercancel=move |_| drag.set(None)
            on:keydown=on_key
        >
            <div class="bg-head">
                <div></div>
                {(0..8).rev().map(|b| view! { <div>{b}</div> }).collect_view()}
            </div>
            {move || {
                let m = shown.get()?;
                let sel = app.sel.get();
                let d = drag.get();
                Some(render_rows(&m, branch.get(), &sel, d.as_ref()))
            }}
        </div>
        <div class="bg-legend">
            <span><b class="mono">"MSB"</b>" / "<b class="mono">"LSB"</b>" mark each signal’s ends"</span>
            <span><Icon icon=Ic::Arrow class="ic-sm"/>"Drag to move · drag the edge to resize · drag empty cells to create"</span>
            <span><span class="kbd">"←"</span><span class="kbd">"→"</span><span class="kbd">"↑"</span><span class="kbd">"↓"</span>" nudge selected"</span>
        </div>
    }
}

fn render_rows(
    m: &Message,
    branch: Option<u64>,
    sel: &Sel,
    drag: Option<&Drag>,
) -> impl IntoView + use<> {
    let total = (m.size * 8) as usize;
    let mut occ: Vec<Vec<usize>> = vec![Vec::new(); total];
    let visible: Vec<bool> = m
        .signals
        .iter()
        .map(|s| layout::in_branch(m, s, branch))
        .collect();
    let mut seg_count = vec![0usize; m.signals.len()];
    for (i, s) in m.signals.iter().enumerate() {
        if !visible[i] {
            continue;
        }
        for b in signal_bits(s) {
            if let Some(o) = occ.get_mut(b as usize) {
                o.push(i);
            }
        }
    }
    let creating: Option<(u32, u32)> = drag.and_then(|d| match d.kind {
        DragKind::Create { anchor } => {
            let e = d.create_to.unwrap_or(anchor);
            Some((anchor.min(e), anchor.max(e)))
        }
        _ => None,
    });
    let dragging_sig = drag
        .and_then(|d| d.sig)
        .filter(|_| drag.is_some_and(|d| d.moved));
    let selected_name = match sel {
        Sel::Signal(id, n) if *id == m.id => Some(n.clone()),
        _ => None,
    };
    let ends: Vec<(u32, u32)> = m.signals.iter().map(layout::lsb_msb).collect();

    // Build segments per row.
    struct Seg {
        left_bit: u32,
        span: u32,
        sigs: Vec<usize>,
    }
    let mut rows: Vec<Vec<Seg>> = Vec::new();
    for byte in 0..m.size {
        let mut segs: Vec<Seg> = Vec::new();
        for col in (0..8u32).rev() {
            let bit = byte * 8 + col;
            let o = occ[bit as usize].clone();
            match segs.last_mut() {
                Some(last) if !o.is_empty() && last.sigs == o => last.span += 1,
                _ => segs.push(Seg {
                    left_bit: bit,
                    span: 1,
                    sigs: o,
                }),
            }
        }
        for s in &segs {
            if let Some(&i) = s.sigs.first() {
                seg_count[i] = seg_count[i].max(s.span as usize);
            }
        }
        rows.push(segs);
    }
    let mut labelled = vec![false; m.signals.len()];

    rows.into_iter()
        .enumerate()
        .map(|(byte, segs)| {
            let cells = segs
                .into_iter()
                .map(|s| {
                    if s.sigs.is_empty() {
                        let bit = s.left_bit;
                        let hl = creating.is_some_and(|(lo, hi)| bit >= lo && bit <= hi);
                        return view! {
                            <div class="bg-cell" class:creating=hl data-bit=bit.to_string() data-span="1" title=format!("bit {bit}")>
                                {bit}
                            </div>
                        }
                        .into_any();
                    }
                    let i = s.sigs[0];
                    let sig = &m.signals[i];
                    let overlap = s.sigs.len() > 1;
                    let (lsb, msb) = ends[i];
                    let right_bit = s.left_bit + 1 - s.span;
                    let has_msb = (right_bit..=s.left_bit).contains(&msb);
                    let has_lsb = (right_bit..=s.left_bit).contains(&lsb);
                    // Label the widest segment of each signal once.
                    let show_label = !labelled[i] && s.span as usize == seg_count[i];
                    if show_label {
                        labelled[i] = true;
                    }
                    let far = far_end(sig);
                    let handle = (right_bit..=s.left_bit).contains(&far).then(|| {
                        let side = if s.left_bit - far < far - right_bit + 1 { "left" } else { "right" };
                        let side = if far == s.left_bit { "left" } else if far == right_bit { "right" } else { side };
                        view! { <span class=format!("bg-handle {side}") title="Drag to resize"></span> }
                    });
                    let names: Vec<String> = s.sigs.iter().map(|j| m.signals[*j].name.clone()).collect();
                    let label = if overlap { names.join(" ⚠ ") } else { sig.name.clone() };
                    let selected = selected_name.as_deref() == Some(sig.name.as_str());
                    let span_style = format!("grid-column: span {};{}", s.span, hue_style(i));
                    let card = Card::Signal(m.id, sig.name.clone());
                    view! {
                        <div
                            class="bg-seg"
                            class:sel=selected
                            class:overlap=overlap
                            class:dragging=dragging_sig == Some(i)
                            style=span_style
                            data-sig=i.to_string()
                            data-bit=s.left_bit.to_string()
                            data-span=s.span.to_string()
                            on:mouseenter=hover_in(card)
                            on:mouseleave=hover_out()
                        >
                            {has_msb.then(|| {
                                let style = if msb == s.left_bit { "" } else { "left:auto;right:5px" };
                                view! { <span class="bg-mark msb" style=style>"MSB"</span> }
                            })}
                            {(has_lsb && sig.size > 1).then(|| {
                                let style = if lsb == right_bit { "" } else { "right:auto;left:5px" };
                                view! { <span class="bg-mark lsb" style=style>"LSB"</span> }
                            })}
                            <span class="bg-seg-label" style=if show_label || overlap { "" } else { "opacity:.55;font-weight:500" }>
                                {if show_label || overlap || s.span >= 3 { label } else { String::new() }}
                            </span>
                            {show_label.then(|| view! { <span class="bg-seg-bits">{format!("{}|{}", sig.start_bit, sig.size)}</span> })}
                            {handle}
                        </div>
                    }
                    .into_any()
                })
                .collect_view();
            view! {
                <div class="bg-row">
                    <div class="bg-byte"><b>{format!("B{byte}")}</b><span>{format!("{}–{}", byte * 8 + 7, byte * 8)}</span></div>
                    {cells}
                </div>
            }
        })
        .collect_view()
}
