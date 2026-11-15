//! First-run page: open, new, examples, restore last session.

use leptos::prelude::*;

use crate::actions;
use crate::examples::EXAMPLES;
use crate::icons::{Ic, Icon, Logo};
use crate::persist;
use crate::state::use_app;

#[component]
pub fn Welcome() -> impl IntoView {
    let app = use_app();
    let last = persist::get("candy.doc")
        .filter(|t| !t.trim().is_empty())
        .map(|t| {
            (
                persist::get("candy.name").unwrap_or_else(|| "untitled.dbc".into()),
                t,
            )
        });
    let last = StoredValue::new(last);

    let features: &[(Ic, &str, &str)] = &[
        (
            Ic::Hash,
            "Drag-and-drop bit layout",
            "Move, resize and create signals right on the byte grid, Intel and Motorola alike.",
        ),
        (
            Ic::Mux,
            "Real multiplexing",
            "Simple, nested and extended (SG_MUL_VAL_) mux with per-branch views.",
        ),
        (
            Ic::Tag,
            "Naming conventions",
            "Vector, AUTOSAR, J1939, motorsport, snake/camel… with one-click renames.",
        ),
        (
            Ic::Wand,
            "Lint & auto-fix",
            "Overlaps, ranges, DLC, duplicates, unknown nodes — explained and fixable.",
        ),
        (
            Ic::Zap,
            "Live decode",
            "Paste candump, ASC or TRC logs and see every value decoded instantly.",
        ),
        (
            Ic::Diff,
            "Diff & merge",
            "Compare two DBCs and cherry-pick changes, or review your own edits.",
        ),
        (
            Ic::Folder,
            "Tree & folders",
            "Group by ID, sender or your own folders; filter everything as you type.",
        ),
        (
            Ic::Palette,
            "Eight colour schemes",
            "From warm Stone to Paddock red and Phosphor green.",
        ),
    ];

    view! {
        <div class="welcome">
            <div class="welcome-inner">
                <div class="hero">
                    <Logo class="logo"/>
                    <h1>"CAN"<em>"dy"</em></h1>
                    <p>"The sweetest CAN DBC editor. Fast, friendly and fully in your browser — nothing is uploaded."</p>
                </div>
                <div class="hero-actions">
                    <button class="btn btn-primary" on:click=move |_| actions::open_file(app)><Icon icon=Ic::Open/>"Open a DBC"</button>
                    <button class="btn" on:click=move |_| app.new_document()><Icon icon=Ic::FilePlus/>"New network"</button>
                </div>
                {move || last.get_value().map(|(name, text)| view! {
                    <div class="restore">
                        <span>"Pick up where you left off:"</span>
                        <button class="btn btn-sm" on:click=move |_| {
                            app.load(&text, &name);
                        }><Icon icon=Ic::Undo class="ic-sm"/>{name.clone()}</button>
                    </div>
                })}
                <div class="hint" style="text-align:center;margin-top:10px">"…or drop a file anywhere, or paste DBC text."</div>

                <div class="examples">
                    {EXAMPLES.iter().enumerate().map(|(i, ex)| view! {
                        <button class="card example-card" on:click=move |_| actions::load_example(app, i)>
                            <div class="t"><Icon icon=Ic::Sparkles class="ic-sm"/>{ex.title}</div>
                            <div class="d">{ex.blurb}</div>
                            <div class="chips">{ex.tags.iter().map(|t| view! { <span class="chip">{*t}</span> }).collect_view()}</div>
                        </button>
                    }).collect_view()}
                </div>

                <div class="features">
                    {features.iter().map(|(icon, title, text)| view! {
                        <div class="feature">
                            <div class="fi"><Icon icon=*icon/></div>
                            <div><h4>{*title}</h4><p>{*text}</p></div>
                        </div>
                    }).collect_view()}
                </div>

                <div class="footer-note">
                    "Made with Rust + WebAssembly · MIT · "
                    <a href="https://github.com/RephlexZero/candy" target="_blank" rel="noopener">"Source on GitHub"</a>
                </div>
            </div>
        </div>
    }
}
