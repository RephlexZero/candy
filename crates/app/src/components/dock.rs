//! Bottom dock: problems, decode playground, diff & merge, raw DBC.

use candy_dbc::decode::{self, Frame};
use candy_dbc::diff::{self, ChangeKind};
use candy_dbc::lint::{self, Severity, Target};
use leptos::prelude::*;

use crate::actions;
use crate::icons::{Ic, Icon};
use crate::persist;
use crate::state::{AppState, Card, DockTab, Sel, ToastKind, use_app};
use crate::ui::{hover_in, hover_out, hue_style, num, plural};

/// Read a second DBC and open the diff tab against it.
pub fn load_compare_file(app: AppState, file: web_sys::File) {
    leptos::task::spawn_local(async move {
        let name = file.name();
        match persist::read_file(&file).await {
            Some(text) => {
                let out = candy_dbc::parse(&text);
                app.diff_other.set(Some((name.clone(), out.dbc)));
                actions::open_dock(app, DockTab::Diff);
                app.toast(format!("Comparing with {name}"), ToastKind::Info);
            }
            None => app.toast(format!("Could not read {name}"), ToastKind::Error),
        }
    });
}

fn pick_compare_file(app: AppState) {
    use wasm_bindgen::JsCast;
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Ok(input) = doc.create_element("input") else {
        return;
    };
    let input: web_sys::HtmlInputElement = input.unchecked_into();
    input.set_type("file");
    input.set_accept(".dbc,text/plain");
    let inp = input.clone();
    let cb = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
        if let Some(file) = inp.files().and_then(|f| f.get(0)) {
            load_compare_file(app, file);
        }
    });
    input.set_onchange(Some(cb.as_ref().unchecked_ref()));
    cb.forget();
    input.click();
}

#[component]
pub fn Dock() -> impl IntoView {
    let app = use_app();
    let height = RwSignal::new(
        persist::get("candy.dock-h")
            .and_then(|h| h.parse::<f64>().ok())
            .unwrap_or(260.0),
    );
    let counts = Memo::new(move |_| app.issues_for(|_| true));

    let resizing = StoredValue::new(None::<(f64, f64)>);
    let _ = window_event_listener(leptos::ev::pointermove, move |ev| {
        if let Some((y0, h0)) = resizing.get_value() {
            let h = (h0 + y0 - ev.client_y() as f64).clamp(120.0, 720.0);
            height.set(h);
        }
    });
    let _ = window_event_listener(leptos::ev::pointerup, move |_| {
        if resizing.get_value().is_some() {
            resizing.set_value(None);
            persist::set("candy.dock-h", &format!("{:.0}", height.get_untracked()));
        }
    });

    let tab = move |t: DockTab, icon: Ic, label: &'static str| {
        view! {
            <button class="dock-tab" class:on=move || app.dock_open.get() && app.dock_tab.get() == t on:click=move |_| {
                if app.dock_open.get_untracked() && app.dock_tab.get_untracked() == t {
                    actions::toggle_dock(app);
                } else {
                    actions::open_dock(app, t);
                }
            }>
                <Icon icon=icon class="ic-sm"/>{label}
                {(t == DockTab::Problems).then(|| view! {
                    {move || {
                        let (e, w) = counts.get();
                        let errs = app.parse_errors.with(|p| p.len());
                        (e + w + errs > 0).then(|| view! { <span class="badge" class:badge-err={e + errs > 0} class:badge-warn={e + errs == 0}>{e + w + errs}</span> })
                    }}
                })}
            </button>
        }
    };

    view! {
        <section class="dock" class:open=move || app.dock_open.get() style=move || format!("--dock-h:{}px", height.get())>
            <Show when=move || app.dock_open.get()>
                <div class="dock-resize" on:pointerdown=move |ev| {
                    ev.prevent_default();
                    resizing.set_value(Some((ev.client_y() as f64, height.get_untracked())));
                }></div>
            </Show>
            <div class="dock-tabs">
                {tab(DockTab::Problems, Ic::Warning, "Problems")}
                {tab(DockTab::Decode, Ic::Zap, "Decode")}
                {tab(DockTab::Diff, Ic::Diff, "Diff & merge")}
                {tab(DockTab::Raw, Ic::Code, "DBC source")}
                <div class="tb-spacer"></div>
                <button class="icon-btn" title="Toggle panel (Ctrl+J)" on:click=move |_| actions::toggle_dock(app)>
                    <Icon icon=if_open(app)/>
                </button>
            </div>
            <Show when=move || app.dock_open.get()>
                <div class="dock-body">
                    {move || match app.dock_tab.get() {
                        DockTab::Problems => view! { <Problems/> }.into_any(),
                        DockTab::Decode => view! { <Decode/> }.into_any(),
                        DockTab::Diff => view! { <DiffView/> }.into_any(),
                        DockTab::Raw => view! { <Raw/> }.into_any(),
                    }}
                </div>
            </Show>
        </section>
    }
}

