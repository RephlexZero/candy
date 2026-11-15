//! Modal dialogs: naming convention picker with bulk-rename preview, shortcuts, about.

use std::collections::BTreeSet;

use candy_dbc::lint;
use candy_dbc::naming::{self, Case, Convention, Entity};
use leptos::prelude::*;

use crate::actions;
use crate::icons::{Ic, Icon, Logo};
use crate::state::{Modal, ToastKind, use_app};
use crate::ui::plural;

#[component]
pub fn Modals() -> impl IntoView {
    let app = use_app();
    move || {
        let m = app.modal.get()?;
        let body = match m {
            Modal::Naming => view! { <NamingModal/> }.into_any(),
            Modal::Shortcuts => view! { <ShortcutsModal/> }.into_any(),
            Modal::About => view! { <AboutModal/> }.into_any(),
        };
        Some(view! {
            <div class="overlay" on:click=move |_| app.modal.set(None)>
                {body}
            </div>
        })
    }
}

fn head(title: &'static str, icon: Ic) -> impl IntoView {
    let app = use_app();
    view! {
        <div class="modal-head">
            <Icon icon=icon/>
            <h2>{title}</h2>
            <button class="icon-btn" title="Close (Esc)" on:click=move |_| app.modal.set(None)><Icon icon=Ic::X/></button>
        </div>
    }
}

fn rule_summary(c: &Convention) -> String {
    let f = |e: Entity| {
        c.rule(e).map(|r| {
            let case = if r.case == Case::Any {
                "any case".to_string()
            } else {
                r.case.example().to_string()
            };
            match r.max_len {
                Some(n) => format!("{case} ≤{n}"),
                None => case,
            }
        })
    };
    format!(
        "Msg {} · Sig {}{}",
        f(Entity::Message).unwrap_or_default(),
        f(Entity::Signal).unwrap_or_default(),
        if c.corner_suffixes {
            " · FL/FR/RL/RR"
        } else {
            ""
        }
    )
}

