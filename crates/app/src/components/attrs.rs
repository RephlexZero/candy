//! Attribute editor shared by the network, node, message and signal inspectors.

use candy_dbc::{AttrKind, AttrObject, AttrValue, Attribute, Dbc, set_attribute};
use leptos::prelude::*;

use crate::state::use_app;

/// Where the attribute list lives in the document.
#[derive(Clone, PartialEq, Debug)]
pub enum Owner {
    Network,
    Node(String),
    Message(u32),
    Signal(u32, String),
}

impl Owner {
    fn object(&self) -> AttrObject {
        match self {
            Owner::Network => AttrObject::Network,
            Owner::Node(_) => AttrObject::Node,
            Owner::Message(_) => AttrObject::Message,
            Owner::Signal(..) => AttrObject::Signal,
        }
    }
    fn list<'a>(&self, d: &'a Dbc) -> Option<&'a Vec<Attribute>> {
        match self {
            Owner::Network => Some(&d.attributes),
            Owner::Node(n) => d.node(n).map(|n| &n.attributes),
            Owner::Message(id) => d.message(*id).map(|m| &m.attributes),
            Owner::Signal(id, s) => d
                .message(*id)
                .and_then(|m| m.signal(s))
                .map(|s| &s.attributes),
        }
    }
    fn list_mut<'a>(&self, d: &'a mut Dbc) -> Option<&'a mut Vec<Attribute>> {
        match self {
            Owner::Network => Some(&mut d.attributes),
            Owner::Node(n) => d
                .nodes
                .iter_mut()
                .find(|x| &x.name == n)
                .map(|n| &mut n.attributes),
            Owner::Message(id) => d.message_mut(*id).map(|m| &mut m.attributes),
            Owner::Signal(id, s) => d
                .message_mut(*id)
                .and_then(|m| m.signal_mut(s))
                .map(|s| &mut s.attributes),
        }
    }
}

/// Parse user input for an attribute of the given kind.
fn parse_value(kind: &AttrKind, s: &str) -> Option<AttrValue> {
    let s = s.trim();
    match kind {
        AttrKind::Int { .. } | AttrKind::Hex { .. } => {
            crate::ui::parse_num(s).map(|v| AttrValue::Int(v.round() as i64))
        }
        AttrKind::Float { .. } => crate::ui::parse_num(s).map(AttrValue::Float),
        AttrKind::String => Some(AttrValue::Str(s.to_string())),
        AttrKind::Enum(opts) => opts
            .iter()
            .position(|o| o == s)
            .map(|i| AttrValue::Int(i as i64)),
    }
}

/// Lists every attribute defined for this object type with an inline editor; values that equal
/// the default are shown as placeholders. CANdy's own attributes are hidden (they have dedicated UI).
#[component]
pub fn AttrEditor(owner: Owner) -> impl IntoView {
    let app = use_app();
    let obj = owner.object();
    let rows = Memo::new({
        let owner = owner.clone();
        move |_| {
            app.doc.with(|d| {
                d.attr_defs
                    .iter()
                    .filter(|a| a.object == obj && !a.name.starts_with("CANdy_"))
                    .map(|a| {
                        let cur = owner
                            .list(d)
                            .and_then(|l| l.iter().find(|x| x.name == a.name))
                            .map(|x| x.value.clone());
                        let def = d.attr_default(&a.name).cloned();
                        (a.name.clone(), a.kind.clone(), cur, def)
                    })
                    .collect::<Vec<_>>()
            })
        }
    });
    move || {
        let rows = rows.get();
        if rows.is_empty() {
            return view! { <div class="hint">"No attributes are defined for this object type."</div> }.into_any();
        }
        rows.into_iter()
            .map(|(name, kind, cur, def)| {
                let owner = owner.clone();
                let n2 = name.clone();
                let kind2 = kind.clone();
                let commit = move |text: String| {
                    let owner = owner.clone();
                    let name = n2.clone();
                    let value = if text.trim().is_empty() { Some(None) } else { parse_value(&kind2, &text).map(Some) };
                    let Some(value) = value else {
                        app.toast(format!("“{text}” is not a valid {} value", kind2.label()), crate::state::ToastKind::Error);
                        return;
                    };
                    app.edit(&format!("set {name}"), move |d| {
                        if let Some(list) = owner.list_mut(d) {
                            set_attribute(list, &name, value);
                        }
                    });
                };
                let shown = |v: &AttrValue| match (&kind, v) {
                    (AttrKind::Enum(o), AttrValue::Int(i)) => o.get(*i as usize).cloned().unwrap_or_else(|| i.to_string()),
                    (AttrKind::Hex { .. }, AttrValue::Int(i)) => format!("0x{i:X}"),
                    _ => v.display(),
                };
                let value = cur.as_ref().map(shown).unwrap_or_default();
                let placeholder = def.as_ref().map(shown).map(|d| format!("default: {d}")).unwrap_or_default();
                let editor = match &kind {
                    AttrKind::Enum(opts) => {
                        let opts = opts.clone();
                        view! {
                            <select class="select" on:change=move |ev| commit(event_target_value(&ev))>
                                <option value="" selected=value.is_empty()>{placeholder.clone()}</option>
                                {opts
                                    .into_iter()
                                    .map(|o| {
                                        let sel = o == value;
                                        view! { <option value=o.clone() selected=sel>{o.clone()}</option> }
                                    })
                                    .collect_view()}
                            </select>
                        }
                        .into_any()
                    }
                    _ => view! {
                        <input
                            class="input mono"
                            placeholder=placeholder
                            prop:value=value
                            spellcheck="false"
                            on:change=move |ev| commit(event_target_value(&ev))
                        />
                    }
                    .into_any(),
                };
                view! {
                    <label class="field">
                        <span class="field-label">
                            {name.clone()}
                            <span class="subtle mono" style="font-weight:500">{kind.label()}</span>
                        </span>
                        {editor}
                    </label>
                }
                .into_any()
            })
            .collect_view()
            .into_any()
    }
}