fn if_open(app: AppState) -> Ic {
    // The icon is static per render; the chevron flips via CSS rotation would need state, so use Panel.
    let _ = app;
    Ic::Panel
}

// ───────────────────────────── Problems ─────────────────────────────

fn target_label(app: AppState, t: &Target) -> String {
    app.doc.with_untracked(|d| {
        let msg = |id: &u32| {
            d.message(*id)
                .map(|m| m.name.clone())
                .unwrap_or_else(|| format!("0x{:X}", id & 0x1FFF_FFFF))
        };
        match t {
            Target::Network => "Network".into(),
            Target::Node(n) => n.clone(),
            Target::Message(id) => msg(id),
            Target::Signal(id, s) => format!("{}.{s}", msg(id)),
            Target::ValueLabel(id, s, v) => format!("{}.{s} = {v}", msg(id)),
            Target::ValueTable(v) => v.clone(),
            Target::EnvVar(v) => v.clone(),
            Target::SignalGroup(id, g) => format!("{} / {g}", msg(id)),
        }
    })
}

#[component]
fn Problems() -> impl IntoView {
    let app = use_app();
    let filter = RwSignal::new(None::<Severity>);
    let fixable = Memo::new(move |_| {
        app.issues
            .with(|i| i.iter().filter(|i| i.fix.is_some()).count())
    });
    view! {
        <div class="dock-toolbar">
            <div class="seg">
                <button class:on=move || filter.get().is_none() on:click=move |_| filter.set(None)>"All"</button>
                <button class:on=move || filter.get() == Some(Severity::Error) on:click=move |_| filter.set(Some(Severity::Error))>"Errors"</button>
                <button class:on=move || filter.get() == Some(Severity::Warning) on:click=move |_| filter.set(Some(Severity::Warning))>"Warnings"</button>
                <button class:on=move || filter.get() == Some(Severity::Info) on:click=move |_| filter.set(Some(Severity::Info))>"Info"</button>
            </div>
            <div class="tb-spacer"></div>
            <Show when={move || fixable.get() > 0}>
                <button class="btn btn-sm btn-primary" on:click=move |_| actions::fix_all(app, None)>
                    <Icon icon=Ic::Wand class="ic-sm"/>{move || format!("Fix all ({})", fixable.get())}
                </button>
            </Show>
        </div>
        {move || {
            let errs = app.parse_errors.get();
            errs.into_iter().map(|e| view! {
                <div class="problem" on:click=move |_| actions::open_dock(app, DockTab::Raw)>
                    <span class="err"><Icon icon=Ic::Error class="ic-sm"/></span>
                    <span class="grow">{e.message.clone()}</span>
                    <span class="rule">{format!("parse · line {}:{}", e.line, e.col)}</span>
                </div>
            }).collect_view()
        }}
        {move || {
            let f = filter.get();
            let list: Vec<_> = app.issues.with(|i| i.iter().filter(|i| f.is_none_or(|f| i.severity == f)).cloned().collect());
            if list.is_empty() && app.parse_errors.with(|p| p.is_empty()) {
                return view! { <div class="empty-state"><Icon icon=Ic::Check class="ic-lg"/><div>"No problems. Sweet!"</div></div> }.into_any();
            }
            list.into_iter().take(500).map(|i| {
                let (icon, cls) = match i.severity {
                    Severity::Error => (Ic::Error, "err"),
                    Severity::Warning => (Ic::Warning, "warn"),
                    Severity::Info => (Ic::Info, "info"),
                };
                let target = i.target.clone();
                let label = target_label(app, &target);
                let fix = i.fix.clone();
                let rule = i.rule;
                view! {
                    <div class="problem" on:click=move |_| {
                        let sel = Sel::from_target(&target);
                        app.select(sel.clone());
                        if let Sel::Signal(id, n) = sel {
                            persist::scroll_into_view(&format!("sigrow-{id}-{n}"));
                        }
                    }>
                        <span class=cls><Icon icon=icon class="ic-sm"/></span>
                        <span class="mono" style="font-weight:600;flex:none;max-width:40%;overflow:hidden;text-overflow:ellipsis">{label}</span>
                        <span class="grow">{i.message.clone()}</span>
                        <span class="rule" on:mouseenter=hover_in(Card::Rule(rule)) on:mouseleave=hover_out()>{rule}</span>
                        {fix.map(|f| {
                            let title = f.label();
                            let text = title.clone();
                            view! {
                                <button class="btn btn-sm btn-ghost" title=title on:click=move |ev| {
                                    ev.stop_propagation();
                                    let f = f.clone();
                                    app.edit(&f.label().to_lowercase(), |d| { lint::apply_fix(d, &f); });
                                    app.fix_selection();
                                }><Icon icon=Ic::Wand class="ic-sm"/>{text}</button>
                            }
                        })}
                    </div>
                }
            }).collect_view().into_any()
        }}
    }
}

