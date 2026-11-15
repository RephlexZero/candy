//! Centre pane: network overview, message editor (header, bit grid, signal table), node and
//! value-table pages.

use candy_dbc::layout::{self, FD_SIZES};
use candy_dbc::{AttrKind, AttrValue, ByteOrder, Mux, ValueEntry, ValueType, set_attribute, stats};
use leptos::prelude::*;

use crate::actions::{self, GROUP_ATTR};
use crate::components::bitgrid::BitGrid;
use crate::icons::{Ic, Icon};
use crate::state::{Card, Sel, ToastKind, use_app};
use crate::ui::{
    Field, NumIn, TextIn, fmt_id, hover_in, hover_out, hue_style, num, parse_id, plural,
};

/// A deferred edit of a value → label list.
pub type EntriesEdit = Box<dyn FnOnce(&mut Vec<ValueEntry>)>;

#[derive(Clone, PartialEq)]
enum Route {
    Network,
    Node(String),
    Message(u32),
    ValueTable(String),
}

#[component]
pub fn MainView() -> impl IntoView {
    let app = use_app();
    let route = Memo::new(move |_| match app.sel.get() {
        Sel::Network => Route::Network,
        Sel::Node(n) => Route::Node(n),
        Sel::Message(id) | Sel::Signal(id, _) => Route::Message(id),
        Sel::ValueTable(v) => Route::ValueTable(v),
    });
    move || match route.get() {
        Route::Network => view! { <NetworkPage/> }.into_any(),
        Route::Node(n) => view! { <NodePage name=n/> }.into_any(),
        Route::Message(id) => view! { <MessagePage id=id/> }.into_any(),
        Route::ValueTable(v) => view! { <ValueTablePage name=v/> }.into_any(),
    }
}

// ───────────────────────────── Network ─────────────────────────────

