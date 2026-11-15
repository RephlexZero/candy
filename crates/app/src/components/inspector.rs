//! Right pane: details and deep editing for the current selection.

use candy_dbc::layout;
use candy_dbc::lint::{self, Severity, Target};
use candy_dbc::naming::{self, Ctx};
use candy_dbc::{ByteOrder, Mux, Signal as DbcSignal, ValueEntry, ValueType, stats};
use leptos::prelude::*;

use crate::actions;
use crate::components::attrs::{AttrEditor, Owner};
use crate::components::main_view::{EntriesEdit, ValueEntries};
use crate::icons::{Ic, Icon};
use crate::state::{Card, Modal, Sel, ToastKind, use_app};
use crate::ui::{Field, NumIn, TextIn, hover_in, hover_out, hue_style, num, plural};

/// Change byte order while keeping the signal's least significant bit where it is.
pub fn set_byte_order(s: &mut DbcSignal, big: bool) {
    let (lsb, _) = layout::lsb_msb(s);
    match (s.byte_order, big) {
        (ByteOrder::LittleEndian, true) => {
            s.byte_order = ByteOrder::BigEndian;
            s.start_bit = layout::motorola_start_from_lsb(lsb, s.size);
        }
        (ByteOrder::BigEndian, false) => {
            s.byte_order = ByteOrder::LittleEndian;
            s.start_bit = lsb;
        }
        _ => {}
    }
}

#[component]
pub fn Inspector() -> impl IntoView {
    let app = use_app();
    move || match app.sel.get() {
        Sel::Network => view! { <NetworkInspector/> }.into_any(),
        Sel::Message(id) => view! { <MessageInspector id=id/> }.into_any(),
        Sel::Signal(id, name) => view! { <SignalInspector id=id name=name/> }.into_any(),
        Sel::Node(n) => view! { <NodeInspector name=n/> }.into_any(),
        Sel::ValueTable(v) => view! { <ValueTableInspector name=v/> }.into_any(),
    }
}

fn section(title: &'static str, icon: Ic, children: impl IntoView) -> impl IntoView {
    view! {
        <div class="insp-section">
            <div class="insp-title"><Icon icon=icon class="ic-sm"/><span class="eyebrow">{title}</span></div>
            {children}
        </div>
    }
}

