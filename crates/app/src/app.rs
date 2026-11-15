use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::actions::{self, THEMES};
use crate::components::{
    dock::Dock, inspector::Inspector, main_view::MainView, modals::Modals, overlays::*,
    palette::Palette, tree::Tree, welcome::Welcome,
};
use crate::icons::{Ic, Icon, Logo};
use crate::persist;
use crate::state::{AppState, DockTab, Modal, use_app};

#[component]
pub fn App() -> impl IntoView {
    let app = AppState::new();
    provide_context(app);
    persist::set_root_attr("data-theme", &app.theme.get_untracked());

    // Restore a document passed by URL, or the last autosaved session (offered on the welcome page).
    if let Some(url) = persist::query_param("url") {
        leptos::task::spawn_local(async move {
            match persist::fetch_text(&url).await {
                Ok(text) => {
                    let name = url.rsplit('/').next().unwrap_or("remote.dbc").to_string();
                    app.load(&text, &name);
                }
                Err(e) => app.toast(
                    format!("Could not fetch {url}: {e}"),
                    crate::state::ToastKind::Error,
                ),
            }
        });
    } else if let Some(ex) = persist::query_param("example").and_then(|e| e.parse::<usize>().ok()) {
        actions::load_example(app, ex);
    }

    // Autosave (debounced) whenever the document changes.
    let save_token = StoredValue::new(0u64);
    Effect::new(move |_| {
        app.doc.track();
        app.file_name.track();
        if !app.has_doc.get_untracked() {
            return;
        }
        let token = save_token.get_value() + 1;
        save_token.set_value(token);
        set_timeout(
            move || {
                if save_token.get_value() == token {
                    let text = app.doc.with_untracked(candy_dbc::write);
                    if persist::set("candy.doc", &text) {
                        persist::set("candy.name", &app.file_name.get_untracked());
                    }
                }
            },
            std::time::Duration::from_millis(600),
        );
    });

    // Page title reflects file + dirty state.
    Effect::new(move |_| {
        let name = app.file_name.get();
        let dirty = app.dirty.get();
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            doc.set_title(&if app.has_doc.get() {
                format!("{}{name} — CANdy", if dirty { "• " } else { "" })
            } else {
                "CANdy — the sweetest CAN DBC editor".into()
            });
        }
    });

    install_shortcuts(app);
    let dragging = install_drop(app);

    view! {
        <div class="app">
            <TopBar/>
            {move || {
                if app.has_doc.get() {
                    view! {
                        <div class="workspace">
                            <aside class="pane-tree"><Tree/></aside>
                            <main class="pane-main" id="main"><MainView/></main>
                            <aside class="pane-insp"><Inspector/></aside>
                            <Dock/>
                        </div>
                    }
                    .into_any()
                } else {
                    view! { <Welcome/> }.into_any()
                }
            }}
        </div>
        <Palette/>
        <Modals/>
        <HoverCard/>
        <Toasts/>
        <Show when=move || dragging.get()>
            <div class="drag-overlay">
                <div>
                    <Icon icon=Ic::Open class="ic-lg"/>
                    <div style="margin-top:8px">"Drop a .dbc file to open it"</div>
                    <div class="muted" style="font-weight:450;font-size:13px;margin-top:2px">
                        "Hold Shift to compare it with the current file instead"
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[component]
fn TopBar() -> impl IntoView {
    let app = use_app();
    let theme_open = RwSignal::new(false);
    let conv_name = move || {
        app.convention
            .get()
            .map(|c| c.name)
            .unwrap_or_else(|| "No convention".into())
    };
    let counts = Memo::new(move |_| app.issues_for(|_| true));

    view! {
        <header class="topbar">
            <div class="brand">
                <Logo/>
                <span class="brand-name">"CAN"<b>"dy"</b></span>
            </div>
            <Show when=move || app.has_doc.get()>
                <div class="file-name hide-sm" title=move || app.file_name.get()>
                    <Show when=move || app.dirty.get()>
                        <span class="dirty-dot" title="Unsaved changes"></span>
                    </Show>
                    <span>{move || app.file_name.get()}</span>
                </div>
            </Show>
            <div class="tb-sep hide-sm"></div>
            <button class="icon-btn" title="New network" on:click=move |_| app.new_document()>
                <Icon icon=Ic::FilePlus/>
            </button>
            <button class="icon-btn" title="Open… (Ctrl+O)" on:click=move |_| actions::open_file(app)>
                <Icon icon=Ic::Open/>
            </button>
            <Show when=move || app.has_doc.get()>
                <button class="icon-btn" title="Save (Ctrl+S)" on:click=move |_| actions::save(app, false)>
                    <Icon icon=Ic::Save/>
                </button>
                <div class="tb-sep"></div>
                <button
                    class="icon-btn"
                    title=move || app.undo_label().map(|l| format!("Undo {l} (Ctrl+Z)")).unwrap_or_else(|| "Nothing to undo".into())
                    prop:disabled=move || app.history_len.get().0 == 0
                    on:click=move |_| app.undo()
                >
                    <Icon icon=Ic::Undo/>
                </button>
                <button
                    class="icon-btn"
                    title=move || app.redo_label().map(|l| format!("Redo {l} (Ctrl+Shift+Z)")).unwrap_or_else(|| "Nothing to redo".into())
                    prop:disabled=move || app.history_len.get().1 == 0
                    on:click=move |_| app.redo()
                >
                    <Icon icon=Ic::Redo/>
                </button>
            </Show>
            <div class="tb-spacer"></div>
            <Show when=move || app.has_doc.get()>
                <button class="search-trigger" on:click=move |_| app.palette_open.set(true)>
                    <Icon icon=Ic::Search class="ic-sm"/>
                    <span>"Search or run a command…"</span>
                    <span class="kbd">"Ctrl K"</span>
                </button>
                <div class="tb-spacer"></div>
                <button class="btn btn-ghost btn-sm hide-sm" title="Naming convention" on:click=move |_| app.modal.set(Some(Modal::Naming))>
                    <Icon icon=Ic::Tag class="ic-sm"/>
                    {conv_name}
                </button>
                <button
                    class="btn btn-ghost btn-sm"
                    title="Problems"
                    on:click=move |_| actions::open_dock(app, DockTab::Problems)
                >
                    {move || {
                        let (e, w) = counts.get();
                        if e == 0 && w == 0 {
                            view! { <span class="badge badge-ok"><Icon icon=Ic::Check class="ic-sm"/>"Clean"</span> }.into_any()
                        } else {
                            view! {
                                <span class="badge badge-err" class:hidden=e == 0><Icon icon=Ic::Error class="ic-sm"/>{e}</span>
                                <span class="badge badge-warn"><Icon icon=Ic::Warning class="ic-sm"/>{w}</span>
                            }
                            .into_any()
                        }
                    }}
                </button>
            </Show>
            <div style="position:relative">
                <button class="icon-btn" title="Theme" on:click=move |_| theme_open.update(|o| *o = !*o)>
                    <Icon icon=Ic::Palette/>
                </button>
                <Show when=move || theme_open.get()>
                    <div class="overlay" style="background:transparent;backdrop-filter:none;padding:0" on:click=move |_| theme_open.set(false)></div>
                    <div class="card" style="position:absolute;right:0;top:38px;width:250px;padding:6px;z-index:101;animation:pop-in 140ms var(--ease)">
                        <div class="eyebrow" style="padding:6px 8px">"Colour scheme"</div>
                        {THEMES
                            .iter()
                            .map(|(id, name, desc)| {
                                let id = *id;
                                view! {
                                    <div
                                        class="pal-item"
                                        class:on=move || app.theme.get() == id
                                        on:click=move |_| {
                                            actions::set_theme(app, id);
                                            theme_open.set(false);
                                        }
                                    >
                                        <ThemeSwatch id=id/>
                                        <div class="grow">
                                            <div style="font-weight:600">{*name}</div>
                                            <div class="hint">{*desc}</div>
                                        </div>
                                    </div>
                                }
                            })
                            .collect_view()}
                    </div>
                </Show>
            </div>
            <button class="icon-btn hide-sm" title="Keyboard shortcuts (?)" on:click=move |_| app.modal.set(Some(Modal::Shortcuts))>
                <Icon icon=Ic::Keyboard/>
            </button>
            <a class="icon-btn hide-sm" title="Source on GitHub" href="https://github.com/RephlexZero/candy" target="_blank" rel="noopener">
                <Icon icon=Ic::Github/>
            </a>
        </header>
    }
}

/// Three-dot preview of a theme's page, surface and accent colours.
#[component]
fn ThemeSwatch(id: &'static str) -> impl IntoView {
    let (a, b, c) = match id {
        "stone-light" => ("#FAF9F7", "#E7E4DF", "#D98014"),
        "stone-dark" => ("#0E0D0C", "#2C2926", "#F0A63C"),
        "midnight" => ("#090D1A", "#202B48", "#38BDF8"),
        "paddock" => ("#0B0B0C", "#28282C", "#EF3340"),
        "phosphor" => ("#040905", "#163219", "#39FF6E"),
        "solarized-light" => ("#FDF6E3", "#E0D8C0", "#268BD2"),
        "solarized-dark" => ("#002B36", "#154855", "#2AA198"),
        "contrast" => ("#000000", "#FFFFFF", "#FFD600"),
        _ => ("#FAF9F7", "#0E0D0C", "#D98014"),
    };
    view! {
        <span style=format!(
            "width:30px;height:30px;border-radius:9px;flex:none;border:1px solid rgb(var(--line));background:linear-gradient(135deg,{a} 0 50%,{b} 50% 100%);position:relative"
        )>
            <span style=format!(
                "position:absolute;right:4px;bottom:4px;width:10px;height:10px;border-radius:50%;background:{c}"
            )></span>
        </span>
    }
}

fn install_shortcuts(app: AppState) {
    let _ = window_event_listener(leptos::ev::keydown, move |ev| {
        let key = ev.key();
        let ctrl = ev.ctrl_key() || ev.meta_key();
        let shift = ev.shift_key();
        let typing = persist::typing();
        if ctrl {
            match key.to_ascii_lowercase().as_str() {
                "k" => {
                    ev.prevent_default();
                    if app.has_doc.get_untracked() {
                        app.palette_open.update(|o| *o = !*o);
                    }
                }
                "s" => {
                    ev.prevent_default();
                    if app.has_doc.get_untracked() {
                        actions::save(app, shift);
                    }
                }
                "o" => {
                    ev.prevent_default();
                    actions::open_file(app);
                }
                "z" if !typing => {
                    ev.prevent_default();
                    if shift { app.redo() } else { app.undo() }
                }
                "y" if !typing => {
                    ev.prevent_default();
                    app.redo();
                }
                "d" if !typing => {
                    ev.prevent_default();
                    actions::duplicate_selection(app);
                }
                "j" => {
                    ev.prevent_default();
                    actions::toggle_dock(app);
                }
                _ => {}
            }
            return;
        }
        if key == "Escape" {
            app.palette_open.set(false);
            app.modal.set(None);
            app.hover.set(None);
            return;
        }
        if typing
            || !app.has_doc.get_untracked()
            || app.palette_open.get_untracked()
            || app.modal.get_untracked().is_some()
        {
            return;
        }
        match key.as_str() {
            "?" => app.modal.set(Some(Modal::Shortcuts)),
            "/" => {
                ev.prevent_default();
                persist::focus("tree-filter");
            }
            "Delete" | "Backspace" => actions::delete_selection(app),
            "n" => actions::add_signal(app, None, None),
            "m" => actions::add_message(app),
            "p" => actions::open_dock(app, DockTab::Problems),
            "r" => actions::open_dock(app, DockTab::Raw),
            "x" => actions::open_dock(app, DockTab::Decode),
            "h" => app.hex_ids.update(|h| *h = !*h),
            _ => {}
        }
    });
}

fn drag_has_files(ev: &web_sys::DragEvent) -> bool {
    ev.data_transfer()
        .is_some_and(|dt| dt.types().includes(&"Files".into(), 0))
}

fn install_drop(app: AppState) -> Memo<bool> {
    let depth = RwSignal::new(0i32);
    let _ = window_event_listener(leptos::ev::dragenter, move |ev| {
        if drag_has_files(&ev) {
            ev.prevent_default();
            depth.update(|d| *d += 1);
        }
    });
    let _ = window_event_listener(leptos::ev::dragover, move |ev| {
        if drag_has_files(&ev) {
            ev.prevent_default();
        }
    });
    let _ = window_event_listener(leptos::ev::dragleave, move |ev| {
        if drag_has_files(&ev) {
            depth.update(|d| *d = (*d - 1).max(0));
        }
    });
    let _ = window_event_listener(leptos::ev::drop, move |ev| {
        depth.set(0);
        if let Some(file) = ev
            .data_transfer()
            .and_then(|dt| dt.files())
            .and_then(|f| f.get(0))
        {
            ev.prevent_default();
            if ev.shift_key() && app.has_doc.get_untracked() {
                crate::components::dock::load_compare_file(app, file);
            } else {
                actions::open_web_file(app, file);
            }
        }
    });
    let _ = window_event_listener_untyped("paste", move |ev| {
        if persist::typing() {
            return;
        }
        let ev: web_sys::ClipboardEvent = ev.unchecked_into();
        if let Some(dt) = ev.clipboard_data() {
            if let Some(file) = dt.files().and_then(|f| f.get(0)) {
                ev.prevent_default();
                actions::open_web_file(app, file);
            } else if let Ok(text) = dt.get_data("text/plain")
                && (text.contains("BO_ ") || text.starts_with("VERSION"))
            {
                ev.prevent_default();
                app.load(&text, "pasted.dbc");
            }
        }
    });
    Memo::new(move |_| depth.get() > 0)
}
