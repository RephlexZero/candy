use leptos::prelude::*;

/// Stroke icons on a 24×24 grid (paths adapted from Lucide, ISC licence).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ic {
    Open,
    Save,
    FilePlus,
    Download,
    Undo,
    Redo,
    Search,
    Palette,
    ChevronRight,
    ChevronDown,
    Plus,
    Trash,
    Copy,
    Error,
    Warning,
    Info,
    Check,
    X,
    Wand,
    Node,
    Message,
    Signal,
    Mux,
    Table,
    Network,
    Keyboard,
    Github,
    Code,
    Zap,
    Diff,
    Tag,
    Folder,
    Hash,
    Panel,
    Sparkles,
    Gauge,
    Arrow,
}

impl Ic {
    fn paths(self) -> &'static str {
        match self {
            Ic::Open => {
                r#"<path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2"/>"#
            }
            Ic::Save => {
                r#"<path d="M15.2 3a2 2 0 0 1 1.4.6l3.8 3.8a2 2 0 0 1 .6 1.4V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z"/><path d="M17 21v-7a1 1 0 0 0-1-1H8a1 1 0 0 0-1 1v7"/><path d="M7 3v4a1 1 0 0 0 1 1h7"/>"#
            }
            Ic::FilePlus => {
                r#"<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="M9 15h6"/><path d="M12 18v-6"/>"#
            }
            Ic::Download => {
                r#"<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/><path d="M12 15V3"/>"#
            }
            Ic::Undo => {
                r#"<path d="M3 7v6h6"/><path d="M21 17a9 9 0 0 0-9-9 9 9 0 0 0-6 2.3L3 13"/>"#
            }
            Ic::Redo => {
                r#"<path d="M21 7v6h-6"/><path d="M3 17a9 9 0 0 1 9-9 9 9 0 0 1 6 2.3l3 2.7"/>"#
            }
            Ic::Search => r#"<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>"#,
            Ic::Palette => {
                r#"<circle cx="13.5" cy="6.5" r=".5" fill="currentColor"/><circle cx="17.5" cy="10.5" r=".5" fill="currentColor"/><circle cx="8.5" cy="7.5" r=".5" fill="currentColor"/><circle cx="6.5" cy="12.5" r=".5" fill="currentColor"/><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.93 0 1.65-.75 1.65-1.69 0-.44-.18-.84-.44-1.13-.29-.29-.44-.65-.44-1.13a1.64 1.64 0 0 1 1.67-1.67h2c3.05 0 5.55-2.5 5.55-5.55C21.97 6.01 17.46 2 12 2z"/>"#
            }
            Ic::ChevronRight => r#"<path d="m9 18 6-6-6-6"/>"#,
            Ic::ChevronDown => r#"<path d="m6 9 6 6 6-6"/>"#,
            Ic::Plus => r#"<path d="M5 12h14"/><path d="M12 5v14"/>"#,
            Ic::Trash => {
                r#"<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>"#
            }
            Ic::Copy => {
                r#"<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>"#
            }
            Ic::Error => {
                r#"<circle cx="12" cy="12" r="10"/><path d="m15 9-6 6"/><path d="m9 9 6 6"/>"#
            }
            Ic::Warning => {
                r#"<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/>"#
            }
            Ic::Info => {
                r#"<circle cx="12" cy="12" r="10"/><path d="M12 16v-4"/><path d="M12 8h.01"/>"#
            }
            Ic::Check => r#"<path d="M20 6 9 17l-5-5"/>"#,
            Ic::X => r#"<path d="M18 6 6 18"/><path d="m6 6 12 12"/>"#,
            Ic::Wand => {
                r#"<path d="m21.64 3.64-1.28-1.28a1.21 1.21 0 0 0-1.72 0L2.36 18.64a1.21 1.21 0 0 0 0 1.72l1.28 1.28a1.2 1.2 0 0 0 1.72 0L21.64 5.36a1.2 1.2 0 0 0 0-1.72"/><path d="m14 7 3 3"/><path d="M5 6v4"/><path d="M19 14v4"/><path d="M10 2v2"/><path d="M7 8H3"/><path d="M21 16h-4"/><path d="M11 3H9"/>"#
            }
            Ic::Node => {
                r#"<rect width="16" height="16" x="4" y="4" rx="2"/><rect width="6" height="6" x="9" y="9" rx="1"/><path d="M15 2v2"/><path d="M15 20v2"/><path d="M2 15h2"/><path d="M2 9h2"/><path d="M20 15h2"/><path d="M20 9h2"/><path d="M9 2v2"/><path d="M9 20v2"/>"#
            }
            Ic::Message => {
                r#"<rect width="20" height="14" x="2" y="5" rx="2"/><path d="M2 10h20"/><path d="M7 15h2"/><path d="M12 15h5"/>"#
            }
            Ic::Signal => r#"<path d="M2 12h3l3-8 4 16 3-8h7"/>"#,
            Ic::Mux => {
                r#"<path d="m12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z"/><path d="m22 17.65-9.17 4.16a2 2 0 0 1-1.66 0L2 17.65"/><path d="m22 12.65-9.17 4.16a2 2 0 0 1-1.66 0L2 12.65"/>"#
            }
            Ic::Table => {
                r#"<path d="M12 3v18"/><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 9h18"/><path d="M3 15h18"/>"#
            }
            Ic::Network => {
                r#"<rect x="16" y="16" width="6" height="6" rx="1"/><rect x="2" y="16" width="6" height="6" rx="1"/><rect x="9" y="2" width="6" height="6" rx="1"/><path d="M5 16v-3a1 1 0 0 1 1-1h12a1 1 0 0 1 1 1v3"/><path d="M12 12V8"/>"#
            }
            Ic::Keyboard => {
                r#"<path d="M10 8h.01"/><path d="M12 12h.01"/><path d="M14 8h.01"/><path d="M16 12h.01"/><path d="M18 8h.01"/><path d="M6 8h.01"/><path d="M7 16h10"/><path d="M8 12h.01"/><rect width="20" height="16" x="2" y="4" rx="2"/>"#
            }
            Ic::Github => {
                r#"<path d="M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4"/><path d="M9 18c-4.51 2-5-2-7-2"/>"#
            }
            Ic::Code => r#"<path d="m16 18 6-6-6-6"/><path d="m8 6-6 6 6 6"/>"#,
            Ic::Zap => {
                r#"<path d="M4 14a1 1 0 0 1-.78-1.63l9.9-10.2a.5.5 0 0 1 .86.46l-1.92 6.02A1 1 0 0 0 13 10h7a1 1 0 0 1 .78 1.63l-9.9 10.2a.5.5 0 0 1-.86-.46l1.92-6.02A1 1 0 0 0 11 14z"/>"#
            }
            Ic::Diff => {
                r#"<circle cx="18" cy="18" r="3"/><circle cx="6" cy="6" r="3"/><path d="M13 6h3a2 2 0 0 1 2 2v7"/><path d="M11 18H8a2 2 0 0 1-2-2V9"/>"#
            }
            Ic::Tag => {
                r#"<path d="M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8.704 8.704a2.426 2.426 0 0 0 3.42 0l6.58-6.58a2.426 2.426 0 0 0 0-3.42z"/><circle cx="7.5" cy="7.5" r=".5" fill="currentColor"/>"#
            }
            Ic::Folder => {
                r#"<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>"#
            }
            Ic::Hash => {
                r#"<path d="M4 9h16"/><path d="M4 15h16"/><path d="M10 3 8 21"/><path d="M16 3l-2 18"/>"#
            }
            Ic::Panel => r#"<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 15h18"/>"#,
            Ic::Sparkles => {
                r#"<path d="M9.94 15.5A2 2 0 0 0 8.5 14.06l-6.13-1.58a.5.5 0 0 1 0-.96L8.5 9.94A2 2 0 0 0 9.94 8.5l1.58-6.13a.5.5 0 0 1 .96 0L14.06 8.5A2 2 0 0 0 15.5 9.94l6.13 1.58a.5.5 0 0 1 0 .96L15.5 14.06a2 2 0 0 0-1.44 1.44l-1.58 6.13a.5.5 0 0 1-.96 0z"/>"#
            }
            Ic::Gauge => r#"<path d="m12 14 4-4"/><path d="M3.34 19a10 10 0 1 1 17.32 0"/>"#,
            Ic::Arrow => r#"<path d="M5 12h14"/><path d="m12 5 7 7-7 7"/>"#,
        }
    }
}

#[component]
pub fn Icon(icon: Ic, #[prop(default = "ic")] class: &'static str) -> impl IntoView {
    view! {
        <svg
            class=class
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.75"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
            inner_html=icon.paths()
        ></svg>
    }
}

/// The CANdy mark: a wrapped sweet whose stripes are CAN bit cells.
#[component]
pub fn Logo(#[prop(default = "logo")] class: &'static str) -> impl IntoView {
    view! {
        <svg class=class viewBox="0 0 32 32" aria-hidden="true">
            <path d="M6 16 1.5 11.5v9z" fill="rgb(var(--accent))" opacity=".55"/>
            <path d="M26 16l4.5-4.5v9z" fill="rgb(var(--accent))" opacity=".55"/>
            <rect x="6" y="8" width="20" height="16" rx="8" fill="rgb(var(--accent))"/>
            <path d="M11 8.6v14.8M16 8v16M21 8.6v14.8" stroke="rgb(var(--on-accent))" stroke-width="2.2" opacity=".9"/>
        </svg>
    }
}
