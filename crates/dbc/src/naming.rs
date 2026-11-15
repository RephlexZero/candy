//! Naming conventions: presets drawn from automotive, motorsport and programming practice,
//! checks against them, and suggested fixes.

use crate::lint::Target;
use crate::model::{Dbc, Message};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Case {
    /// No case requirement (only the base identifier rules).
    Any,
    /// `EngineSpeed`
    Pascal,
    /// `engineSpeed`
    Camel,
    /// `engine_speed`
    Snake,
    /// `ENGINE_SPEED`
    ScreamingSnake,
    /// `Engine_Speed`, `Whl_Speed_FL` (Vector / motorsport style)
    PascalSnake,
}

impl Case {
    pub const ALL: [Case; 6] = [Case::Any, Case::Pascal, Case::Camel, Case::Snake, Case::ScreamingSnake, Case::PascalSnake];

    pub fn label(self) -> &'static str {
        match self {
            Case::Any => "Any",
            Case::Pascal => "PascalCase",
            Case::Camel => "camelCase",
            Case::Snake => "snake_case",
            Case::ScreamingSnake => "SCREAMING_SNAKE",
            Case::PascalSnake => "Pascal_Snake",
        }
    }

    pub fn example(self) -> &'static str {
        match self {
            Case::Any => "anything_Goes",
            Case::Pascal => "WheelSpeedFL",
            Case::Camel => "wheelSpeedFL",
            Case::Snake => "wheel_speed_fl",
            Case::ScreamingSnake => "WHEEL_SPEED_FL",
            Case::PascalSnake => "Wheel_Speed_FL",
        }
    }

    pub fn matches(self, s: &str) -> bool {
        let b = s.as_bytes();
        if b.is_empty() {
            return false;
        }
        let alnum = |c: &u8| c.is_ascii_alphanumeric();
        match self {
            Case::Any => true,
            Case::Pascal => b[0].is_ascii_uppercase() && b.iter().all(alnum),
            Case::Camel => b[0].is_ascii_lowercase() && b.iter().all(alnum),
            Case::Snake => {
                b[0].is_ascii_lowercase()
                    && s.split('_').all(|w| !w.is_empty() && w.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
            }
            Case::ScreamingSnake => {
                b[0].is_ascii_uppercase()
                    && s.split('_').all(|w| !w.is_empty() && w.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
            }
            Case::PascalSnake => {
                b[0].is_ascii_uppercase()
                    && s.split('_').all(|w| {
                        let wb = w.as_bytes();
                        !wb.is_empty() && (wb[0].is_ascii_uppercase() || wb[0].is_ascii_digit()) && wb.iter().all(alnum)
                    })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Prefix {
    #[default]
    None,
    /// Name must start with the transmitting node's name (`ECU_EngineSpeed`).
    Transmitter,
    /// Signal name must start with its message's name.
    MessageName,
    Literal(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub case: Case,
    pub max_len: Option<usize>,
    #[serde(default)]
    pub prefix: Prefix,
    #[serde(default)]
    pub forbid_keywords: bool,
    #[serde(default)]
    pub no_double_underscore: bool,
    #[serde(default)]
    pub no_trailing_underscore: bool,
}

impl Rule {
    pub const fn new(case: Case, max_len: Option<usize>) -> Self {
        Self {
            case,
            max_len,
            prefix: Prefix::None,
            forbid_keywords: false,
            no_double_underscore: false,
            no_trailing_underscore: false,
        }
    }
    fn strict(mut self) -> Self {
        self.no_double_underscore = true;
        self.no_trailing_underscore = true;
        self
    }
    fn keywords(mut self) -> Self {
        self.forbid_keywords = true;
        self
    }
    fn prefix(mut self, p: Prefix) -> Self {
        self.prefix = p;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Convention {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub node: Rule,
    pub message: Rule,
    pub signal: Rule,
    pub value_table: Rule,
    pub env_var: Rule,
    /// Rule for `VAL_` labels; `None` leaves them as free text.
    pub enum_label: Option<Rule>,
    /// Require standard wheel-corner suffixes (FL/FR/RL/RR) at the end of signal names.
    #[serde(default)]
    pub corner_suffixes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entity {
    Node,
    Message,
    Signal,
    ValueTable,
    EnvVar,
    EnumLabel,
}

impl Entity {
    pub fn label(self) -> &'static str {
        match self {
            Entity::Node => "Node",
            Entity::Message => "Message",
            Entity::Signal => "Signal",
            Entity::ValueTable => "Value table",
            Entity::EnvVar => "Env. variable",
            Entity::EnumLabel => "Value label",
        }
    }
}

impl Convention {
    pub fn rule(&self, e: Entity) -> Option<&Rule> {
        match e {
            Entity::Node => Some(&self.node),
            Entity::Message => Some(&self.message),
            Entity::Signal => Some(&self.signal),
            Entity::ValueTable => Some(&self.value_table),
            Entity::EnvVar => Some(&self.env_var),
            Entity::EnumLabel => self.enum_label.as_ref(),
        }
    }
}

/// Attribute CANdy stores the active convention in, so it travels with the DBC.
pub const CONVENTION_ATTR: &str = "CANdy_NamingConvention";

pub fn presets() -> Vec<Convention> {
    use Case::*;
    let conv = |id: &str, name: &str, category: &str, description: &str| Convention {
        id: id.into(),
        name: name.into(),
        category: category.into(),
        description: description.into(),
        node: Rule::new(Any, Some(32)),
        message: Rule::new(Any, Some(32)),
        signal: Rule::new(Any, Some(32)),
        value_table: Rule::new(Any, Some(32)),
        env_var: Rule::new(Any, Some(32)),
        enum_label: None,
        corner_suffixes: false,
    };
    vec![
        conv(
            "legacy",
            "Classic DBC",
            "Compatibility",
            "Only what every DBC tool accepts: C identifiers up to 32 characters.",
        ),
        Convention {
            node: Rule::new(Pascal, Some(32)),
            message: Rule::new(PascalSnake, Some(32)).strict(),
            signal: Rule::new(PascalSnake, Some(32)).strict(),
            value_table: Rule::new(PascalSnake, Some(32)).strict(),
            env_var: Rule::new(PascalSnake, Some(32)).strict(),
            ..conv(
                "vector",
                "Automotive OEM",
                "Automotive",
                "Vector CANdb++ house style: Pascal_Snake words, 32-character limit, no stray underscores.",
            )
        },
        Convention {
            node: Rule::new(Any, Some(128)).strict(),
            message: Rule::new(Any, Some(128)).strict(),
            signal: Rule::new(Any, Some(128)).strict(),
            value_table: Rule::new(Any, Some(128)).strict(),
            env_var: Rule::new(Any, Some(128)).strict(),
            enum_label: Some(Rule::new(Any, Some(128)).strict()),
            ..conv(
                "autosar",
                "AUTOSAR short names",
                "Automotive",
                "AUTOSAR shortName rules ([A-Za-z][A-Za-z0-9_]{0,127}, no '__', no trailing '_'), applied to value labels too so they map to CompuMethod symbols.",
            )
        },
        Convention {
            node: Rule::new(Pascal, Some(32)),
            message: Rule::new(ScreamingSnake, Some(32)),
            signal: Rule::new(Pascal, Some(32)),
            value_table: Rule::new(Pascal, Some(32)),
            env_var: Rule::new(Pascal, Some(32)),
            ..conv(
                "j1939",
                "SAE J1939",
                "Heavy-duty",
                "PGN acronyms for messages (EEC1, CCVS1), PascalCase SPN-style signals (EngSpeed).",
            )
        },
        Convention {
            node: Rule::new(ScreamingSnake, Some(16)),
            message: Rule::new(PascalSnake, Some(32)).strict(),
            signal: Rule::new(PascalSnake, Some(32)).strict(),
            value_table: Rule::new(PascalSnake, Some(32)).strict(),
            env_var: Rule::new(PascalSnake, Some(32)).strict(),
            corner_suffixes: true,
            ..conv(
                "motorsport",
                "Motorsport",
                "Motorsport",
                "MoTeC / Bosch logging style: Group_Quantity_Location (Whl_Speed_FL), short upper-case device nodes (ECU, PDM, DASH), standard FL/FR/RL/RR corners.",
            )
        },
        Convention {
            node: Rule::new(Snake, Some(32)).keywords(),
            message: Rule::new(Snake, Some(32)).keywords(),
            signal: Rule::new(Snake, Some(32)).keywords(),
            value_table: Rule::new(Snake, Some(32)).keywords(),
            env_var: Rule::new(Snake, Some(32)).keywords(),
            enum_label: Some(Rule::new(ScreamingSnake, Some(32)).keywords()),
            ..conv(
                "snake",
                "snake_case",
                "Programming",
                "C / Rust / Python friendly: snake_case everywhere, SCREAMING_SNAKE value labels, no language keywords — generated code just works.",
            )
        },
        Convention {
            node: Rule::new(Pascal, Some(32)).keywords(),
            message: Rule::new(Pascal, Some(32)).keywords(),
            signal: Rule::new(Camel, Some(32)).keywords(),
            value_table: Rule::new(Pascal, Some(32)).keywords(),
            env_var: Rule::new(Camel, Some(32)).keywords(),
            enum_label: Some(Rule::new(Pascal, Some(32)).keywords()),
            ..conv(
                "camel",
                "camelCase",
                "Programming",
                "Java / C# / TypeScript style: PascalCase types (messages, nodes), camelCase fields (signals).",
            )
        },
        Convention {
            node: Rule::new(ScreamingSnake, Some(32)).keywords().strict(),
            message: Rule::new(ScreamingSnake, Some(32)).keywords().strict(),
            signal: Rule::new(ScreamingSnake, Some(32)).keywords().strict(),
            value_table: Rule::new(ScreamingSnake, Some(32)).keywords().strict(),
            env_var: Rule::new(ScreamingSnake, Some(32)).keywords().strict(),
            enum_label: Some(Rule::new(ScreamingSnake, Some(32)).keywords().strict()),
            ..conv(
                "macro",
                "SCREAMING_SNAKE",
                "Programming",
                "Embedded C macro style: everything upper case with underscores.",
            )
        },
        Convention {
            message: Rule::new(PascalSnake, Some(32)).prefix(Prefix::Transmitter),
            signal: Rule::new(PascalSnake, Some(32)).prefix(Prefix::Transmitter),
            node: Rule::new(ScreamingSnake, Some(8)),
            ..conv(
                "prefixed",
                "Node-prefixed",
                "Automotive",
                "Every message and signal starts with its transmitter (ECU_Engine_Speed) so ownership is obvious in logs.",
            )
        },
    ]
}

pub fn preset(id: &str) -> Option<Convention> {
    presets().into_iter().find(|c| c.id == id)
}

/// The convention stored in the file, if any (preset id or custom JSON).
pub fn from_dbc(dbc: &Dbc) -> Option<Convention> {
    let v = dbc.network_attribute(CONVENTION_ATTR)?.as_str()?.to_string();
    preset(&v).or_else(|| serde_json::from_str(&v).ok())
}

/// Encode a convention for storage in the DBC: preset id, or JSON for a custom one.
pub fn to_attr_string(conv: &Convention) -> String {
    match preset(&conv.id) {
        Some(p) if &p == conv => conv.id.clone(),
        _ => serde_json::to_string(conv).unwrap_or_default(),
    }
}

const KEYWORDS: &[&str] = &[
    // C / C++
    "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else", "enum", "extern",
    "float", "for", "goto", "if", "inline", "int", "long", "register", "restrict", "return", "short", "signed",
    "sizeof", "static", "struct", "switch", "typedef", "union", "unsigned", "void", "volatile", "while", "bool",
    "class", "delete", "new", "private", "protected", "public", "template", "this", "throw", "try", "catch",
    // Rust
    "as", "async", "await", "crate", "dyn", "fn", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
    "pub", "ref", "self", "Self", "super", "trait", "type", "unsafe", "use", "where", "yield",
    // Python
    "and", "assert", "def", "del", "elif", "except", "finally", "from", "global", "import", "is", "lambda",
    "nonlocal", "not", "or", "pass", "raise", "with", "None", "True", "False",
];

pub fn is_keyword(s: &str) -> bool {
    KEYWORDS.contains(&s)
}

/// A valid DBC identifier: `[A-Za-z_][A-Za-z0-9_]*`.
pub fn is_identifier(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_') && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Split an identifier into words: `ABSWheelSpeed_FL2` → `ABS`, `Wheel`, `Speed`, `FL2`.
pub fn split_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    for chunk in s.split(|c: char| !c.is_ascii_alphanumeric()) {
        let cs: Vec<char> = chunk.chars().collect();
        let mut cur = String::new();
        for i in 0..cs.len() {
            let c = cs[i];
            if !cur.is_empty() {
                let prev = cs[i - 1];
                let next = cs.get(i + 1).copied();
                let boundary = (prev.is_ascii_lowercase() && c.is_ascii_uppercase())
                    || (prev.is_ascii_uppercase() && c.is_ascii_uppercase() && next.is_some_and(|n| n.is_ascii_lowercase()))
                    || (prev.is_ascii_digit() && c.is_ascii_uppercase() && next.is_some_and(|n| n.is_ascii_lowercase()));
                if boundary {
                    words.push(std::mem::take(&mut cur));
                }
            }
            cur.push(c);
        }
        if !cur.is_empty() {
            words.push(cur);
        }
    }
    words
}

fn capitalise(w: &str, keep_acronyms: bool) -> String {
    let all_upper = w.chars().any(|c| c.is_ascii_alphabetic()) && !w.chars().any(|c| c.is_ascii_lowercase());
    if keep_acronyms && all_upper && w.chars().filter(|c| c.is_ascii_alphabetic()).count() <= 3 {
        return w.to_string();
    }
    let mut c = w.chars();
    match c.next() {
        Some(f) => f.to_ascii_uppercase().to_string() + &c.as_str().to_ascii_lowercase(),
        None => String::new(),
    }
}

/// Convert a name to a case style.
pub fn to_case(s: &str, case: Case) -> String {
    let words = split_words(s);
    if words.is_empty() {
        return s.to_string();
    }
    // Keep short acronyms (ABS, FL, RPM) unless the whole name was upper case.
    let source_upper = !s.chars().any(|c| c.is_ascii_lowercase());
    let keep = !source_upper || words.len() == 1;
    let out = match case {
        Case::Any => return sanitize(s),
        Case::Pascal => words.iter().map(|w| capitalise(w, keep)).collect::<String>(),
        Case::Camel => {
            let mut out = words[0].to_ascii_lowercase();
            for w in &words[1..] {
                out.push_str(&capitalise(w, keep));
            }
            out
        }
        Case::Snake => words.iter().map(|w| w.to_ascii_lowercase()).collect::<Vec<_>>().join("_"),
        Case::ScreamingSnake => words.iter().map(|w| w.to_ascii_uppercase()).collect::<Vec<_>>().join("_"),
        Case::PascalSnake => words.iter().map(|w| capitalise(w, keep)).collect::<Vec<_>>().join("_"),
    };
    sanitize(&out)
}

/// Make any string a valid identifier.
pub fn sanitize(s: &str) -> String {
    let mut out: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if out.is_empty() {
        out.push('_');
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// Common automotive abbreviations, used to shorten names that break a length limit.
pub const ABBREVIATIONS: &[(&str, &str)] = &[
    ("Acceleration", "Accel"),
    ("Accelerator", "Accel"),
    ("Actual", "Act"),
    ("Ambient", "Amb"),
    ("Angle", "Ang"),
    ("Average", "Avg"),
    ("Battery", "Batt"),
    ("Brake", "Brk"),
    ("Calibration", "Cal"),
    ("Charge", "Chg"),
    ("Checksum", "Chks"),
    ("Command", "Cmd"),
    ("Configuration", "Cfg"),
    ("Control", "Ctrl"),
    ("Controller", "Ctrl"),
    ("Coolant", "Clnt"),
    ("Counter", "Cntr"),
    ("Current", "Curr"),
    ("Desired", "Des"),
    ("Diagnostic", "Diag"),
    ("Differential", "Diff"),
    ("Discharge", "Dchg"),
    ("Driver", "Drv"),
    ("Engine", "Eng"),
    ("Error", "Err"),
    ("Exhaust", "Exh"),
    ("Frequency", "Freq"),
    ("Front", "Frt"),
    ("Indicator", "Ind"),
    ("Information", "Info"),
    ("Lateral", "Lat"),
    ("Left", "Lh"),
    ("Limit", "Lim"),
    ("Longitudinal", "Long"),
    ("Maximum", "Max"),
    ("Message", "Msg"),
    ("Minimum", "Min"),
    ("Module", "Mod"),
    ("Number", "Num"),
    ("Position", "Pos"),
    ("Power", "Pwr"),
    ("Pressure", "Press"),
    ("Rear", "Rr"),
    ("Reference", "Ref"),
    ("Request", "Req"),
    ("Response", "Resp"),
    ("Right", "Rh"),
    ("Rotation", "Rot"),
    ("Sensor", "Sens"),
    ("Signal", "Sig"),
    ("Speed", "Spd"),
    ("Status", "Sts"),
    ("Steering", "Steer"),
    ("Supply", "Sup"),
    ("Suspension", "Susp"),
    ("Switch", "Sw"),
    ("System", "Sys"),
    ("Target", "Tgt"),
    ("Temperature", "Temp"),
    ("Throttle", "Thr"),
    ("Torque", "Trq"),
    ("Transmission", "Trans"),
    ("Value", "Val"),
    ("Vehicle", "Veh"),
    ("Velocity", "Vel"),
    ("Vertical", "Vert"),
    ("Voltage", "Volt"),
    ("Warning", "Warn"),
    ("Wheel", "Whl"),
];

/// Shorten a name to `max` characters: abbreviate words first, then truncate.
pub fn shorten(name: &str, max: usize, case: Case) -> String {
    if name.len() <= max {
        return name.to_string();
    }
    let mut words = split_words(name);
    for w in words.iter_mut() {
        if let Some((_, short)) = ABBREVIATIONS.iter().find(|(long, _)| long.eq_ignore_ascii_case(w)) {
            *w = short.to_string();
        }
    }
    let joined = if matches!(case, Case::Pascal | Case::Camel) { words.concat() } else { words.join("_") };
    let mut out = to_case(&joined, case);
    if out.len() > max {
        out.truncate(max);
        while out.ends_with('_') {
            out.pop();
        }
    }
    out
}

/// Normalise wheel-corner designators to a trailing FL/FR/RL/RR.
pub fn fix_corners(name: &str) -> String {
    let words = split_words(name);
    let lower: Vec<String> = words.iter().map(|w| w.to_ascii_lowercase()).collect();
    let single = |w: &str| -> Option<&'static str> {
        match w {
            "fl" | "lf" | "frontleft" | "leftfront" => Some("FL"),
            "fr" | "rf" | "frontright" | "rightfront" => Some("FR"),
            "rl" | "lr" | "rearleft" | "leftrear" => Some("RL"),
            "rr" | "rearright" | "rightrear" => Some("RR"),
            _ => None,
        }
    };
    let pair = |a: &str, b: &str| -> Option<&'static str> {
        match (a, b) {
            ("front", "left") | ("left", "front") => Some("FL"),
            ("front", "right") | ("right", "front") => Some("FR"),
            ("rear", "left") | ("left", "rear") => Some("RL"),
            ("rear", "right") | ("right", "rear") => Some("RR"),
            _ => None,
        }
    };
    let mut corner = None;
    let mut keep: Vec<String> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if corner.is_none() {
            if i + 1 < words.len()
                && let Some(c) = pair(&lower[i], &lower[i + 1])
            {
                corner = Some(c);
                i += 2;
                continue;
            }
            if let Some(c) = single(&lower[i]) {
                corner = Some(c);
                i += 1;
                continue;
            }
        }
        keep.push(words[i].clone());
        i += 1;
    }
    match corner {
        None => name.to_string(),
        Some(c) => {
            let sep = if name.contains('_') || keep.is_empty() { "_" } else { "" };
            let base = if name.contains('_') { keep.join("_") } else { keep.concat() };
            if base.is_empty() { c.to_string() } else { format!("{base}{sep}{c}") }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamingIssue {
    pub target: Target,
    pub entity: Entity,
    pub name: String,
    pub reasons: Vec<String>,
    pub suggestion: String,
}

/// Context needed for prefix rules.
pub struct Ctx<'a> {
    pub transmitter: Option<&'a str>,
    pub message: Option<&'a str>,
}

fn required_prefix(rule: &Rule, ctx: &Ctx) -> Option<String> {
    match &rule.prefix {
        Prefix::None => None,
        Prefix::Transmitter => ctx.transmitter.filter(|t| *t != crate::model::NO_NODE).map(|t| format!("{t}_")),
        Prefix::MessageName => ctx.message.map(|m| format!("{m}_")),
        Prefix::Literal(p) => Some(p.clone()),
    }
}

/// Check one name. Returns the reasons it fails and a suggested replacement.
pub fn check_name(rule: &Rule, name: &str, ctx: &Ctx, corners: bool) -> Option<(Vec<String>, String)> {
    let mut reasons = Vec::new();
    let prefix = required_prefix(rule, ctx);
    let body = match &prefix {
        Some(p) if name.starts_with(p.as_str()) => &name[p.len()..],
        Some(p) => {
            reasons.push(format!("should start with '{p}'"));
            name
        }
        None => name,
    };
    if !is_identifier(name) {
        reasons.push("contains characters DBC does not allow".into());
    }
    if !rule.case.matches(body) && rule.case != Case::Any {
        reasons.push(format!("should be {}", rule.case.label()));
    }
    if let Some(max) = rule.max_len
        && name.len() > max
    {
        reasons.push(format!("longer than {max} characters ({})", name.len()));
    }
    if rule.no_double_underscore && name.contains("__") {
        reasons.push("contains '__'".into());
    }
    if rule.no_trailing_underscore && (name.ends_with('_') || name.starts_with('_')) {
        reasons.push("starts or ends with '_'".into());
    }
    if rule.forbid_keywords && is_keyword(name) {
        reasons.push("is a programming-language keyword".into());
    }
    let cornered = if corners { fix_corners(body) } else { body.to_string() };
    if corners && cornered != body {
        reasons.push("use a standard FL/FR/RL/RR corner suffix".into());
    }
    if reasons.is_empty() {
        return None;
    }
    // Build the suggestion.
    let mut s = to_case(&cornered, rule.case);
    if rule.no_double_underscore {
        while s.contains("__") {
            s = s.replace("__", "_");
        }
    }
    if rule.no_trailing_underscore {
        s = s.trim_matches('_').to_string();
        if s.is_empty() || s.starts_with(|c: char| c.is_ascii_digit()) {
            s.insert(0, 'X');
        }
    }
    if let Some(p) = &prefix {
        s = format!("{p}{s}");
    }
    if let Some(max) = rule.max_len {
        s = match &prefix {
            Some(p) if p.len() < max => format!("{p}{}", shorten(&s[p.len()..], max - p.len(), rule.case)),
            _ => shorten(&s, max, rule.case),
        };
    }
    if rule.forbid_keywords && is_keyword(&s) {
        s.push('_');
        if rule.no_trailing_underscore {
            s.pop();
            s.push_str("Val");
        }
    }
    Some((reasons, s))
}

fn msg_ctx(m: &Message) -> Ctx<'_> {
    Ctx { transmitter: Some(m.transmitter.as_str()), message: Some(m.name.as_str()) }
}

/// Check every name in a file against a convention.
pub fn check_dbc(dbc: &Dbc, conv: &Convention) -> Vec<NamingIssue> {
    let mut out = Vec::new();
    let none = Ctx { transmitter: None, message: None };
    fn push(out: &mut Vec<NamingIssue>, target: Target, entity: Entity, name: &str, r: Option<(Vec<String>, String)>) {
        if let Some((reasons, suggestion)) = r {
            out.push(NamingIssue { target, entity, name: name.to_string(), reasons, suggestion });
        }
    }
    for n in &dbc.nodes {
        push(&mut out, Target::Node(n.name.clone()), Entity::Node, &n.name, check_name(&conv.node, &n.name, &none, false));
    }
    for m in &dbc.messages {
        if m.is_pseudo() {
            continue;
        }
        let ctx = msg_ctx(m);
        push(&mut out, Target::Message(m.id), Entity::Message, &m.name, check_name(&conv.message, &m.name, &ctx, false));
        for s in &m.signals {
            let t = Target::Signal(m.id, s.name.clone());
            push(&mut out, t, Entity::Signal, &s.name, check_name(&conv.signal, &s.name, &ctx, conv.corner_suffixes));
            if let Some(rule) = &conv.enum_label {
                for e in &s.value_descriptions {
                    let t = Target::ValueLabel(m.id, s.name.clone(), e.value);
                    push(&mut out, t, Entity::EnumLabel, &e.label, check_name(rule, &e.label, &none, false));
                }
            }
        }
    }
    for v in &dbc.value_tables {
        let t = Target::ValueTable(v.name.clone());
        push(&mut out, t, Entity::ValueTable, &v.name, check_name(&conv.value_table, &v.name, &none, false));
    }
    for e in &dbc.env_vars {
        let t = Target::EnvVar(e.name.clone());
        push(&mut out, t, Entity::EnvVar, &e.name, check_name(&conv.env_var, &e.name, &none, false));
    }
    dedupe_suggestions(dbc, &mut out);
    out
}

/// Avoid suggesting a name that collides with an existing sibling or another suggestion.
fn dedupe_suggestions(dbc: &Dbc, issues: &mut [NamingIssue]) {
    use std::collections::HashSet;
    let mut taken: HashSet<String> = HashSet::new();
    for n in &dbc.nodes {
        taken.insert(format!("n:{}", n.name));
    }
    for m in &dbc.messages {
        taken.insert(format!("m:{}", m.name));
        for s in &m.signals {
            taken.insert(format!("s{}:{}", m.id, s.name));
        }
    }
    for i in issues.iter_mut() {
        let scope = match &i.target {
            Target::Node(_) => "n:".to_string(),
            Target::Message(_) => "m:".to_string(),
            Target::Signal(id, _) => format!("s{id}:"),
            _ => continue,
        };
        taken.remove(&format!("{scope}{}", i.name));
        let mut candidate = i.suggestion.clone();
        let mut k = 2;
        while taken.contains(&format!("{scope}{candidate}")) {
            candidate = format!("{}{}{k}", i.suggestion, if i.suggestion.contains('_') { "_" } else { "" });
            k += 1;
        }
        i.suggestion = candidate.clone();
        taken.insert(format!("{scope}{candidate}"));
    }
}