#[component]
fn NetworkPage() -> impl IntoView {
    let app = use_app();
    let summary = Memo::new(move |_| app.doc.with(|d| stats::summary(d, stats::bitrate(d))));
    let bitrate = Memo::new(move |_| app.doc.with(stats::bitrate));
    let loads = Memo::new(move |_| {
        app.doc.with(|d| {
            let br = stats::bitrate(d);
            let mut v: Vec<(u32, String, f64, f64)> = d
                .messages
                .iter()
                .filter_map(|m| {
                    Some((
                        m.id,
                        m.name.clone(),
                        stats::message_load(d, m, br)?,
                        d.cycle_time_ms(m)?,
                    ))
                })
                .collect();
            v.sort_by(|a, b| b.2.total_cmp(&a.2));
            v.truncate(10);
            v
        })
    });

    let stat = |icon: Ic, label: &'static str, value: Signal<String>| {
        view! {
            <div class="card stat">
                <div class="stat-label"><Icon icon=icon class="ic-sm"/>{label}</div>
                <div class="stat-value">{move || value.get()}</div>
            </div>
        }
    };

    view! {
        <div class="page">
            <div class="page-head">
                <div class="page-icon"><Icon icon=Ic::Network/></div>
                <div class="grow">
                    <h1 class="page-title">{move || app.file_name.get()}</h1>
                    <div class="page-sub">
                        <span>{move || format!("{} kbit/s", num(bitrate.get() / 1000.0))}</span>
                        <span>"·"</span>
                        <span>{move || app.convention.get().map(|c| format!("{} naming", c.name)).unwrap_or_else(|| "No naming convention".into())}</span>
                        {move || {
                            let v = app.doc.with(|d| d.version.clone());
                            (!v.is_empty()).then(|| view! { <span>"·"</span><span>{v}</span> })
                        }}
                    </div>
                </div>
                <div class="page-actions">
                    <button class="btn btn-sm" on:click=move |_| actions::add_node(app)><Icon icon=Ic::Plus class="ic-sm"/>"Node"</button>
                    <button class="btn btn-sm btn-primary" on:click=move |_| actions::add_message(app)><Icon icon=Ic::Plus class="ic-sm"/>"Message"</button>
                </div>
            </div>

            <div class="stats">
                {stat(Ic::Message, "Messages", Signal::derive(move || summary.get().messages.to_string()))}
                {stat(Ic::Signal, "Signals", Signal::derive(move || summary.get().signals.to_string()))}
                {stat(Ic::Node, "Nodes", Signal::derive(move || summary.get().nodes.to_string()))}
                {stat(Ic::Mux, "Multiplexed", Signal::derive(move || summary.get().multiplexed_messages.to_string()))}
                {stat(Ic::Gauge, "Est. bus load", Signal::derive(move || {
                    let s = summary.get();
                    if s.messages_with_cycle == 0 { "—".into() } else { format!("{:.1}%", s.bus_load * 100.0) }
                }))}
            </div>

            <div class="grid2 section-gap">
                <div class="card">
                    <div class="card-head"><Icon icon=Ic::Gauge class="ic-sm"/><h3>"Bus load by message"</h3>
                        <span class="hint">"worst-case stuffing"</span>
                    </div>
                    {move || {
                        let l = loads.get();
                        if l.is_empty() {
                            return view! { <div class="card-pad hint">"Set GenMsgCycleTime on messages to estimate bus load."</div> }.into_any();
                        }
                        let max = l.first().map(|x| x.2).unwrap_or(1.0).max(1e-9);
                        l.into_iter()
                            .map(|(id, name, load, cycle)| {
                                view! {
                                    <div class="list-row" on:click=move |_| app.select(Sel::Message(id))>
                                        <span class="grow mono">{name}</span>
                                        <span class="hint mono" style="width:60px;text-align:right">{format!("{} ms", num(cycle))}</span>
                                        <div class="load-bar" class:hot={load > 0.1} style="width:90px">
                                            <div style=format!("width:{:.1}%", load / max * 100.0)></div>
                                        </div>
                                        <span class="mono" style="width:52px;text-align:right">{format!("{:.2}%", load * 100.0)}</span>
                                    </div>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }}
                </div>

                <div class="card">
                    <div class="card-head"><Icon icon=Ic::Node class="ic-sm"/><h3>"Nodes"</h3>
                        <button class="icon-btn" title="Add node" on:click=move |_| actions::add_node(app)><Icon icon=Ic::Plus class="ic-sm"/></button>
                    </div>
                    {move || {
                        app.doc.with(|d| {
                            if d.nodes.is_empty() {
                                return view! { <div class="card-pad hint">"No nodes yet."</div> }.into_any();
                            }
                            d.nodes
                                .iter()
                                .map(|n| {
                                    let tx = d.messages.iter().filter(|m| m.transmitter == n.name).count();
                                    let rx = d.messages.iter().flat_map(|m| &m.signals).filter(|s| s.receivers.contains(&n.name)).count();
                                    let name = n.name.clone();
                                    let card = Card::Node(name.clone());
                                    let click = name.clone();
                                    view! {
                                        <div class="list-row" on:click=move |_| app.select(Sel::Node(click.clone())) on:mouseenter=hover_in(card) on:mouseleave=hover_out()>
                                            <Icon icon=Ic::Node class="ic-sm"/>
                                            <span class="grow mono">{name}</span>
                                            <span class="hint">{format!("{tx} tx · {rx} rx")}</span>
                                        </div>
                                    }
                                })
                                .collect_view()
                                .into_any()
                        })
                    }}
                </div>
            </div>

            <div class="grid2 section-gap">
                <div class="card card-pad">
                    <div class="fields" style="grid-template-columns:1fr 1fr">
                        <Field label="Bitrate (bit/s)" hint="Stored as the Baudrate attribute">
                            <NumIn value=Signal::derive(move || bitrate.get()) integer=true on_commit=move |v| {
                                app.edit("set bitrate", |d| {
                                    d.ensure_attr_def(candy_dbc::AttrObject::Network, "Baudrate", AttrKind::Int { min: 0, max: 8_000_000 }, AttrValue::Int(500_000));
                                    set_attribute(&mut d.attributes, "Baudrate", Some(AttrValue::Int(v as i64)));
                                })
                            }/>
                        </Field>
                        <Field label="Version">
                            <TextIn value=Signal::derive(move || app.doc.with(|d| d.version.clone())) on_commit=move |v| app.edit("set version", |d| d.version = v)/>
                        </Field>
                    </div>
                    <Field label="Network comment">
                        <textarea
                            class="textarea"
                            rows="3"
                            prop:value=move || app.doc.with(|d| d.comment.clone().unwrap_or_default())
                            on:change=move |ev| {
                                let v = event_target_value(&ev);
                                app.edit("edit comment", |d| d.comment = (!v.trim().is_empty()).then_some(v));
                            }
                        ></textarea>
                    </Field>
                </div>

                <div class="card">
                    <div class="card-head"><Icon icon=Ic::Table class="ic-sm"/><h3>"Value tables"</h3>
                        <button class="icon-btn" title="Add value table" on:click=move |_| actions::add_value_table(app)><Icon icon=Ic::Plus class="ic-sm"/></button>
                    </div>
                    {move || {
                        app.doc.with(|d| {
                            if d.value_tables.is_empty() {
                                return view! { <div class="card-pad hint">"Shared VAL_TABLE_ definitions appear here."</div> }.into_any();
                            }
                            d.value_tables
                                .iter()
                                .map(|t| {
                                    let name = t.name.clone();
                                    let click = name.clone();
                                    let preview = t.entries.iter().take(4).map(|e| format!("{}={}", e.value, e.label)).collect::<Vec<_>>().join(", ");
                                    view! {
                                        <div class="list-row" on:click=move |_| app.select(Sel::ValueTable(click.clone()))>
                                            <Icon icon=Ic::Table class="ic-sm"/>
                                            <span class="mono">{name}</span>
                                            <span class="grow hint">{preview}</span>
                                        </div>
                                    }
                                })
                                .collect_view()
                                .into_any()
                        })
                    }}
                </div>
            </div>

            <div class="card section-gap">
                <div class="card-head"><Icon icon=Ic::Tag class="ic-sm"/><h3>"Attribute definitions"</h3></div>
                <div class="table-wrap">
                    <table class="tbl">
                        <thead><tr><th>"Name"</th><th>"Applies to"</th><th>"Type"</th><th>"Default"</th></tr></thead>
                        <tbody>
                            {move || {
                                app.doc.with(|d| {
                                    d.attr_defs
                                        .iter()
                                        .map(|a| {
                                            let range = match &a.kind {
                                                AttrKind::Int { min, max } | AttrKind::Hex { min, max } => format!(" [{min}…{max}]"),
                                                AttrKind::Float { min, max } => format!(" [{}…{}]", num(*min), num(*max)),
                                                AttrKind::Enum(v) => format!(" {{{}}}", v.join(", ")),
                                                AttrKind::String => String::new(),
                                            };
                                            let def = d.attr_default(&a.name).map(|v| match (&a.kind, v) {
                                                (AttrKind::Enum(o), AttrValue::Int(i)) => o.get(*i as usize).cloned().unwrap_or_default(),
                                                _ => v.display(),
                                            });
                                            view! {
                                                <tr>
                                                    <td class="mono">{a.name.clone()}</td>
                                                    <td>{a.object.label()}</td>
                                                    <td class="mono hint">{format!("{}{range}", a.kind.label())}</td>
                                                    <td class="mono">{def.unwrap_or_default()}</td>
                                                </tr>
                                            }
                                        })
                                        .collect_view()
                                })
                            }}
                        </tbody>
                    </table>
                </div>
            </div>
        </div>
    }
}

// ───────────────────────────── Message ─────────────────────────────

fn parse_mux(s: &str) -> Option<Mux> {
    let s = s.trim();
    if s.is_empty() || s == "-" || s == "—" {
        return Some(Mux::None);
    }
    if s == "M" {
        return Some(Mux::Multiplexor);
    }
    let rest = s.strip_prefix('m')?;
    if let Some(v) = rest.strip_suffix('M') {
        return v.parse().ok().map(Mux::MultiplexedMultiplexor);
    }
    rest.parse().ok().map(Mux::Multiplexed)
}

#[component]
fn MessagePage(id: u32) -> impl IntoView {
    let app = use_app();
    let msg = Memo::new(move |_| app.doc.with(|d| d.message(id).cloned()));
    let exists = Memo::new(move |_| msg.with(|m| m.is_some()));
    let nodes = Memo::new(move |_| {
        app.doc
            .with(|d| d.nodes.iter().map(|n| n.name.clone()).collect::<Vec<_>>())
    });
    let groups = Memo::new(move |_| {
        app.doc.with(|d| {
            let mut g: Vec<String> = d
                .messages
                .iter()
                .filter_map(|m| {
                    m.attribute(GROUP_ATTR)
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                })
                .filter(|s| !s.is_empty())
                .collect();
            g.sort();
            g.dedup();
            g
        })
    });

    let set_name = move |v: String| {
        let v = v.trim().to_string();
        if v.is_empty() {
            return;
        }
        app.edit(&format!("rename to {v}"), |d| {
            if let Some(m) = d.message_mut(id) {
                m.name = v.clone();
            }
        });
    };
    let set_id = move |text: String, ext: Option<bool>| {
        let Some(m) = msg.get_untracked() else { return };
        let Some(new_can) = (if text.is_empty() {
            Some(m.can_id())
        } else {
            parse_id(&text)
        }) else {
            app.toast(format!("“{text}” is not a valid id"), ToastKind::Error);
            return;
        };
        let ext = ext.unwrap_or(m.is_extended() || new_can > 0x7FF);
        if (!ext && new_can > 0x7FF) || new_can > 0x1FFF_FFFF {
            app.toast(
                "Id out of range for this frame format".into(),
                ToastKind::Error,
            );
            return;
        }
        let new_raw = if ext {
            new_can | candy_dbc::EXTENDED_FLAG
        } else {
            new_can
        };
        if new_raw == id {
            return;
        }
        if app.doc.with_untracked(|d| d.message(new_raw).is_some()) {
            app.toast(
                format!("Id 0x{new_can:X} is already used"),
                ToastKind::Error,
            );
            return;
        }
        let was_signal = app.sel.get_untracked();
        app.edit("change id", |d| d.change_message_id(id, new_raw));
        app.sel.set(match was_signal {
            Sel::Signal(_, s) => Sel::Signal(new_raw, s),
            _ => Sel::Message(new_raw),
        });
        app.expanded.update(|e| {
            e.insert(new_raw);
        });
    };

    view! {
        <Show when=move || exists.get() fallback=|| view! { <div class="page empty-state">"This message no longer exists."</div> }>
            <div class="page">
                <div class="page-head">
                    <div class="page-icon"><Icon icon=Ic::Message/></div>
                    <div class="grow" style="min-width:0">
                        <TextIn class="page-title-input mono" value=Signal::derive(move || msg.with(|m| m.as_ref().map(|m| m.name.clone()).unwrap_or_default())) on_commit=set_name/>
                        <div class="page-sub">
                            {move || {
                                let m = msg.get()?;
                                let j = m.j1939();
                                let util = layout::utilisation(&m);
                                Some(view! {
                                    <span class="badge badge-accent mono">{m.id_hex()}</span>
                                    <span class="mono">{m.can_id().to_string()}</span>
                                    <span>{if m.is_extended() { "Extended (29-bit)" } else { "Standard (11-bit)" }}</span>
                                    {(m.size > 8).then(|| view! { <span class="badge badge-info">"CAN FD"</span> })}
                                    {j.map(|j| view! { <span class="mono hint">{format!("PGN {} · SA 0x{:02X} · P{}", j.pgn, j.source, j.priority)}</span> })}
                                    <span class="hint">{format!("{} · {:.0}% of payload used", plural(m.signals.len(), "signal"), util * 100.0)}</span>
                                })
                            }}
                        </div>
                    </div>
                    <div class="page-actions">
                        <button class="btn btn-sm" title="Duplicate (Ctrl+D)" on:click=move |_| {
                            app.select(Sel::Message(id));
                            actions::duplicate_selection(app)
                        }><Icon icon=Ic::Copy class="ic-sm"/>"Duplicate"</button>
                        <button class="btn btn-sm btn-danger" on:click=move |_| {
                            app.select(Sel::Message(id));
                            actions::delete_selection(app)
                        }><Icon icon=Ic::Trash class="ic-sm"/>"Delete"</button>
                        <button class="btn btn-sm btn-primary" title="Add signal (N)" on:click=move |_| actions::add_signal(app, Some(id), None)>
                            <Icon icon=Ic::Plus class="ic-sm"/>"Signal"
                        </button>
                    </div>
                </div>

                <div class="card card-pad">
                    <div class="fields">
                        <Field label="Identifier" hint="Hex (0x…) or decimal">
                            <TextIn class="input mono" value=Signal::derive(move || msg.with(|m| m.as_ref().map(|m| m.id_hex()).unwrap_or_default())) on_commit=move |v| set_id(v, None)/>
                        </Field>
                        <Field label="Frame format">
                            <select class="select" on:change=move |ev| set_id(String::new(), Some(event_target_value(&ev) == "ext"))>
                                <option value="std" selected=move || msg.with(|m| m.as_ref().is_some_and(|m| !m.is_extended()))>"Standard 11-bit"</option>
                                <option value="ext" selected=move || msg.with(|m| m.as_ref().is_some_and(|m| m.is_extended()))>"Extended 29-bit"</option>
                            </select>
                        </Field>
                        <Field label="Length">
                            <select class="select" on:change=move |ev| {
                                if let Ok(size) = event_target_value(&ev).parse::<u32>() {
                                    app.edit("set length", |d| if let Some(m) = d.message_mut(id) { m.size = size });
                                }
                            }>
                                {FD_SIZES
                                    .iter()
                                    .map(|s| {
                                        let s = *s;
                                        view! {
                                            <option value=s.to_string() selected=move || msg.with(|m| m.as_ref().is_some_and(|m| m.size == s))>
                                                {if s > 8 { format!("{s} bytes (FD)") } else { format!("{s} bytes") }}
                                            </option>
                                        }
                                    })
                                    .collect_view()}
                            </select>
                        </Field>
                        <Field label="Transmitter">
                            <select class="select" on:change=move |ev| {
                                let v = event_target_value(&ev);
                                app.edit("set transmitter", |d| if let Some(m) = d.message_mut(id) { m.transmitter = v });
                            }>
                                <option value=candy_dbc::NO_NODE selected=move || msg.with(|m| m.as_ref().is_some_and(|m| m.transmitter == candy_dbc::NO_NODE))>"— none —"</option>
                                {move || {
                                    let cur = msg.with(|m| m.as_ref().map(|m| m.transmitter.clone()).unwrap_or_default());
                                    nodes.get().into_iter().map(|n| {
                                        let sel = n == cur;
                                        view! { <option value=n.clone() selected=sel>{n.clone()}</option> }
                                    }).collect_view()
                                }}
                            </select>
                        </Field>
                        <Field label="Cycle time (ms)" hint="GenMsgCycleTime">
                            <NumIn integer=true value=Signal::derive(move || msg.with(|m| m.as_ref().and_then(|m| app.doc.with_untracked(|d| d.cycle_time_ms(m))).unwrap_or(0.0))) on_commit=move |v| {
                                app.edit("set cycle time", |d| {
                                    actions::ensure_msg_attr(d, "GenMsgCycleTime", AttrKind::Int { min: 0, max: 65535 });
                                    if let Some(m) = d.message_mut(id) {
                                        set_attribute(&mut m.attributes, "GenMsgCycleTime", (v > 0.0).then_some(AttrValue::Int(v as i64)));
                                    }
                                })
                            }/>
                        </Field>
                        <Field label="Folder" hint="Use / to nest">
                            <TextIn list="candy-groups" placeholder="e.g. Chassis/Brakes" value=Signal::derive(move || msg.with(|m| m.as_ref().and_then(|m| m.attribute(GROUP_ATTR)).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default())) on_commit=move |v| {
                                let v = v.trim().trim_matches('/').to_string();
                                app.edit("set folder", |d| {
                                    actions::ensure_msg_attr(d, GROUP_ATTR, AttrKind::String);
                                    if let Some(m) = d.message_mut(id) {
                                        set_attribute(&mut m.attributes, GROUP_ATTR, (!v.is_empty()).then(|| AttrValue::Str(v.clone())));
                                    }
                                })
                            }/>
                            <datalist id="candy-groups">
                                {move || groups.get().into_iter().map(|g| view! { <option value=g></option> }).collect_view()}
                            </datalist>
                        </Field>
                    </div>
                    <div style="margin-top:10px">
                        <Field label="Comment">
                            <textarea
                                class="textarea"
                                rows="2"
                                placeholder="What is this message for?"
                                prop:value=move || msg.with(|m| m.as_ref().and_then(|m| m.comment.clone()).unwrap_or_default())
                                on:change=move |ev| {
                                    let v = event_target_value(&ev);
                                    app.edit("edit comment", |d| if let Some(m) = d.message_mut(id) { m.comment = (!v.trim().is_empty()).then_some(v) });
                                }
                            ></textarea>
                        </Field>
                    </div>
                </div>

                <div class="card card-pad section-gap bitgrid-wrap">
                    <BitGrid msg_id=id/>
                </div>

                <SignalTable id=id/>
            </div>
        </Show>
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SortKey {
    Order,
    Name,
    Start,
}

#[component]
fn SignalTable(id: u32) -> impl IntoView {
    let app = use_app();
    let sort = RwSignal::new(SortKey::Order);
    let keys = Memo::new(move |_| {
        app.doc.with(|d| {
            let Some(m) = d.message(id) else {
                return Vec::new();
            };
            let mut v: Vec<(usize, String, u32)> = m
                .signals
                .iter()
                .enumerate()
                .map(|(i, s)| (i, s.name.clone(), s.start_bit))
                .collect();
            match sort.get() {
                SortKey::Order => {}
                SortKey::Name => v.sort_by_key(|a| a.1.to_lowercase()),
                SortKey::Start => v.sort_by_key(|x| x.2),
            }
            v.into_iter().map(|(i, n, _)| (i, n)).collect::<Vec<_>>()
        })
    });
    let th = move |label: &'static str, key: SortKey| {
        view! {
            <th class="sortable" on:click=move |_| sort.update(|s| *s = if *s == key { SortKey::Order } else { key })>
                {label}{move || (sort.get() == key).then_some(" ↓")}
            </th>
        }
    };
    view! {
        <div class="card section-gap">
            <div class="card-head">
                <Icon icon=Ic::Signal class="ic-sm"/>
                <h3>"Signals"</h3>
                <span class="hint">"Edit inline · click a row to inspect"</span>
                <button class="icon-btn" title="Add signal" on:click=move |_| actions::add_signal(app, Some(id), None)><Icon icon=Ic::Plus class="ic-sm"/></button>
            </div>
            <div class="table-wrap">
                <table class="tbl">
                    <thead>
                        <tr>
                            <th style="width:14px"></th>
                            {th("Name", SortKey::Name)}
                            <th title="Multiplexing: M = switch, m3 = present when switch is 3">"Mux"</th>
                            {th("Start", SortKey::Start)}
                            <th>"Size"</th>
                            <th>"Order"</th>
                            <th>"Type"</th>
                            <th>"Factor"</th>
                            <th>"Offset"</th>
                            <th>"Min"</th>
                            <th>"Max"</th>
                            <th>"Unit"</th>
                            <th>"Receivers"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <For each=move || keys.get() key=|k| k.clone() let:k>
                            <SignalRow id=id index=k.0 name=k.1/>
                        </For>
                    </tbody>
                </table>
                <Show when=move || keys.with(|k| k.is_empty())>
                    <div class="empty-state">
                        "No signals yet. Drag across empty bits above, or "
                        <button class="btn btn-sm" on:click=move |_| actions::add_signal(app, Some(id), None)>"add one"</button>
                    </div>
                </Show>
            </div>
        </div>
    }
}

#[component]
fn SignalRow(id: u32, index: usize, name: String) -> impl IntoView {
    let app = use_app();
    let sig = Memo::new({
        let name = name.clone();
        move |_| {
            app.doc
                .with(|d| d.message(id).and_then(|m| m.signal(&name)).cloned())
        }
    });
    let get = move |f: fn(&candy_dbc::Signal) -> f64| {
        Signal::derive(move || sig.with(|s| s.as_ref().map(f).unwrap_or(0.0)))
    };
    let edit = {
        let name = name.clone();
        move |field: &'static str, f: Box<dyn FnOnce(&mut candy_dbc::Signal)>| {
            app.edit_coalesce(
                &format!("{id}:{name}:{field}"),
                &format!("edit {field}"),
                |d| {
                    if let Some(s) = d.message_mut(id).and_then(|m| m.signal_mut(&name)) {
                        f(s);
                    }
                },
            );
        }
    };
    let selected = {
        let name = name.clone();
        Memo::new(move |_| matches!(app.sel.get(), Sel::Signal(i, ref n) if i == id && *n == name))
    };
    let row_id = format!("sigrow-{id}-{name}");
    let card = Card::Signal(id, name.clone());
    let n_click = name.clone();
    let n_rename = name.clone();

    let e1 = edit.clone();
    let e2 = edit.clone();
    let e3 = edit.clone();
    let e4 = edit.clone();
    let e5 = edit.clone();
    let e6 = edit.clone();
    let e7 = edit.clone();
    let e8 = edit.clone();
    let e9 = edit.clone();
    let e10 = edit.clone();

    view! {
        <tr id=row_id class:sel=move || selected.get() on:click=move |_| {
            if !matches!(app.sel.get_untracked(), Sel::Signal(i, ref n) if i == id && *n == n_click) {
                app.select(Sel::Signal(id, n_click.clone()));
            }
        }>
            <td><span class="dot" style=hue_style(index) on:mouseenter=hover_in(card) on:mouseleave=hover_out()></span></td>
            <td>
                <TextIn class="cell mono w-name" value=Signal::derive({ let n = name.clone(); move || n.clone() }) on_commit=move |v| {
                    let v = v.trim().to_string();
                    if v.is_empty() || v == n_rename {
                        return;
                    }
                    if app.doc.with_untracked(|d| d.message(id).is_some_and(|m| m.signal(&v).is_some())) {
                        app.toast(format!("{v} already exists in this message"), ToastKind::Error);
                        return;
                    }
                    let old = n_rename.clone();
                    app.edit(&format!("rename {old}"), |d| d.rename_signal(id, &old, &v));
                    app.select(Sel::Signal(id, v));
                }/>
            </td>
            <td>
                <TextIn class="cell mono w-sm" placeholder="—" value=Signal::derive(move || sig.with(|s| s.as_ref().map(|s| s.mux.token()).unwrap_or_default())) on_commit=move |v| {
                    match parse_mux(&v) {
                        Some(mx) => e1("mux", Box::new(move |s| s.mux = mx)),
                        None => app.toast("Use M, m<value> or m<value>M".into(), ToastKind::Error),
                    }
                }/>
            </td>
            <td><NumIn class="cell mono w-sm" integer=true value=get(|s| s.start_bit as f64) on_commit=move |v| e2("start bit", Box::new(move |s| s.start_bit = v.max(0.0) as u32))/></td>
            <td><NumIn class="cell mono w-sm" integer=true value=get(|s| s.size as f64) on_commit=move |v| e3("size", Box::new(move |s| s.size = v.clamp(1.0, 64.0) as u32))/></td>
            <td>
                <select class="cell" on:change=move |ev| {
                    let big = event_target_value(&ev) == "be";
                    e4("byte order", Box::new(move |s| crate::components::inspector::set_byte_order(s, big)));
                }>
                    <option value="le" selected=move || sig.with(|s| s.as_ref().is_some_and(|s| s.byte_order == ByteOrder::LittleEndian))>"Intel"</option>
                    <option value="be" selected=move || sig.with(|s| s.as_ref().is_some_and(|s| s.byte_order == ByteOrder::BigEndian))>"Motorola"</option>
                </select>
            </td>
            <td>
                <select class="cell" on:change=move |ev| {
                    let vt = match event_target_value(&ev).as_str() {
                        "s" => ValueType::Signed,
                        "f" => ValueType::Float32,
                        "d" => ValueType::Float64,
                        _ => ValueType::Unsigned,
                    };
                    e5("type", Box::new(move |s| {
                        s.value_type = vt;
                        match vt {
                            ValueType::Float32 => s.size = 32,
                            ValueType::Float64 => s.size = 64,
                            _ => {}
                        }
                    }));
                }>
                    {[("u", "unsigned", ValueType::Unsigned), ("s", "signed", ValueType::Signed), ("f", "float32", ValueType::Float32), ("d", "float64", ValueType::Float64)]
                        .into_iter()
                        .map(|(v, l, t)| view! { <option value=v selected=move || sig.with(|s| s.as_ref().is_some_and(|s| s.value_type == t))>{l}</option> })
                        .collect_view()}
                </select>
            </td>
            <td><NumIn class="cell mono w-num" value=get(|s| s.factor) on_commit=move |v| e6("factor", Box::new(move |s| s.factor = v))/></td>
            <td><NumIn class="cell mono w-num" value=get(|s| s.offset) on_commit=move |v| e7("offset", Box::new(move |s| s.offset = v))/></td>
            <td><NumIn class="cell mono w-num" value=get(|s| s.min) on_commit=move |v| e8("min", Box::new(move |s| s.min = v))/></td>
            <td><NumIn class="cell mono w-num" value=get(|s| s.max) on_commit=move |v| e9("max", Box::new(move |s| s.max = v))/></td>
            <td>
                <TextIn class="cell w-sm" placeholder="—" value=Signal::derive(move || sig.with(|s| s.as_ref().map(|s| s.unit.clone()).unwrap_or_default())) on_commit=move |v| e10("unit", Box::new(move |s| s.unit = v))/>
            </td>
            <td class="hint" style="max-width:160px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">
                {move || sig.with(|s| s.as_ref().map(|s| s.receivers.iter().filter(|r| *r != candy_dbc::NO_NODE).cloned().collect::<Vec<_>>().join(", ")).unwrap_or_default())}
            </td>
        </tr>
    }
}

// ───────────────────────────── Node ─────────────────────────────

#[component]
fn NodePage(name: String) -> impl IntoView {
    let app = use_app();
    let n_tx = name.clone();
    let tx = Memo::new(move |_| {
        app.doc.with(|d| {
            d.messages
                .iter()
                .filter(|m| m.transmitter == n_tx || m.extra_transmitters.contains(&n_tx))
                .map(|m| (m.id, m.name.clone(), fmt_id(m, true), m.signals.len()))
                .collect::<Vec<_>>()
        })
    });
    let n_rx = name.clone();
    let rx = Memo::new(move |_| {
        app.doc.with(|d| {
            d.messages
                .iter()
                .flat_map(|m| {
                    m.signals
                        .iter()
                        .filter(|s| s.receivers.contains(&n_rx))
                        .map(move |s| (m.id, m.name.clone(), s.name.clone()))
                })
                .collect::<Vec<_>>()
        })
    });
    let n_rename = name.clone();
    view! {
        <div class="page">
            <div class="page-head">
                <div class="page-icon"><Icon icon=Ic::Node/></div>
                <div class="grow">
                    <TextIn class="page-title-input mono" value=Signal::derive({ let n = name.clone(); move || n.clone() }) on_commit=move |v| {
                        let v = v.trim().to_string();
                        if v.is_empty() || v == n_rename { return; }
                        if app.doc.with_untracked(|d| d.node(&v).is_some()) {
                            app.toast(format!("Node {v} already exists"), ToastKind::Error);
                            return;
                        }
                        let old = n_rename.clone();
                        app.edit(&format!("rename node {old}"), |d| d.rename_node(&old, &v));
                        app.select(Sel::Node(v));
                    }/>
                    <div class="page-sub">{move || format!("Sends {} · receives {}", plural(tx.with(|t| t.len()), "message"), plural(rx.with(|r| r.len()), "signal"))}</div>
                </div>
                <div class="page-actions">
                    <button class="btn btn-sm btn-danger" on:click=move |_| actions::delete_selection(app)><Icon icon=Ic::Trash class="ic-sm"/>"Delete"</button>
                </div>
            </div>
            <div class="grid2">
                <div class="card">
                    <div class="card-head"><Icon icon=Ic::Arrow class="ic-sm"/><h3>"Transmits"</h3></div>
                    {move || tx.get().into_iter().map(|(id, n, hex, count)| view! {
                        <div class="list-row" on:click=move |_| app.select(Sel::Message(id)) on:mouseenter=hover_in(Card::Message(id)) on:mouseleave=hover_out()>
                            <span class="mono hint" style="width:84px">{hex}</span>
                            <span class="grow mono">{n}</span>
                            <span class="hint">{plural(count, "signal")}</span>
                        </div>
                    }).collect_view()}
                </div>
                <div class="card">
                    <div class="card-head"><Icon icon=Ic::Signal class="ic-sm"/><h3>"Receives"</h3></div>
                    {move || rx.get().into_iter().take(200).map(|(id, m, s)| {
                        let s2 = s.clone();
                        view! {
                            <div class="list-row" on:click=move |_| app.select(Sel::Signal(id, s2.clone()))>
                                <span class="grow mono">{s}</span>
                                <span class="hint mono">{m}</span>
                            </div>
                        }
                    }).collect_view()}
                </div>
            </div>
        </div>
    }
}

// ───────────────────────────── Value table ─────────────────────────────

#[component]
fn ValueTablePage(name: String) -> impl IntoView {
    let app = use_app();
    let n_get = name.clone();
    let entries = Memo::new(move |_| {
        app.doc.with(|d| {
            d.value_tables
                .iter()
                .find(|t| t.name == n_get)
                .map(|t| t.entries.clone())
                .unwrap_or_default()
        })
    });
    let n_edit = name.clone();
    let edit = move |f: EntriesEdit| {
        let n = n_edit.clone();
        app.edit("edit value table", move |d| {
            if let Some(t) = d.value_tables.iter_mut().find(|t| t.name == n) {
                f(&mut t.entries);
            }
        });
    };
    let n_rename = name.clone();
    let e_add = edit.clone();
    view! {
        <div class="page" style="max-width:720px">
            <div class="page-head">
                <div class="page-icon"><Icon icon=Ic::Table/></div>
                <div class="grow">
                    <TextIn class="page-title-input mono" value=Signal::derive({ let n = name.clone(); move || n.clone() }) on_commit=move |v| {
                        let v = v.trim().to_string();
                        if v.is_empty() || v == n_rename { return; }
                        let old = n_rename.clone();
                        app.edit("rename value table", |d| if let Some(t) = d.value_tables.iter_mut().find(|t| t.name == old) { t.name = v.clone() });
                        app.select(Sel::ValueTable(v));
                    }/>
                    <div class="page-sub">"Shared value table (VAL_TABLE_) · copy it onto signals from the inspector"</div>
                </div>
                <div class="page-actions">
                    <button class="btn btn-sm btn-danger" on:click=move |_| actions::delete_selection(app)><Icon icon=Ic::Trash class="ic-sm"/>"Delete"</button>
                </div>
            </div>
            <div class="card card-pad">
                <ValueEntries entries=Signal::derive(move || entries.get()) on_change=move |f| edit(f)/>
                <button class="btn btn-sm" style="margin-top:10px" on:click=move |_| {
                    e_add(Box::new(|e| {
                        let next = e.iter().map(|x| x.value + 1).max().unwrap_or(0);
                        e.push(ValueEntry::new(next, format!("Value{next}")));
                    }))
                }><Icon icon=Ic::Plus class="ic-sm"/>"Add value"</button>
            </div>
        </div>
    }
}

/// Editable list of value → label rows, shared by value tables and signal value descriptions.
#[component]
pub fn ValueEntries(
    #[prop(into)] entries: Signal<Vec<ValueEntry>>,
    on_change: impl Fn(EntriesEdit) + Clone + Send + Sync + 'static,
) -> impl IntoView {
    move || {
        let list = entries.get();
        if list.is_empty() {
            return view! { <div class="hint">"No values yet."</div> }.into_any();
        }
        list.into_iter()
            .enumerate()
            .map(|(i, e)| {
                let (c1, c2, c3) = (on_change.clone(), on_change.clone(), on_change.clone());
                view! {
                    <div class="vt-row" style="margin-bottom:6px">
                        <NumIn integer=true value=Signal::derive(move || e.value as f64) on_commit=move |v| c1(Box::new(move |l| if let Some(x) = l.get_mut(i) { x.value = v as i64 }))/>
                        <TextIn value=Signal::derive({ let l = e.label.clone(); move || l.clone() }) on_commit=move |v| c2(Box::new(move |l| if let Some(x) = l.get_mut(i) { x.label = v }))/>
                        <button class="icon-btn" title="Remove" on:click=move |_| c3(Box::new(move |l| { if i < l.len() { l.remove(i); } }))>
                            <Icon icon=Ic::X class="ic-sm"/>
                        </button>
                    </div>
                }
            })
            .collect_view()
            .into_any()
    }
}
