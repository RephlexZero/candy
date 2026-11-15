//! Small shared UI pieces and formatting helpers.

use candy_dbc::{Message, fmt_num};
use leptos::prelude::*;

use crate::state::{Card, Hover, use_app};

/// Hue for the n-th signal of a message (golden-angle spacing keeps neighbours distinct).
pub fn hue(index: usize) -> f64 {
    (index as f64 * 137.508 + 28.0) % 360.0
}

pub fn hue_style(index: usize) -> String {
    format!("--h:{:.1}", hue(index))
}

pub fn fmt_id(m: &Message, hex: bool) -> String {
    if hex {
        m.id_hex()
    } else {
        m.can_id().to_string()
    }
}

pub fn parse_num(s: &str) -> Option<f64> {
    let s = s.trim().replace(',', ".");
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return i64::from_str_radix(h, 16).ok().map(|v| v as f64);
    }
    s.parse().ok().filter(|v: &f64| v.is_finite())
}

pub fn parse_id(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(h, 16).ok()
    } else if s
        .chars()
        .any(|c| c.is_ascii_hexdigit() && c.is_ascii_alphabetic())
    {
        u32::from_str_radix(s, 16).ok()
    } else {
        s.parse().ok()
    }
}

pub fn num(v: f64) -> String {
    fmt_num(candy_dbc::lint::round_sig(v))
}

pub fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

/// Attach to `on:mouseenter` to show a hover card after a short delay.
pub fn hover_in(card: Card) -> impl Fn(leptos::ev::MouseEvent) + 'static {
    move |ev: leptos::ev::MouseEvent| {
        let app = use_app();
        let (x, y) = (ev.client_x() as f64, ev.client_y() as f64);
        let card = card.clone();
        let token = js_sys::Math::random();
        HOVER_TOKEN.with(|t| t.set(token));
        set_timeout(
            move || {
                if HOVER_TOKEN.with(|t| t.get()) == token {
                    app.hover.set(Some(Hover { card, x, y }));
                }
            },
            std::time::Duration::from_millis(380),
        );
    }
}

pub fn hover_out() -> impl Fn(leptos::ev::MouseEvent) + 'static {
    move |_| {
        HOVER_TOKEN.with(|t| t.set(0.0));
        use_app().hover.set(None);
    }
}

thread_local! {
    static HOVER_TOKEN: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// A labelled form field.
#[component]
pub fn Field(
    #[prop(into)] label: String,
    #[prop(optional, into)] hint: Option<String>,
    children: Children,
) -> impl IntoView {
    view! {
        <label class="field">
            <span class="field-label">{label}</span>
            {children()}
            {hint.map(|h| view! { <span class="field-hint">{h}</span> })}
        </label>
    }
}

/// Text input that commits on change (blur / Enter).
#[component]
pub fn TextIn(
    #[prop(into)] value: Signal<String>,
    on_commit: impl Fn(String) + 'static,
    #[prop(optional, into)] class: String,
    #[prop(optional, into)] placeholder: String,
    #[prop(optional, into)] list: String,
    #[prop(optional, into)] id: String,
) -> impl IntoView {
    let class = if class.is_empty() {
        "input".to_string()
    } else {
        class
    };
    view! {
        <input
            class=class
            id=(!id.is_empty()).then_some(id)
            list=(!list.is_empty()).then_some(list)
            placeholder=placeholder
            spellcheck="false"
            autocomplete="off"
            prop:value=move || value.get()
            on:change=move |ev| on_commit(event_target_value(&ev))
            on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                if ev.key() == "Enter" {
                    let el: web_sys::HtmlElement = wasm_bindgen::JsCast::unchecked_into(event_target::<web_sys::HtmlElement>(&ev));
                    let _ = el.blur();
                }
            }
        />
    }
}

/// Numeric input: shows a formatted number, commits parsed values, flags invalid input.
#[component]
pub fn NumIn(
    #[prop(into)] value: Signal<f64>,
    on_commit: impl Fn(f64) + 'static,
    #[prop(optional, into)] class: String,
    #[prop(optional)] integer: bool,
) -> impl IntoView {
    let bad = RwSignal::new(false);
    let base = if class.is_empty() {
        "input mono num".to_string()
    } else {
        class
    };
    view! {
        <input
            class=move || format!("{base}{}", if bad.get() { " bad" } else { "" })
            inputmode="decimal"
            spellcheck="false"
            prop:value=move || num(value.get())
            on:input=move |ev| bad.set(parse_num(&event_target_value(&ev)).is_none())
            on:change=move |ev| {
                if let Some(v) = parse_num(&event_target_value(&ev)) {
                    bad.set(false);
                    on_commit(if integer { v.round() } else { v });
                }
            }
            on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                if ev.key() == "Enter" {
                    let el: web_sys::HtmlElement = wasm_bindgen::JsCast::unchecked_into(event_target::<web_sys::HtmlElement>(&ev));
                    let _ = el.blur();
                }
            }
        />
    }
}