/// Issues for one target, each with its fix button.
#[component]
fn IssueList(pred: fn(&Target, &Target) -> bool, target: Target) -> impl IntoView {
    let app = use_app();
    let issues = Memo::new(move |_| {
        app.issues.with(|is| {
            is.iter()
                .filter(|i| pred(&i.target, &target))
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    move || {
        let list = issues.get();
        if list.is_empty() {
            return None;
        }
        Some(section(
            "Problems",
            Ic::Warning,
            list.into_iter()
                .map(|i| {
                    let icon = match i.severity {
                        Severity::Error => Ic::Error,
                        Severity::Warning => Ic::Warning,
                        Severity::Info => Ic::Info,
                    };
                    let cls = match i.severity {
                        Severity::Error => "err",
                        Severity::Warning => "warn",
                        Severity::Info => "info",
                    };
                    let fix = i.fix.clone();
                    view! {
                        <div class="suggest" style="margin-bottom:6px" on:mouseenter=hover_in(Card::Rule(i.rule)) on:mouseleave=hover_out()>
                            <span class=cls><Icon icon=icon class="ic-sm"/></span>
                            <span class="grow">{i.message.clone()}</span>
                            {fix.map(|f| {
                                let label = f.label();
                                view! {
                                    <button class="btn btn-sm" title=label on:click=move |_| {
                                        let f = f.clone();
                                        app.edit(&f.label().to_lowercase(), |d| { lint::apply_fix(d, &f); });
                                        app.fix_selection();
                                    }><Icon icon=Ic::Wand class="ic-sm"/>"Fix"</button>
                                }
                            })}
                        </div>
                    }
                })
                .collect_view(),
        ))
    }
}

// ───────────────────────────── Signal ─────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum MuxRole {
    None,
    Switch,
    Branch,
    BranchSwitch,
}

#[component]
fn SignalInspector(id: u32, name: String) -> impl IntoView {
    let app = use_app();
    let sig = Memo::new({
        let name = name.clone();
        move |_| {
            app.doc
                .with(|d| d.message(id).and_then(|m| m.signal(&name)).cloned())
        }
    });
    let msg_name = Memo::new(move |_| {
        app.doc.with(|d| {
            d.message(id)
                .map(|m| (m.name.clone(), m.transmitter.clone(), m.size))
        })
    });
    let index = Memo::new({
        let name = name.clone();
        move |_| {
            app.doc.with(|d| {
                d.message(id)
                    .and_then(|m| m.signals.iter().position(|s| s.name == name))
                    .unwrap_or(0)
            })
        }
    });
    let edit = {
        let name = name.clone();
        move |label: &'static str, f: Box<dyn FnOnce(&mut DbcSignal)>| {
            app.edit_coalesce(&format!("insp:{id}:{name}:{label}"), label, |d| {
                if let Some(s) = d.message_mut(id).and_then(|m| m.signal_mut(&name)) {
                    f(s);
                }
            });
        }
    };
    let get = move |f: fn(&DbcSignal) -> f64| {
        Signal::derive(move || sig.with(|s| s.as_ref().map(f).unwrap_or(0.0)))
    };

    // Naming suggestion for this signal under the active convention.
    let suggestion = Memo::new({
        let name = name.clone();
        move |_| {
            let conv = app.convention.get()?;
            let (m, tx, _) = msg_name.get()?;
            let ctx = Ctx {
                transmitter: Some(&tx),
                message: Some(&m),
            };
            naming::check_name(&conv.signal, &name, &ctx, conv.corner_suffixes)
        }
    });

    let rename = {
        let name = name.clone();
        move |v: String| {
            let v = v.trim().to_string();
            if v.is_empty() || v == name {
                return;
            }
            if app
                .doc
                .with_untracked(|d| d.message(id).is_some_and(|m| m.signal(&v).is_some()))
            {
                app.toast(
                    format!("{v} already exists in this message"),
                    ToastKind::Error,
                );
                return;
            }
            let old = name.clone();
            app.edit(&format!("rename {old}"), |d| d.rename_signal(id, &old, &v));
            app.select(Sel::Signal(id, v));
        }
    };
    let rename2 = rename.clone();

    // Calculator state.
    let calc_raw = RwSignal::new(0.0f64);

    let role = Memo::new(move |_| match sig.with(|s| s.as_ref().map(|s| s.mux)) {
        Some(Mux::Multiplexor) => MuxRole::Switch,
        Some(Mux::Multiplexed(_)) => MuxRole::Branch,
        Some(Mux::MultiplexedMultiplexor(_)) => MuxRole::BranchSwitch,
        _ => MuxRole::None,
    });
    let selector =
        Memo::new(move |_| sig.with(|s| s.as_ref().and_then(|s| s.mux.selector()).unwrap_or(0)));

    let (
        e_order,
        e_type,
        e_start,
        e_size,
        e_role,
        e_sel,
        e_factor,
        e_offset,
        e_min,
        e_max,
        e_unit,
        e_fit,
        e_comment,
        e_vt,
        e_copy,
        e_rx,
    ) = (
        edit.clone(),
        edit.clone(),
        edit.clone(),
        edit.clone(),
        edit.clone(),
        edit.clone(),
        edit.clone(),
        edit.clone(),
        edit.clone(),
        edit.clone(),
        edit.clone(),
        edit.clone(),
        edit.clone(),
        edit.clone(),
        edit.clone(),
        edit.clone(),
    );
    let e_add_vt = e_vt.clone();
    let name_for_issues = name.clone();
    let name_for_attrs = name.clone();
    let name_val = name.clone();

    view! {
            <div class="insp-head">
                <span class="dot" style=move || hue_style(index.get())></span>
                <div style="min-width:0">
                    <div class="t mono">{name_val.clone()}</div>
                    <div class="s">{move || msg_name.get().map(|m| format!("in {}", m.0)).unwrap_or_default()}</div>
                </div>
            </div>

            {section("Identity", Ic::Tag, view! {
                <Field label="Name">
                    <TextIn class="input mono" value=Signal::derive({ let n = name.clone(); move || n.clone() }) on_commit=rename/>
                </Field>
                {move || suggestion.get().map(|(reasons, sugg)| {
                    let r = rename2.clone();
                    let s2 = sugg.clone();
                    view! {
                        <div class="suggest" style="margin-top:8px">
                            <Icon icon=Ic::Sparkles class="ic-sm"/>
                            <div class="grow">
                                <div>"Convention suggests "<code class="mono">{sugg}</code></div>
                                <div class="hint">{reasons.join("; ")}</div>
                            </div>
                            <button class="btn btn-sm" on:click=move |_| r(s2.clone())>"Apply"</button>
                        </div>
                    }
                })}
                <div style="margin-top:10px">
                    <Field label="Comment">
                        <textarea class="textarea" rows="2" placeholder="Describe the signal"
                            prop:value=move || sig.with(|s| s.as_ref().and_then(|s| s.comment.clone()).unwrap_or_default())
                            on:change=move |ev| {
                                let v = event_target_value(&ev);
                                e_comment("edit comment", Box::new(move |s| s.comment = (!v.trim().is_empty()).then_some(v)));
                            }
                        ></textarea>
                    </Field>
                </div>
            })}

            {section("Layout", Ic::Hash, view! {
                <div class="row2">
                    <Field label="Start bit" hint="Intel: LSB · Motorola: MSB">
                        <NumIn integer=true value=get(|s| s.start_bit as f64) on_commit=move |v| e_start("move signal", Box::new(move |s| s.start_bit = v.max(0.0) as u32))/>
                    </Field>
                    <Field label="Size (bits)">
                        <NumIn integer=true value=get(|s| s.size as f64) on_commit=move |v| e_size("resize signal", Box::new(move |s| s.size = v.clamp(1.0, 64.0) as u32))/>
                    </Field>
                </div>
                <div class="row2" style="margin-top:10px">
                    <Field label="Byte order">
                        <select class="select" on:change=move |ev| {
                            let big = event_target_value(&ev) == "be";
                            e_order("change byte order", Box::new(move |s| set_byte_order(s, big)));
                        }>
                            <option value="le" selected=move || sig.with(|s| s.as_ref().is_some_and(|s| s.byte_order == ByteOrder::LittleEndian))>"Intel (little endian)"</option>
                            <option value="be" selected=move || sig.with(|s| s.as_ref().is_some_and(|s| s.byte_order == ByteOrder::BigEndian))>"Motorola (big endian)"</option>
                        </select>
                    </Field>
                    <Field label="Value type">
                        <select class="select" on:change=move |ev| {
                            let vt = match event_target_value(&ev).as_str() {
                                "s" => ValueType::Signed,
                                "f" => ValueType::Float32,
                                "d" => ValueType::Float64,
                                _ => ValueType::Unsigned,
                            };
                            e_type("change value type", Box::new(move |s| {
                                s.value_type = vt;
                                if vt == ValueType::Float32 { s.size = 32 }
                                if vt == ValueType::Float64 { s.size = 64 }
                            }));
                        }>
                            {[("u", "Unsigned", ValueType::Unsigned), ("s", "Signed", ValueType::Signed), ("f", "Float (32-bit)", ValueType::Float32), ("d", "Double (64-bit)", ValueType::Float64)]
                                .into_iter()
                                .map(|(v, l, t)| view! { <option value=v selected=move || sig.with(|s| s.as_ref().is_some_and(|s| s.value_type == t))>{l}</option> })
                                .collect_view()}
                        </select>
                    </Field>
                </div>
                {move || {
                    let s = sig.get()?;
                    let (_, _, size) = msg_name.get()?;
                    let bits = layout::signal_bits(&s);
                    let (lsb, msb) = layout::lsb_msb(&s);
                    let total = (size * 8) as usize;
                    Some(view! {
                        <div class="mini-bits" style=hue_style(index.get()) title=format!("LSB {lsb} · MSB {msb}")>
                            {(0..total).map(|i| {
                                // Draw rows as bytes with bit 7 on the left, like the main grid.
                                let byte = i / 8;
                                let bit = byte * 8 + 7 - i % 8;
                                view! { <div class:on=bits.contains(&(bit as u32))></div> }
                            }).collect_view()}
                        </div>
                        <div class="hint" style="margin-top:6px">{format!("Bits {lsb}…{msb} · uses {} byte{}", layout::bytes_needed(&s), if layout::bytes_needed(&s) == 1 { "" } else { "s" })}</div>
                    })
                }}
            })}

            {section("Multiplexing", Ic::Mux, view! {
                <div class="row2">
                    <Field label="Role">
                        <select class="select" on:change=move |ev| {
                            let sel = selector.get_untracked();
                            let mux = match event_target_value(&ev).as_str() {
                                "switch" => Mux::Multiplexor,
                                "branch" => Mux::Multiplexed(sel),
                                "both" => Mux::MultiplexedMultiplexor(sel),
                                _ => Mux::None,
                            };
                            e_role("change mux role", Box::new(move |s| s.mux = mux));
                        }>
                            {[("none", "Always present", MuxRole::None), ("switch", "Switch (M)", MuxRole::Switch), ("branch", "Multiplexed (mN)", MuxRole::Branch), ("both", "Nested switch (mNM)", MuxRole::BranchSwitch)]
                                .into_iter()
                                .map(|(v, l, r)| view! { <option value=v selected=move || role.get() == r>{l}</option> })
                                .collect_view()}
                        </select>
                    </Field>
                    {move || matches!(role.get(), MuxRole::Branch | MuxRole::BranchSwitch).then(|| {
                        let e = e_sel.clone();
                        view! {
                        <Field label="When switch =">
                            <NumIn integer=true value=Signal::derive(move || selector.get() as f64) on_commit={
                                move |v| {
                                    let v = v.max(0.0) as u64;
                                    e("change mux value", Box::new(move |s| s.mux = match s.mux {
                                        Mux::MultiplexedMultiplexor(_) => Mux::MultiplexedMultiplexor(v),
                                        _ => Mux::Multiplexed(v),
                                    }));
                                }
                            }/>
                        </Field>
                        }
                    })}
                </div>
                {move || {
                    let s = sig.get()?;
                    if s.ext_mux.is_empty() {
                        return None;
                    }
                    Some(view! {
                        <div class="hint" style="margin-top:8px">
                            "Extended multiplexing (SG_MUL_VAL_): "
                            {s.ext_mux.iter().map(|e| format!("{} ∈ {}", e.switch, e.ranges.iter().map(|(a, b)| if a == b { a.to_string() } else { format!("{a}–{b}") }).collect::<Vec<_>>().join(", "))).collect::<Vec<_>>().join("; ")}
                        </div>
                    })
                }}
            })}

            {section("Scaling", Ic::Zap, view! {
                <div class="formula">{move || sig.with(|s| s.as_ref().map(|s| s.formula()).unwrap_or_default())}</div>
                <div class="row2" style="margin-top:10px">
                    <Field label="Factor"><NumIn value=get(|s| s.factor) on_commit=move |v| e_factor("edit factor", Box::new(move |s| s.factor = v))/></Field>
                    <Field label="Offset"><NumIn value=get(|s| s.offset) on_commit=move |v| e_offset("edit offset", Box::new(move |s| s.offset = v))/></Field>
                </div>
                <div class="row3" style="margin-top:10px">
                    <Field label="Min"><NumIn value=get(|s| s.min) on_commit=move |v| e_min("edit min", Box::new(move |s| s.min = v))/></Field>
                    <Field label="Max"><NumIn value=get(|s| s.max) on_commit=move |v| e_max("edit max", Box::new(move |s| s.max = v))/></Field>
                    <Field label="Unit">
                        <TextIn placeholder="—" value=Signal::derive(move || sig.with(|s| s.as_ref().map(|s| s.unit.clone()).unwrap_or_default())) on_commit=move |v| e_unit("edit unit", Box::new(move |s| s.unit = v))/>
                    </Field>
                </div>
                <div style="display:flex;gap:8px;align-items:center;margin-top:8px">
                    <span class="hint grow">{move || sig.with(|s| s.as_ref().map(|s| {
                        let (lo, hi) = s.physical_range();
                        format!("Representable: {} … {}", num(lo), num(hi))
                    }).unwrap_or_default())}</span>
                    <button class="btn btn-sm btn-ghost" title="Set min/max to the full representable range" on:click=move |_| {
                        e_fit("fit range", Box::new(|s| {
                            let (lo, hi) = s.physical_range();
                            s.min = lint::round_sig(lo);
                            s.max = lint::round_sig(hi);
                        }))
                    }>"Fit range"</button>
                </div>
                <div class="calc" style="margin-top:10px">
                    <Field label="Raw">
                        <NumIn value=Signal::derive(move || calc_raw.get()) on_commit=move |v| calc_raw.set(v)/>
                    </Field>
                    <span class="arrow">"⇄"</span>
                    <Field label="Physical">
                        <NumIn value=Signal::derive(move || sig.with(|s| s.as_ref().map(|s| s.to_physical(calc_raw.get())).unwrap_or(0.0))) on_commit=move |v| {
                            if let Some(s) = sig.get_untracked() {
                                calc_raw.set(s.to_raw(v).round());
                            }
                        }/>
                    </Field>
                </div>
                {move || sig.with(|s| s.as_ref().and_then(|s| s.value_label(calc_raw.get() as i64).map(|l| view! { <div class="hint" style="margin-top:4px">"Label: "<b>{l.to_string()}</b></div> })))}
            })}

            {section("Value descriptions", Ic::Table, view! {
                <ValueEntries entries=Signal::derive(move || sig.with(|s| s.as_ref().map(|s| s.value_descriptions.clone()).unwrap_or_default())) on_change={
                    let e = e_vt.clone();
                    move |f: EntriesEdit| e("edit value descriptions", Box::new(move |s| f(&mut s.value_descriptions)))
                }/>
                <div style="display:flex;gap:6px;margin-top:8px">
                    <button class="btn btn-sm" on:click=move |_| e_add_vt("add value", Box::new(|s| {
                        let next = s.value_descriptions.iter().map(|x| x.value + 1).max().unwrap_or(0);
                        s.value_descriptions.push(ValueEntry::new(next, format!("Value{next}")));
                    }))><Icon icon=Ic::Plus class="ic-sm"/>"Add"</button>
                    {move || {
                        let tables = app.doc.with(|d| d.value_tables.iter().map(|t| t.name.clone()).collect::<Vec<_>>());
                        (!tables.is_empty()).then(|| {
                            let e = e_copy.clone();
                            view! {
                                <select class="select" style="flex:1" on:change=move |ev| {
                                    let t = event_target_value(&ev);
                                    let entries = app.doc.with_untracked(|d| d.value_tables.iter().find(|x| x.name == t).map(|x| x.entries.clone()));
                                    if let Some(entries) = entries {
                                        e("use value table", Box::new(move |s| s.value_descriptions = entries));
                                    }
                                }>
                                    <option value="" selected=true>"Copy from value table…"</option>
                                    {tables.into_iter().map(|t| view! { <option value=t.clone()>{t.clone()}</option> }).collect_view()}
                                </select>
                            }
                        })
                    }}
                </div>
            })}

            {section("Receivers", Ic::Node, view! {
                <div class="chips">
                    {move || {
                        let nodes = app.doc.with(|d| d.nodes.iter().map(|n| n.name.clone()).collect::<Vec<_>>());
                        if nodes.is_empty() {
                            return view! { <span class="hint">"Add nodes to choose receivers."</span> }.into_any();
                        }
                        let rx = sig.with(|s| s.as_ref().map(|s| s.receivers.clone()).unwrap_or_default());
                        nodes.into_iter().map(|n| {
                            let on = rx.contains(&n);
                            let e = e_rx.clone();
                            let n2 = n.clone();
                            view! {
                                <button class="chip" class:on=on on:click=move |_| {
                                    let n = n2.clone();
                                    e("toggle receiver", Box::new(move |s| {
                                        s.receivers.retain(|r| r != candy_dbc::NO_NODE);
                                        if let Some(p) = s.receivers.iter().position(|r| *r == n) { s.receivers.remove(p); } else { s.receivers.push(n); }
                                        if s.receivers.is_empty() { s.receivers.push(candy_dbc::NO_NODE.into()); }
                                    }));
                                }>{n}</button>
                            }
                        }).collect_view().into_any()
                    }}
                </div>
            })}

            <IssueList pred={|t, me| match (t, me) {
                (Target::Signal(a, x), Target::Signal(b, y)) | (Target::ValueLabel(a, x, _), Target::Signal(b, y)) => a == b && x == y,
                _ => false,
            }} target=Target::Signal(id, name_for_issues.clone())/>

            {section("Attributes", Ic::Tag, view! { <div class="fields" style="grid-template-columns:1fr"><AttrEditor owner=Owner::Signal(id, name_for_attrs.clone())/></div> })}
    }
}

// ───────────────────────────── Message ─────────────────────────────

#[component]
fn MessageInspector(id: u32) -> impl IntoView {
    let app = use_app();
    let msg = Memo::new(move |_| app.doc.with(|d| d.message(id).cloned()));
    let load = Memo::new(move |_| {
        app.doc.with(|d| {
            d.message(id)
                .and_then(|m| stats::message_load(d, m, stats::bitrate(d)))
        })
    });
    let groups = Memo::new(move |_| {
        app.doc.with(|d| {
            d.signal_groups
                .iter()
                .filter(|g| g.message_id == id)
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    view! {
        <Show when=move || msg.with(|m| m.is_some())>
            <div class="insp-head">
                <span class="page-icon" style="width:30px;height:30px"><Icon icon=Ic::Message class="ic-sm"/></span>
                <div style="min-width:0">
                    <div class="t mono">{move || msg.with(|m| m.as_ref().map(|m| m.name.clone()).unwrap_or_default())}</div>
                    <div class="s">{move || msg.with(|m| m.as_ref().map(|m| format!("{} · {} bytes", m.id_hex(), m.size)).unwrap_or_default())}</div>
                </div>
            </div>
            {section("Summary", Ic::Info, view! {
                {move || {
                    let m = msg.get()?;
                    let j = m.j1939();
                    Some(view! {
                        <dl class="kv">
                            <dt>"Id (hex)"</dt><dd>{m.id_hex()}</dd>
                            <dt>"Id (dec)"</dt><dd>{m.can_id().to_string()}</dd>
                            <dt>"Raw DBC id"</dt><dd>{m.id.to_string()}</dd>
                            {j.map(|j| view! {
                                <dt>"J1939 PGN"</dt><dd>{format!("{} (0x{:05X})", j.pgn, j.pgn)}</dd>
                                <dt>"Priority"</dt><dd>{j.priority.to_string()}</dd>
                                <dt>"Source addr."</dt><dd>{format!("0x{:02X}", j.source)}</dd>
                                {j.destination.map(|d| view! { <dt>"Destination"</dt><dd>{format!("0x{d:02X}")}</dd> })}
                            })}
                            <dt>"Signals"</dt><dd>{m.signals.len().to_string()}</dd>
                            <dt>"Payload used"</dt><dd>{format!("{:.0}%", layout::utilisation(&m) * 100.0)}</dd>
                            <dt>"Frame bits"</dt><dd>{format!("≤ {}", stats::frame_bits(&m))}</dd>
                            <dt>"Bus load"</dt><dd>{load.get().map(|l| format!("{:.2}%", l * 100.0)).unwrap_or_else(|| "—".into())}</dd>
                        </dl>
                    })
                }}
            })}
            {move || {
                let g = groups.get();
                (!g.is_empty()).then(|| section("Signal groups", Ic::Folder, g.into_iter().map(|g| view! {
                    <div style="margin-bottom:8px">
                        <div class="mono" style="font-weight:600">{g.name.clone()}<span class="hint">{format!(" · rep {}", g.repetitions)}</span></div>
                        <div class="chips" style="margin-top:4px">
                            {g.signals.iter().map(|s| {
                                let s2 = s.clone();
                                view! { <button class="chip mono" on:click=move |_| app.select(Sel::Signal(id, s2.clone()))>{s.clone()}</button> }
                            }).collect_view()}
                        </div>
                    </div>
                }).collect_view()))
            }}
            <IssueList pred={|t, me| match (t, me) {
                (Target::Message(a), Target::Message(b)) | (Target::SignalGroup(a, _), Target::Message(b)) => a == b,
                (Target::Signal(a, _), Target::Message(b)) | (Target::ValueLabel(a, _, _), Target::Message(b)) => a == b,
                _ => false,
            }} target=Target::Message(id)/>
            {section("Attributes", Ic::Tag, view! { <div class="fields" style="grid-template-columns:1fr"><AttrEditor owner=Owner::Message(id)/></div> })}
        </Show>
    }
}

// ───────────────────────────── Node / table / network ─────────────────────────────

#[component]
fn NodeInspector(name: String) -> impl IntoView {
    let app = use_app();
    let n = name.clone();
    let comment = Memo::new(move |_| {
        app.doc.with(|d| {
            d.node(&n)
                .and_then(|x| x.comment.clone())
                .unwrap_or_default()
        })
    });
    let n_edit = name.clone();
    view! {
        <div class="insp-head">
            <span class="page-icon" style="width:30px;height:30px"><Icon icon=Ic::Node class="ic-sm"/></span>
            <div><div class="t mono">{name.clone()}</div><div class="s">"Node (ECU)"</div></div>
        </div>
        {section("Comment", Ic::Info, view! {
            <textarea class="textarea" rows="3" prop:value=move || comment.get() on:change=move |ev| {
                let v = event_target_value(&ev);
                let n = n_edit.clone();
                app.edit("edit comment", |d| if let Some(x) = d.nodes.iter_mut().find(|x| x.name == n) { x.comment = (!v.trim().is_empty()).then_some(v) });
            }></textarea>
        })}
        <IssueList pred={|t, me| t == me} target=Target::Node(name.clone())/>
        {section("Attributes", Ic::Tag, view! { <div class="fields" style="grid-template-columns:1fr"><AttrEditor owner=Owner::Node(name.clone())/></div> })}
    }
}

#[component]
fn ValueTableInspector(name: String) -> impl IntoView {
    let app = use_app();
    let n = name.clone();
    let info = Memo::new(move |_| {
        app.doc.with(|d| {
            d.value_tables.iter().find(|t| t.name == n).map(|t| {
                (
                    t.entries.len(),
                    t.entries.iter().map(|e| e.value).min(),
                    t.entries.iter().map(|e| e.value).max(),
                )
            })
        })
    });
    view! {
        <div class="insp-head">
            <span class="page-icon" style="width:30px;height:30px"><Icon icon=Ic::Table class="ic-sm"/></span>
            <div><div class="t mono">{name.clone()}</div><div class="s">"Value table"</div></div>
        </div>
        {section("Summary", Ic::Info, view! {
            {move || info.get().map(|(n, lo, hi)| view! {
                <dl class="kv">
                    <dt>"Entries"</dt><dd>{n.to_string()}</dd>
                    <dt>"Lowest"</dt><dd>{lo.map(|v| v.to_string()).unwrap_or_default()}</dd>
                    <dt>"Highest"</dt><dd>{hi.map(|v| v.to_string()).unwrap_or_default()}</dd>
                    <dt>"Needs"</dt><dd>{hi.map(|v| format!("{} bits", 64 - (v.max(1) as u64).leading_zeros())).unwrap_or_default()}</dd>
                </dl>
            })}
        })}
        <IssueList pred={|t, me| t == me} target=Target::ValueTable(name.clone())/>
    }
}

#[component]
fn NetworkInspector() -> impl IntoView {
    let app = use_app();
    let counts = Memo::new(move |_| app.issues_for(|_| true));
    let fixable = Memo::new(move |_| {
        app.issues
            .with(|i| i.iter().filter(|i| i.fix.is_some()).count())
    });
    view! {
        <div class="insp-head">
            <span class="page-icon" style="width:30px;height:30px"><Icon icon=Ic::Network class="ic-sm"/></span>
            <div><div class="t">"Network"</div><div class="s">{move || app.file_name.get()}</div></div>
        </div>
        {section("Health", Ic::Gauge, view! {
            {move || {
                let (e, w) = counts.get();
                view! {
                    <dl class="kv">
                        <dt>"Errors"</dt><dd class={if e > 0 { "err" } else { "ok" }}>{e.to_string()}</dd>
                        <dt>"Warnings"</dt><dd class={if w > 0 { "warn" } else { "ok" }}>{w.to_string()}</dd>
                        <dt>"Auto-fixable"</dt><dd>{fixable.get().to_string()}</dd>
                    </dl>
                }
            }}
            <div style="display:flex;gap:6px;margin-top:10px;flex-wrap:wrap">
                <button class="btn btn-sm" on:click=move |_| actions::open_dock(app, crate::state::DockTab::Problems)><Icon icon=Ic::Warning class="ic-sm"/>"Problems"</button>
                <Show when={move || fixable.get() > 0}>
                    <button class="btn btn-sm btn-primary" on:click=move |_| actions::fix_all(app, None)><Icon icon=Ic::Wand class="ic-sm"/>{move || format!("Fix {}", plural(fixable.get(), "issue"))}</button>
                </Show>
            </div>
        })}
        {section("Naming convention", Ic::Tag, view! {
            <div class="hint" style="margin-bottom:8px">{move || app.convention.get().map(|c| c.description).unwrap_or_else(|| "Pick a convention to check names and get rename suggestions. It is saved inside the DBC.".into())}</div>
            <button class="btn btn-sm" on:click=move |_| app.modal.set(Some(Modal::Naming))>
                <Icon icon=Ic::Tag class="ic-sm"/>{move || app.convention.get().map(|c| c.name).unwrap_or_else(|| "Choose…".into())}
            </button>
        })}
        {section("Network attributes", Ic::Tag, view! { <div class="fields" style="grid-template-columns:1fr"><AttrEditor owner=Owner::Network/></div> })}
    }
}