#[component]
fn NamingModal() -> impl IntoView {
    let app = use_app();
    let presets = naming::presets();
    let current = app.convention.get_untracked().map(|c| c.id);
    let chosen = RwSignal::new(current.clone());
    let presets_sv = StoredValue::new(presets.clone());
    let conv = Memo::new(move |_| {
        chosen
            .get()
            .and_then(|id| presets_sv.with_value(|p| p.iter().find(|c| c.id == id).cloned()))
            .or_else(|| {
                // A custom convention stored in the file.
                let cur = app.convention.get_untracked()?;
                (Some(cur.id.clone()) == chosen.get()).then_some(cur)
            })
    });
    let preview = Memo::new(move |_| {
        conv.get()
            .map(|c| app.doc.with_untracked(|d| naming::check_dbc(d, &c)))
            .unwrap_or_default()
    });
    let picked = RwSignal::new(BTreeSet::<usize>::new());
    Effect::new(move |_| {
        let n = preview.with(|p| p.len());
        picked.set((0..n).collect());
    });

    let mut categories: Vec<String> = presets.iter().map(|c| c.category.clone()).collect();
    categories.dedup();

    let apply = move |rename: bool| {
        let c = conv.get_untracked();
        let issues = preview.get_untracked();
        let sel = picked.get_untracked();
        let changing =
            app.convention.get_untracked().map(|x| x.id) != c.as_ref().map(|x| x.id.clone());
        if changing {
            actions::set_convention(app, c.as_ref());
        }
        if rename && !sel.is_empty() {
            let mut n = 0;
            app.edit("rename to convention", |d| {
                for i in &sel {
                    if let Some(is) = issues.get(*i)
                        && lint::rename(d, &is.target, &is.suggestion)
                    {
                        n += 1;
                    }
                }
            });
            app.fix_selection();
            app.toast(format!("Renamed {}", plural(n, "item")), ToastKind::Success);
        }
        app.modal.set(None);
    };

    view! {
        <div class="modal" on:click=|ev| ev.stop_propagation()>
            {head("Naming convention", Ic::Tag)}
            <div class="modal-body">
                <p class="muted" style="margin-top:0">
                    "CANdy checks every name against the chosen convention, explains what is wrong and suggests a fix. The choice is stored in the DBC (attribute "
                    <code class="mono">{naming::CONVENTION_ATTR}</code>") so your team gets the same checks."
                </p>
                <div class="conv-grid">
                    <button class="conv-card" class:on=move || chosen.get().is_none() on:click=move |_| chosen.set(None)>
                        <div class="name">"None"</div>
                        <div class="desc">"Only basic DBC identifier rules."</div>
                    </button>
                </div>
                {categories.into_iter().map(|cat| {
                    let list: Vec<Convention> = presets.iter().filter(|c| c.category == cat).cloned().collect();
                    view! {
                        <div class="eyebrow" style="margin:16px 0 8px">{cat.clone()}</div>
                        <div class="conv-grid">
                            {list.into_iter().map(|c| {
                                let id = c.id.clone();
                                let id2 = c.id.clone();
                                let summary = rule_summary(&c);
                                view! {
                                    <button class="conv-card" class:on=move || chosen.get().as_deref() == Some(id.as_str()) on:click=move |_| chosen.set(Some(id2.clone()))>
                                        <div class="name">{c.name.clone()}</div>
                                        <div class="desc">{c.description.clone()}</div>
                                        <code class="mono">{summary}</code>
                                    </button>
                                }
                            }).collect_view()}
                        </div>
                    }
                }).collect_view()}

                <div class="eyebrow" style="margin:20px 0 6px">
                    {move || match conv.get() {
                        None => "Preview".to_string(),
                        Some(_) => format!("Preview · {} not following it", plural(preview.with(|p| p.len()), "name")),
                    }}
                </div>
                {move || {
                    let list = preview.get();
                    if conv.with(|c| c.is_none()) {
                        return view! { <div class="hint">"Pick a convention to preview renames."</div> }.into_any();
                    }
                    if list.is_empty() {
                        return view! { <div class="hint"><Icon icon=Ic::Check class="ic-sm"/>" Every name already follows this convention."</div> }.into_any();
                    }
                    view! {
                        <div style="max-height:280px;overflow:auto">
                            {list.into_iter().enumerate().take(400).map(|(i, is)| view! {
                                <label class="rename-row">
                                    <input type="checkbox" class="check" prop:checked=move || picked.with(|p| p.contains(&i)) on:change=move |_| picked.update(|p| if !p.remove(&i) { p.insert(i); })/>
                                    <span class="mono diff-old" title=is.entity.label()>{is.name.clone()}</span>
                                    <span class="subtle">"→"</span>
                                    <span class="mono diff-new">{is.suggestion.clone()}</span>
                                    <span class="why">{format!("{} · {}", is.entity.label(), is.reasons.join("; "))}</span>
                                </label>
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}
            </div>
            <div class="modal-foot">
                <button class="btn btn-ghost" on:click=move |_| app.modal.set(None)>"Cancel"</button>
                <button class="btn" on:click=move |_| apply(false)>"Use convention"</button>
                <button class="btn btn-primary" prop:disabled=move || picked.with(|p| p.is_empty()) || conv.with(|c| c.is_none()) on:click=move |_| apply(true)>
                    <Icon icon=Ic::Wand class="ic-sm"/>{move || format!("Use & rename {}", picked.with(|p| p.len()))}
                </button>
            </div>
        </div>
    }
}

#[component]
fn ShortcutsModal() -> impl IntoView {
    let rows: &[(&str, &str)] = &[
        ("Command palette / search", "Ctrl K"),
        ("Open file", "Ctrl O"),
        ("Save / Save as", "Ctrl S / Ctrl ⇧ S"),
        ("Undo / Redo", "Ctrl Z / Ctrl ⇧ Z"),
        ("Duplicate selection", "Ctrl D"),
        ("Delete selection", "Del"),
        ("Toggle bottom panel", "Ctrl J"),
        ("New message", "M"),
        ("New signal", "N"),
        ("Filter the tree", "/"),
        ("Problems · Decode · Source", "P · X · R"),
        ("Hex / decimal ids", "H"),
        ("Nudge signal in grid", "← → ↑ ↓"),
        ("Compare a file (drop)", "⇧ + drop"),
        ("This help", "?"),
    ];
    view! {
        <div class="modal narrow" on:click=|ev| ev.stop_propagation()>
            {head("Keyboard shortcuts", Ic::Keyboard)}
            <div class="modal-body">
                <div class="shortcuts">
                    {rows.iter().map(|(a, k)| view! {
                        <span>{*a}</span>
                        <span>{k.split(' ').filter(|p| !p.is_empty()).map(|p| if p == "/" || p == "·" || p == "+" { view! { <span class="subtle">{format!(" {p} ")}</span> }.into_any() } else { view! { <span class="kbd">{p.to_string()}</span> }.into_any() }).collect_view()}</span>
                    }).collect_view()}
                </div>
            </div>
        </div>
    }
}

#[component]
fn AboutModal() -> impl IntoView {
    view! {
        <div class="modal narrow" on:click=|ev| ev.stop_propagation()>
            {head("About CANdy", Ic::Info)}
            <div class="modal-body" style="text-align:center">
                <Logo class="logo hero-logo"/>
                <h2 style="margin:8px 0 4px">"CAN"<b>"dy"</b></h2>
                <p class="muted">"A sweet, fully in-browser CAN DBC editor written in Rust and compiled to WebAssembly. Your files never leave your machine."</p>
                <p class="hint">{format!("Version {}", env!("CARGO_PKG_VERSION"))}" · MIT licence"</p>
                <p><a href="https://github.com/RephlexZero/candy" target="_blank" rel="noopener">"github.com/RephlexZero/candy"</a></p>
            </div>
        </div>
    }
}