// ───────────────────────────── Decode ─────────────────────────────

/// Sample log lines generated from the current DBC (mid-range values), so the playground works
/// out of the box.
fn sample_log(d: &candy_dbc::Dbc) -> String {
    let mut out = String::new();
    let mut t = 0.0;
    for m in d.messages.iter().filter(|m| !m.is_pseudo()).take(12) {
        let branches = candy_dbc::layout::mux_values(m);
        let picks: Vec<Option<u64>> = if branches.is_empty() {
            vec![None]
        } else {
            branches.iter().take(3).map(|b| Some(*b)).collect()
        };
        for b in picks {
            let mut data = vec![0u8; m.size as usize];
            for (k, s) in m.signals.iter().enumerate() {
                if !candy_dbc::layout::in_branch(m, s, b) {
                    continue;
                }
                if s.mux.is_switch() && matches!(s.mux, candy_dbc::Mux::Multiplexor) {
                    if let Some(b) = b {
                        decode::insert_raw(s, &mut data, b);
                    }
                    continue;
                }
                let (lo, hi) = if s.min < s.max {
                    (s.min, s.max)
                } else {
                    s.physical_range()
                };
                let frac = 0.25 + 0.5 * (((k * 37 + t as usize) % 100) as f64 / 100.0);
                decode::encode_physical(s, &mut data, lo + (hi - lo) * frac);
            }
            t += 0.01;
            let hex: Vec<String> = data.iter().map(|b| format!("{b:02X}")).collect();
            let id = if m.is_extended() {
                format!("{:08X}", m.can_id())
            } else {
                format!("{:03X}", m.can_id())
            };
            out.push_str(&format!("({t:.6}) can0 {id}#{}\n", hex.join("")));
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
enum DecodeMode {
    Log,
    Compose,
}

#[component]
fn Decode() -> impl IntoView {
    let app = use_app();
    let mode = RwSignal::new(DecodeMode::Log);
    let frames = Memo::new(move |_| decode::parse_log(&app.decode_input.get()));
    view! {
        <div class="split">
            <div style="display:flex;flex-direction:column">
                <div class="dock-toolbar">
                    <div class="seg">
                        <button class:on=move || mode.get() == DecodeMode::Log on:click=move |_| mode.set(DecodeMode::Log)>"Decode log"</button>
                        <button class:on=move || mode.get() == DecodeMode::Compose on:click=move |_| mode.set(DecodeMode::Compose)>"Compose frame"</button>
                    </div>
                    <div class="tb-spacer"></div>
                    <Show when=move || mode.get() == DecodeMode::Log>
                        <button class="btn btn-sm btn-ghost" title="Generate frames from this DBC" on:click=move |_| app.decode_input.set(app.doc.with_untracked(sample_log))>
                            <Icon icon=Ic::Sparkles class="ic-sm"/>"Sample"
                        </button>
                    </Show>
                </div>
                {move || match mode.get() {
                    DecodeMode::Log => view! {
                        <textarea
                            class="log-input grow"
                            spellcheck="false"
                            placeholder="Paste frames: candump (can0 123#DEADBEEF), Vector ASC, PCAN TRC, or plain “123 DE AD BE EF” lines."
                            prop:value=move || app.decode_input.get()
                            on:input=move |ev| app.decode_input.set(event_target_value(&ev))
                        ></textarea>
                    }.into_any(),
                    DecodeMode::Compose => view! { <Composer/> }.into_any(),
                }}
            </div>
            <div>
                {move || {
                    let fs = frames.get();
                    if fs.is_empty() {
                        return view! { <div class="empty-state">"Decoded frames appear here, with every signal’s physical value and label."</div> }.into_any();
                    }
                    let total = fs.len();
                    view! {
                        <div class="dock-toolbar"><span class="hint">{format!("{} · showing up to 300", plural(total, "frame"))}</span></div>
                        {fs.into_iter().take(300).map(|f| view! { <FrameView frame=f/> }).collect_view()}
                    }.into_any()
                }}
            </div>
        </div>
    }
}

#[component]
fn FrameView(frame: Frame) -> impl IntoView {
    let app = use_app();
    let found = app.doc.with_untracked(|d| {
        decode::find_message(d, &frame).map(|m| {
            (
                m.id,
                m.name.clone(),
                decode::decode_message(m, &frame.data),
                m.signals.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
            )
        })
    });
    let bytes = frame
        .data
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    let id = if frame.extended {
        format!("{:08X}", frame.id)
    } else {
        format!("{:03X}", frame.id)
    };
    view! {
        <div class="frame">
            <div class="frame-head">
                {frame.timestamp.map(|t| view! { <span class="hint mono">{format!("{t:.4}")}</span> })}
                <span class="badge badge-accent mono">{id}</span>
                {match &found {
                    Some((mid, name, _, _)) => {
                        let mid = *mid;
                        view! { <a class="mono" style="font-weight:650;cursor:pointer" on:click=move |_| app.select(Sel::Message(mid))>{name.clone()}</a> }.into_any()
                    }
                    None => view! { <span class="hint">"Unknown message"</span> }.into_any(),
                }}
                <span class="frame-bytes">{bytes}</span>
            </div>
            {found.map(|(mid, _, sigs, order)| view! {
                <div class="sigvals">
                    {sigs.into_iter().map(|s| {
                        let idx = order.iter().position(|n| *n == s.name).unwrap_or(0);
                        let value = s.label.clone().map(|l| format!("{l} ({})", num(s.raw_value))).unwrap_or_else(|| format!("{}{}", num(s.physical), if s.unit.is_empty() { String::new() } else { format!(" {}", s.unit) }));
                        let name = s.name.clone();
                        let title = format!("raw {} (0x{:X}) · bits {:?}", num(s.raw_value), s.raw, s.bits);
                        view! {
                            <span class="sigval" class:off=!s.active class:oor=!s.in_range style=hue_style(idx) title=title on:click=move |_| app.select(Sel::Signal(mid, name.clone()))>
                                <span>{s.name.clone()}</span><b>{value}</b>
                            </span>
                        }
                    }).collect_view()}
                </div>
            })}
        </div>
    }
}

/// Pick a message, type physical values, get the payload bytes.
#[component]
fn Composer() -> impl IntoView {
    let app = use_app();
    let msg_id = RwSignal::new(
        app.sel
            .get_untracked()
            .message_id()
            .or_else(|| app.doc.with_untracked(|d| d.messages.first().map(|m| m.id))),
    );
    let values = RwSignal::new(std::collections::HashMap::<String, f64>::new());
    let msg = Memo::new(move |_| {
        msg_id
            .get()
            .and_then(|id| app.doc.with(|d| d.message(id).cloned()))
    });
    let data = Memo::new(move |_| {
        let m = msg.get()?;
        let vals = values.get();
        let mut data = vec![0u8; m.size as usize];
        // Switches first so the active branch is known.
        let switch_val = m
            .multiplexor()
            .map(|s| vals.get(&s.name).copied().unwrap_or(0.0));
        let branch = m
            .multiplexor()
            .zip(switch_val)
            .map(|(s, v)| s.to_raw(v).round().max(0.0) as u64);
        for s in &m.signals {
            if !candy_dbc::layout::in_branch(&m, s, branch) {
                continue;
            }
            decode::encode_physical(
                s,
                &mut data,
                vals.get(&s.name).copied().unwrap_or(s.offset.max(s.min)),
            );
        }
        Some((data, branch))
    });
    view! {
        <div style="padding:12px 14px;overflow:auto">
            <select class="select" style="width:100%" on:change=move |ev| {
                msg_id.set(event_target_value(&ev).parse().ok());
                values.set(Default::default());
            }>
                {move || app.doc.with(|d| d.messages.iter().map(|m| {
                    let id = m.id;
                    view! { <option value=id.to_string() selected=move || msg_id.get() == Some(id)>{format!("{} · {}", m.id_hex(), m.name)}</option> }
                }).collect_view())}
            </select>
            {move || data.get().map(|(bytes, _)| {
                let hex = bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ");
                let line = msg.with_untracked(|m| m.as_ref().map(|m| {
                    let id = if m.is_extended() { format!("{:08X}", m.can_id()) } else { format!("{:03X}", m.can_id()) };
                    format!("can0 {id}#{}", bytes.iter().map(|b| format!("{b:02X}")).collect::<String>())
                })).unwrap_or_default();
                let l2 = line.clone();
                view! {
                    <div class="formula" style="margin:10px 0;display:flex;gap:8px;align-items:center">
                        <span class="grow">{hex}</span>
                        <button class="btn btn-sm btn-ghost" title="Copy as candump line" on:click=move |_| {
                            persist::copy_text(&l2);
                            app.toast("Copied candump line".into(), ToastKind::Success);
                        }><Icon icon=Ic::Copy class="ic-sm"/></button>
                        <button class="btn btn-sm btn-ghost" title="Append to the decode log" on:click=move |_| {
                            let l = line.clone();
                            app.decode_input.update(|t| { if !t.is_empty() && !t.ends_with('\n') { t.push('\n'); } t.push_str(&l); t.push('\n'); });
                        }><Icon icon=Ic::Plus class="ic-sm"/></button>
                    </div>
                }
            })}
            {move || {
                let m = msg.get()?;
                let branch = data.with(|d| d.as_ref().and_then(|x| x.1));
                Some(m.signals.iter().enumerate().filter(|(_, s)| candy_dbc::layout::in_branch(&m, s, branch)).map(|(i, s)| {
                    let name = s.name.clone();
                    let n2 = name.clone();
                    let default = s.offset.max(s.min);
                    let unit = s.unit.clone();
                    view! {
                        <div class="vt-row" style="grid-template-columns:1fr 120px 50px;margin-bottom:5px">
                            <span class="mono" style="overflow:hidden;text-overflow:ellipsis"><span class="dot" style=hue_style(i)></span>" "{name.clone()}</span>
                            <crate::ui::NumIn value=Signal::derive(move || values.with(|v| v.get(&n2).copied().unwrap_or(default))) on_commit=move |v| values.update(|m| { m.insert(name.clone(), v); })/>
                            <span class="hint">{unit}</span>
                        </div>
                    }
                }).collect_view())
            }}
        </div>
    }
}

// ───────────────────────────── Diff ─────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum DiffMode {
    SinceOpen,
    OtherFile,
}

#[component]
fn DiffView() -> impl IntoView {
    let app = use_app();
    let mode = RwSignal::new(if app.diff_other.with_untracked(|o| o.is_some()) {
        DiffMode::OtherFile
    } else {
        DiffMode::SinceOpen
    });
    let picked = RwSignal::new(std::collections::BTreeSet::<usize>::new());
    // For "other file": changes that turn the current document into the other one.
    let changes = Memo::new(move |_| match mode.get() {
        DiffMode::SinceOpen => app.baseline.with(|b| app.doc.with(|d| diff::diff(b, d))),
        DiffMode::OtherFile => app.diff_other.with(|o| {
            o.as_ref()
                .map(|(_, other)| app.doc.with(|d| diff::diff(d, other)))
                .unwrap_or_default()
        }),
    });
    Effect::new(move |_| {
        changes.track();
        picked.set(Default::default());
    });
    view! {
        <div class="dock-toolbar">
            <div class="seg">
                <button class:on=move || mode.get() == DiffMode::SinceOpen on:click=move |_| mode.set(DiffMode::SinceOpen)>"Changes since open"</button>
                <button class:on=move || mode.get() == DiffMode::OtherFile on:click=move |_| mode.set(DiffMode::OtherFile)>"Compare with file"</button>
            </div>
            <Show when=move || mode.get() == DiffMode::OtherFile>
                <button class="btn btn-sm" on:click=move |_| pick_compare_file(app)>
                    <Icon icon=Ic::Open class="ic-sm"/>{move || app.diff_other.with(|o| o.as_ref().map(|(n, _)| n.clone()).unwrap_or_else(|| "Choose file…".into()))}
                </button>
            </Show>
            <div class="tb-spacer"></div>
            <span class="hint">{move || plural(changes.with(|c| c.len()), "change")}</span>
            <Show when=move || !changes.with(|c| c.is_empty())>
                <button class="btn btn-sm btn-ghost" on:click=move |_| {
                    let n = changes.with_untracked(|c| c.len());
                    picked.update(|p| if p.len() == n { p.clear() } else { *p = (0..n).collect() });
                }>"Select all"</button>
                <button class="btn btn-sm btn-primary" prop:disabled=move || picked.with(|p| p.is_empty()) on:click=move |_| {
                    let sel: Vec<usize> = picked.get_untracked().into_iter().collect();
                    match mode.get_untracked() {
                        DiffMode::OtherFile => {
                            let Some((name, other)) = app.diff_other.get_untracked() else { return };
                            app.edit(&format!("merge from {name}"), |d| *d = diff::merge(d, &other, &sel));
                            app.toast(format!("Merged {}", plural(sel.len(), "change")), ToastKind::Success);
                        }
                        DiffMode::SinceOpen => {
                            // Revert: apply the matching inverse changes from the baseline.
                            let base = app.baseline.get_untracked();
                            let forward = changes.get_untracked();
                            app.edit("revert changes", |d| {
                                let inverse = diff::diff(d, &base);
                                for i in &sel {
                                    if let Some(c) = forward.get(*i)
                                        && let Some(inv) = inverse.iter().find(|x| x.item == c.item)
                                    {
                                        diff::apply(d, &base, inv);
                                    }
                                }
                            });
                            app.fix_selection();
                            app.toast(format!("Reverted {}", plural(sel.len(), "change")), ToastKind::Success);
                        }
                    }
                }>
                    <Icon icon=Ic::Check class="ic-sm"/>{move || if mode.get() == DiffMode::OtherFile { format!("Merge selected ({})", picked.with(|p| p.len())) } else { format!("Revert selected ({})", picked.with(|p| p.len())) }}
                </button>
            </Show>
        </div>
        {move || {
            let list = changes.get();
            if list.is_empty() {
                let msg = match mode.get() {
                    DiffMode::SinceOpen => "No changes since the file was opened.",
                    DiffMode::OtherFile if app.diff_other.with(|o| o.is_none()) => "Choose a second DBC (or Shift+drop one anywhere) to compare and cherry-pick changes.",
                    DiffMode::OtherFile => "The files are identical.",
                };
                return view! { <div class="empty-state">{msg}</div> }.into_any();
            }
            list.into_iter().enumerate().map(|(i, c)| {
                let (kind, cls) = match c.kind {
                    ChangeKind::Added => ("Added", "kind added"),
                    ChangeKind::Removed => ("Removed", "kind removed"),
                    ChangeKind::Modified => ("Changed", "kind modified"),
                };
                view! {
                    <label class="diff-row">
                        <input type="checkbox" class="check" prop:checked=move || picked.with(|p| p.contains(&i)) on:change=move |_| picked.update(|p| if !p.remove(&i) { p.insert(i); })/>
                        <span class=cls>{kind}</span>
                        <div class="grow">
                            <div class="mono" style="font-weight:600">{c.label.clone()}</div>
                            {(!c.details.is_empty()).then(|| view! {
                                <div class="diff-detail">
                                    {c.details.iter().map(|(f, old, new)| view! {
                                        <span class="hint">{f.clone()}</span>
                                        <span><span class="diff-old">{old.clone()}</span>" → "<span class="diff-new">{new.clone()}</span></span>
                                    }).collect_view()}
                                </div>
                            })}
                        </div>
                    </label>
                }
            }).collect_view().into_any()
        }}
    }
}

// ───────────────────────────── Raw ─────────────────────────────

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

const KEYWORDS: &[&str] = &[
    "VERSION",
    "NS_",
    "BS_",
    "BU_",
    "BO_",
    "SG_",
    "CM_",
    "BA_DEF_",
    "BA_DEF_DEF_",
    "BA_",
    "VAL_",
    "VAL_TABLE_",
    "SIG_VALTYPE_",
    "SIG_GROUP_",
    "SG_MUL_VAL_",
    "BO_TX_BU_",
    "EV_",
    "ENVVAR_DATA_",
    "BA_DEF_REL_",
    "BA_DEF_DEF_REL_",
    "BA_REL_",
    "BU_SG_REL_",
    "BU_BO_REL_",
    "BU_EV_REL_",
    "SGTYPE_",
    "SIG_TYPE_REF_",
    "CAT_DEF_",
    "CAT_",
    "FILTER",
    "NS_DESC_",
];

/// Minimal DBC syntax highlighter producing HTML, one `.ln` span per line.
fn highlight(src: &str) -> String {
    let mut out = String::with_capacity(src.len() * 2);
    let mut in_str = false;
    for line in src.lines() {
        out.push_str("<span class=\"ln\">");
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        if in_str {
            out.push_str("<span class=\"tok-str\">");
        }
        while i < chars.len() {
            let c = chars[i];
            if in_str {
                let start = i;
                while i < chars.len() && !(chars[i] == '"' && (i == 0 || chars[i - 1] != '\\')) {
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                    in_str = false;
                }
                out.push_str(&escape(&chars[start..i].iter().collect::<String>()));
                if !in_str {
                    out.push_str("</span>");
                }
                continue;
            }
            if c == '"' {
                in_str = true;
                out.push_str("<span class=\"tok-str\">\"");
                i += 1;
                continue;
            }
            if c.is_ascii_alphabetic() || c == '_' {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                if KEYWORDS.contains(&word.as_str()) {
                    out.push_str(&format!("<span class=\"tok-kw\">{word}</span>"));
                } else {
                    out.push_str(&word);
                }
                continue;
            }
            if c.is_ascii_digit()
                || (c == '-' && chars.get(i + 1).is_some_and(|n| n.is_ascii_digit()))
            {
                let start = i;
                i += 1;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric()
                        || chars[i] == '.'
                        || chars[i] == '+'
                        || chars[i] == '-')
                {
                    i += 1;
                }
                out.push_str(&format!(
                    "<span class=\"tok-num\">{}</span>",
                    escape(&chars[start..i].iter().collect::<String>())
                ));
                continue;
            }
            out.push_str(&escape(&c.to_string()));
            i += 1;
        }
        if in_str {
            out.push_str("</span>");
        }
        out.push_str("</span>");
    }
    out
}

#[component]
fn Raw() -> impl IntoView {
    let app = use_app();
    let editing = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let text = Memo::new(move |_| app.doc.with(candy_dbc::write));
    view! {
        <div class="dock-toolbar">
            <span class="hint">{move || format!("{} lines · {} KB", text.with(|t| t.lines().count()), text.with(|t| t.len()) / 1024)}</span>
            <div class="tb-spacer"></div>
            <Show when=move || !editing.get() fallback=move || view! {
                <button class="btn btn-sm btn-ghost" on:click=move |_| editing.set(false)>"Cancel"</button>
                <button class="btn btn-sm btn-primary" on:click=move |_| {
                    let out = candy_dbc::parse(&draft.get_untracked());
                    let n = out.errors.len();
                    app.edit("edit source", |d| *d = out.dbc);
                    app.parse_errors.set(out.errors);
                    app.fix_selection();
                    editing.set(false);
                    if n > 0 {
                        app.toast(format!("Applied with {}", plural(n, "parse problem")), ToastKind::Error);
                    } else {
                        app.toast("Source applied".into(), ToastKind::Success);
                    }
                }><Icon icon=Ic::Check class="ic-sm"/>"Apply"</button>
            }>
                <button class="btn btn-sm btn-ghost" on:click=move |_| {
                    persist::copy_text(&text.get_untracked());
                    app.toast("DBC copied to clipboard".into(), ToastKind::Success);
                }><Icon icon=Ic::Copy class="ic-sm"/>"Copy"</button>
                <button class="btn btn-sm btn-ghost" on:click=move |_| persist::download(&app.file_name.get_untracked(), &text.get_untracked(), "text/plain")>
                    <Icon icon=Ic::Download class="ic-sm"/>"Download"
                </button>
                <button class="btn btn-sm" on:click=move |_| {
                    draft.set(text.get_untracked());
                    editing.set(true);
                }><Icon icon=Ic::Code class="ic-sm"/>"Edit source"</button>
            </Show>
        </div>
        {move || if editing.get() {
            view! {
                <textarea class="log-input" style="min-height:100%" spellcheck="false" prop:value=move || draft.get() on:input=move |ev| draft.set(event_target_value(&ev))></textarea>
            }.into_any()
        } else {
            view! { <pre class="raw" inner_html=move || highlight(&text.get())></pre> }.into_any()
        }}
    }
}
