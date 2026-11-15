//! The in-memory model of a DBC file.
//!
//! Comments, attributes and value descriptions are attached to the object they describe, so
//! editing one object never needs to touch a separate table. Statements CANdy does not model are
//! kept verbatim in [`Dbc::extra`] so a load/save cycle never loses information.

/// Bit 31 of a raw DBC message id marks a 29-bit (extended) identifier.
pub const EXTENDED_FLAG: u32 = 0x8000_0000;

/// Placeholder node name used by Vector tools when a message or signal has no real node.
pub const NO_NODE: &str = "Vector__XXX";

/// Pseudo message Vector tools use to hold signals that belong to no message.
pub const INDEPENDENT_SIG_MSG: u32 = 0xC000_0000;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dbc {
    pub version: String,
    pub new_symbols: Vec<String>,
    /// Raw text after `BS_:` (almost always empty).
    pub bit_timing: String,
    pub nodes: Vec<Node>,
    pub value_tables: Vec<ValueTable>,
    pub messages: Vec<Message>,
    pub env_vars: Vec<EnvVar>,
    /// Network-level `CM_ "..."`.
    pub comment: Option<String>,
    pub attr_defs: Vec<AttrDef>,
    pub attr_defaults: Vec<AttrDefault>,
    /// Network-level `BA_ "name" value;`.
    pub attributes: Vec<Attribute>,
    pub signal_groups: Vec<SignalGroup>,
    /// Statements CANdy does not model, kept verbatim (without the trailing newline).
    pub extra: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Node {
    pub name: String,
    pub comment: Option<String>,
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ValueTable {
    pub name: String,
    pub entries: Vec<ValueEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValueEntry {
    pub value: i64,
    pub label: String,
}

impl ValueEntry {
    pub fn new(value: i64, label: impl Into<String>) -> Self {
        Self {
            value,
            label: label.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// Raw DBC id: bit 31 set means extended (29-bit).
    pub id: u32,
    pub name: String,
    /// Payload length in bytes (DLC 0–8 classic, up to 64 for CAN FD).
    pub size: u32,
    pub transmitter: String,
    pub signals: Vec<Signal>,
    pub comment: Option<String>,
    pub attributes: Vec<Attribute>,
    /// Extra transmitters from `BO_TX_BU_`.
    pub extra_transmitters: Vec<String>,
}

impl Default for Message {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            size: 8,
            transmitter: NO_NODE.into(),
            signals: Vec::new(),
            comment: None,
            attributes: Vec::new(),
            extra_transmitters: Vec::new(),
        }
    }
}

impl Message {
    pub fn is_extended(&self) -> bool {
        self.id & EXTENDED_FLAG != 0
    }

    /// The identifier as it appears on the bus (without the extended flag).
    pub fn can_id(&self) -> u32 {
        self.id & !EXTENDED_FLAG
    }

    pub fn set_can_id(&mut self, can_id: u32, extended: bool) {
        self.id = if extended {
            (can_id & 0x1FFF_FFFF) | EXTENDED_FLAG
        } else {
            can_id & 0x7FF
        };
    }

    pub fn is_pseudo(&self) -> bool {
        self.id == INDEPENDENT_SIG_MSG
    }

    pub fn signal(&self, name: &str) -> Option<&Signal> {
        self.signals.iter().find(|s| s.name == name)
    }

    pub fn signal_mut(&mut self, name: &str) -> Option<&mut Signal> {
        self.signals.iter_mut().find(|s| s.name == name)
    }

    pub fn attribute(&self, name: &str) -> Option<&AttrValue> {
        self.attributes
            .iter()
            .find(|a| a.name == name)
            .map(|a| &a.value)
    }

    /// The (first) multiplexor switch signal, if any.
    pub fn multiplexor(&self) -> Option<&Signal> {
        self.signals
            .iter()
            .find(|s| matches!(s.mux, Mux::Multiplexor))
    }

    pub fn is_multiplexed(&self) -> bool {
        self.signals.iter().any(|s| s.mux != Mux::None)
    }

    /// Hex id formatted the way engineers write it (`0x1A0`, `0x18FEF100x`).
    pub fn id_hex(&self) -> String {
        if self.is_extended() {
            format!("0x{:08X}", self.can_id())
        } else {
            format!("0x{:03X}", self.can_id())
        }
    }

    pub fn j1939(&self) -> Option<J1939Id> {
        self.is_extended()
            .then(|| J1939Id::from_can_id(self.can_id()))
    }
}

/// J1939 breakdown of a 29-bit identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct J1939Id {
    pub priority: u8,
    pub pgn: u32,
    pub source: u8,
    /// Destination address for PDU1 (peer-to-peer) frames.
    pub destination: Option<u8>,
}

impl J1939Id {
    pub fn from_can_id(id: u32) -> Self {
        let priority = ((id >> 26) & 0x7) as u8;
        let pf = (id >> 16) & 0xFF;
        let ps = ((id >> 8) & 0xFF) as u8;
        let dp_edp = (id >> 24) & 0x3;
        let source = (id & 0xFF) as u8;
        if pf < 240 {
            Self {
                priority,
                pgn: (dp_edp << 16) | (pf << 8),
                source,
                destination: Some(ps),
            }
        } else {
            Self {
                priority,
                pgn: (dp_edp << 16) | (pf << 8) | ps as u32,
                source,
                destination: None,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ByteOrder {
    /// Intel (`@1`): start bit is the least significant bit.
    #[default]
    LittleEndian,
    /// Motorola (`@0`): start bit is the most significant bit.
    BigEndian,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValueType {
    #[default]
    Unsigned,
    Signed,
    /// IEEE float (`SIG_VALTYPE_ … 1`), always 32 bits.
    Float32,
    /// IEEE double (`SIG_VALTYPE_ … 2`), always 64 bits.
    Float64,
}

impl ValueType {
    pub fn is_signed(self) -> bool {
        !matches!(self, ValueType::Unsigned)
    }
    pub fn is_float(self) -> bool {
        matches!(self, ValueType::Float32 | ValueType::Float64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mux {
    #[default]
    None,
    /// `M`: this signal selects which multiplexed signals are present.
    Multiplexor,
    /// `mN`: present when the switch equals N.
    Multiplexed(u64),
    /// `mNM`: present when its parent switch equals N, and itself a switch (nested mux).
    MultiplexedMultiplexor(u64),
}

impl Mux {
    pub fn is_switch(self) -> bool {
        matches!(self, Mux::Multiplexor | Mux::MultiplexedMultiplexor(_))
    }
    pub fn selector(self) -> Option<u64> {
        match self {
            Mux::Multiplexed(v) | Mux::MultiplexedMultiplexor(v) => Some(v),
            _ => None,
        }
    }
    pub fn token(self) -> String {
        match self {
            Mux::None => String::new(),
            Mux::Multiplexor => "M".into(),
            Mux::Multiplexed(v) => format!("m{v}"),
            Mux::MultiplexedMultiplexor(v) => format!("m{v}M"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    pub name: String,
    pub mux: Mux,
    pub start_bit: u32,
    pub size: u32,
    pub byte_order: ByteOrder,
    pub value_type: ValueType,
    pub factor: f64,
    pub offset: f64,
    pub min: f64,
    pub max: f64,
    pub unit: String,
    pub receivers: Vec<String>,
    pub comment: Option<String>,
    pub attributes: Vec<Attribute>,
    /// `VAL_` entries for this signal.
    pub value_descriptions: Vec<ValueEntry>,
    /// `SG_MUL_VAL_` extended multiplexing entries.
    pub ext_mux: Vec<ExtMux>,
}

impl Default for Signal {
    fn default() -> Self {
        Self {
            name: String::new(),
            mux: Mux::None,
            start_bit: 0,
            size: 8,
            byte_order: ByteOrder::LittleEndian,
            value_type: ValueType::Unsigned,
            factor: 1.0,
            offset: 0.0,
            min: 0.0,
            max: 0.0,
            unit: String::new(),
            receivers: vec![NO_NODE.into()],
            comment: None,
            attributes: Vec::new(),
            value_descriptions: Vec::new(),
            ext_mux: Vec::new(),
        }
    }
}

impl Signal {
    /// Smallest and largest raw integer value representable by this signal.
    pub fn raw_range(&self) -> (f64, f64) {
        match self.value_type {
            ValueType::Float32 => (f32::MIN as f64, f32::MAX as f64),
            ValueType::Float64 => (f64::MIN, f64::MAX),
            ValueType::Signed => {
                let n = self.size.clamp(1, 64) as i32;
                (-(2f64.powi(n - 1)), 2f64.powi(n - 1) - 1.0)
            }
            ValueType::Unsigned => (0.0, 2f64.powi(self.size.clamp(1, 64) as i32) - 1.0),
        }
    }

    /// Physical range the raw range maps onto with this factor and offset.
    pub fn physical_range(&self) -> (f64, f64) {
        let (lo, hi) = self.raw_range();
        let a = lo * self.factor + self.offset;
        let b = hi * self.factor + self.offset;
        if a <= b { (a, b) } else { (b, a) }
    }

    pub fn to_physical(&self, raw: f64) -> f64 {
        raw * self.factor + self.offset
    }

    pub fn to_raw(&self, physical: f64) -> f64 {
        if self.factor == 0.0 {
            0.0
        } else {
            (physical - self.offset) / self.factor
        }
    }

    pub fn value_label(&self, raw: i64) -> Option<&str> {
        self.value_descriptions
            .iter()
            .find(|e| e.value == raw)
            .map(|e| e.label.as_str())
    }

    pub fn attribute(&self, name: &str) -> Option<&AttrValue> {
        self.attributes
            .iter()
            .find(|a| a.name == name)
            .map(|a| &a.value)
    }

    /// Human-readable scaling formula, e.g. `phys = raw × 0.1 − 40`.
    pub fn formula(&self) -> String {
        let mut s = String::from("raw");
        if self.factor != 1.0 {
            s.push_str(&format!(" × {}", fmt_num(self.factor)));
        }
        if self.offset > 0.0 {
            s.push_str(&format!(" + {}", fmt_num(self.offset)));
        } else if self.offset < 0.0 {
            s.push_str(&format!(" − {}", fmt_num(-self.offset)));
        }
        s
    }
}

/// One `SG_MUL_VAL_` entry: the signal is present when `switch` is in any of `ranges`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtMux {
    pub switch: String,
    pub ranges: Vec<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EnvVar {
    pub name: String,
    /// 0 integer, 1 float, 2 string.
    pub var_type: u8,
    pub min: f64,
    pub max: f64,
    pub unit: String,
    pub initial: f64,
    pub ev_id: u32,
    pub access: String,
    pub nodes: Vec<String>,
    pub comment: Option<String>,
    pub attributes: Vec<Attribute>,
    pub value_descriptions: Vec<ValueEntry>,
    /// `ENVVAR_DATA_` size, when declared.
    pub data_size: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttrObject {
    Network,
    Node,
    Message,
    Signal,
    EnvVar,
    /// `BU_BO_REL_`
    NodeMessage,
    /// `BU_SG_REL_`
    NodeSignal,
    /// `BU_EV_REL_`
    NodeEnvVar,
}

impl AttrObject {
    pub fn keyword(self) -> &'static str {
        match self {
            AttrObject::Network => "",
            AttrObject::Node => "BU_",
            AttrObject::Message => "BO_",
            AttrObject::Signal => "SG_",
            AttrObject::EnvVar => "EV_",
            AttrObject::NodeMessage => "BU_BO_REL_",
            AttrObject::NodeSignal => "BU_SG_REL_",
            AttrObject::NodeEnvVar => "BU_EV_REL_",
        }
    }
    pub fn is_relation(self) -> bool {
        matches!(
            self,
            AttrObject::NodeMessage | AttrObject::NodeSignal | AttrObject::NodeEnvVar
        )
    }
    pub fn label(self) -> &'static str {
        match self {
            AttrObject::Network => "Network",
            AttrObject::Node => "Node",
            AttrObject::Message => "Message",
            AttrObject::Signal => "Signal",
            AttrObject::EnvVar => "Env. variable",
            AttrObject::NodeMessage => "Node ↔ Message",
            AttrObject::NodeSignal => "Node ↔ Signal",
            AttrObject::NodeEnvVar => "Node ↔ Env. var",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttrKind {
    Int { min: i64, max: i64 },
    Hex { min: i64, max: i64 },
    Float { min: f64, max: f64 },
    String,
    Enum(Vec<String>),
}

impl AttrKind {
    pub fn label(&self) -> &'static str {
        match self {
            AttrKind::Int { .. } => "INT",
            AttrKind::Hex { .. } => "HEX",
            AttrKind::Float { .. } => "FLOAT",
            AttrKind::String => "STRING",
            AttrKind::Enum(_) => "ENUM",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttrDef {
    pub object: AttrObject,
    pub name: String,
    pub kind: AttrKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttrDefault {
    pub name: String,
    pub value: AttrValue,
    /// Declared with `BA_DEF_DEF_REL_`.
    pub relation: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttrValue {
    Int(i64),
    Float(f64),
    Str(String),
}

impl AttrValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            AttrValue::Int(i) => Some(*i as f64),
            AttrValue::Float(f) => Some(*f),
            AttrValue::Str(s) => s.trim().parse().ok(),
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            AttrValue::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn display(&self) -> String {
        match self {
            AttrValue::Int(i) => i.to_string(),
            AttrValue::Float(f) => fmt_num(*f),
            AttrValue::Str(s) => s.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: String,
    pub value: AttrValue,
}

impl Attribute {
    pub fn new(name: impl Into<String>, value: AttrValue) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignalGroup {
    pub message_id: u32,
    pub name: String,
    pub repetitions: u32,
    pub signals: Vec<String>,
}

/// Shortest round-trip formatting for numbers in DBC output and the UI.
pub fn fmt_num(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{v}");
    if s.len() > 18 { format!("{v:e}") } else { s }
}

impl Dbc {
    pub fn new() -> Self {
        Self {
            new_symbols: crate::writer::DEFAULT_NEW_SYMBOLS
                .iter()
                .map(|s| s.to_string())
                .collect(),
            ..Default::default()
        }
    }

    pub fn message(&self, id: u32) -> Option<&Message> {
        self.messages.iter().find(|m| m.id == id)
    }

    pub fn message_mut(&mut self, id: u32) -> Option<&mut Message> {
        self.messages.iter_mut().find(|m| m.id == id)
    }

    pub fn message_by_name(&self, name: &str) -> Option<&Message> {
        self.messages.iter().find(|m| m.name == name)
    }

    pub fn node(&self, name: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.name == name)
    }

    pub fn attr_def(&self, name: &str) -> Option<&AttrDef> {
        self.attr_defs.iter().find(|d| d.name == name)
    }

    pub fn attr_default(&self, name: &str) -> Option<&AttrValue> {
        self.attr_defaults
            .iter()
            .find(|d| d.name == name)
            .map(|d| &d.value)
    }

    pub fn network_attribute(&self, name: &str) -> Option<&AttrValue> {
        self.attributes
            .iter()
            .find(|a| a.name == name)
            .map(|a| &a.value)
    }

    /// Effective attribute on a message: explicit value, else the declared default.
    pub fn message_attr(&self, msg: &Message, name: &str) -> Option<AttrValue> {
        msg.attribute(name)
            .cloned()
            .or_else(|| self.attr_default(name).cloned())
    }

    /// Cycle time in ms from `GenMsgCycleTime`, if set and non-zero.
    pub fn cycle_time_ms(&self, msg: &Message) -> Option<f64> {
        self.message_attr(msg, "GenMsgCycleTime")
            .and_then(|v| v.as_f64())
            .filter(|v| *v > 0.0)
    }

    /// Resolve an enum attribute value (stored as an index) to its label.
    pub fn enum_label(&self, attr: &str, value: &AttrValue) -> Option<String> {
        let def = self.attr_def(attr)?;
        if let AttrKind::Enum(options) = &def.kind {
            match value {
                AttrValue::Int(i) => options.get(*i as usize).cloned(),
                AttrValue::Str(s) => Some(s.clone()),
                AttrValue::Float(f) => options.get(*f as usize).cloned(),
            }
        } else {
            None
        }
    }

    /// All node names referenced anywhere (declared or not).
    pub fn signal_count(&self) -> usize {
        self.messages.iter().map(|m| m.signals.len()).sum()
    }

    /// Next unused standard id at or above `from`.
    pub fn free_id(&self, from: u32, extended: bool) -> u32 {
        let flag = if extended { EXTENDED_FLAG } else { 0 };
        let max = if extended { 0x1FFF_FFFF } else { 0x7FF };
        let mut id = from.min(max);
        while self.message(id | flag).is_some() && id < max {
            id += 1;
        }
        id | flag
    }

    /// Rename a node everywhere it is referenced.
    pub fn rename_node(&mut self, old: &str, new: &str) {
        for n in &mut self.nodes {
            if n.name == old {
                n.name = new.into();
            }
        }
        for m in &mut self.messages {
            if m.transmitter == old {
                m.transmitter = new.into();
            }
            for t in &mut m.extra_transmitters {
                if t == old {
                    *t = new.into();
                }
            }
            for s in &mut m.signals {
                for r in &mut s.receivers {
                    if r == old {
                        *r = new.into();
                    }
                }
            }
        }
        for e in &mut self.env_vars {
            for n in &mut e.nodes {
                if n == old {
                    *n = new.into();
                }
            }
        }
    }

    /// Rename a signal inside a message, updating groups and extended-mux references.
    pub fn rename_signal(&mut self, msg_id: u32, old: &str, new: &str) {
        if let Some(m) = self.message_mut(msg_id) {
            for s in &mut m.signals {
                if s.name == old {
                    s.name = new.into();
                }
                for e in &mut s.ext_mux {
                    if e.switch == old {
                        e.switch = new.into();
                    }
                }
            }
        }
        for g in &mut self.signal_groups {
            if g.message_id == msg_id {
                for s in &mut g.signals {
                    if s == old {
                        *s = new.into();
                    }
                }
            }
        }
    }

    /// Change a message id, updating signal groups.
    pub fn change_message_id(&mut self, old: u32, new: u32) {
        if let Some(m) = self.message_mut(old) {
            m.id = new;
        }
        for g in &mut self.signal_groups {
            if g.message_id == old {
                g.message_id = new;
            }
        }
    }

    pub fn remove_message(&mut self, id: u32) {
        self.messages.retain(|m| m.id != id);
        self.signal_groups.retain(|g| g.message_id != id);
    }

    pub fn remove_signal(&mut self, msg_id: u32, name: &str) {
        if let Some(m) = self.message_mut(msg_id) {
            m.signals.retain(|s| s.name != name);
        }
        for g in &mut self.signal_groups {
            if g.message_id == msg_id {
                g.signals.retain(|s| s != name);
            }
        }
    }

    /// Ensure an attribute definition exists (used for CANdy's own metadata attributes).
    pub fn ensure_attr_def(
        &mut self,
        object: AttrObject,
        name: &str,
        kind: AttrKind,
        default: AttrValue,
    ) {
        if self.attr_def(name).is_none() {
            self.attr_defs.push(AttrDef {
                object,
                name: name.into(),
                kind,
            });
            self.attr_defaults.push(AttrDefault {
                name: name.into(),
                value: default,
                relation: false,
            });
        }
    }
}

/// Set or replace an attribute in a list.
pub fn set_attribute(list: &mut Vec<Attribute>, name: &str, value: Option<AttrValue>) {
    match value {
        Some(v) => {
            if let Some(a) = list.iter_mut().find(|a| a.name == name) {
                a.value = v;
            } else {
                list.push(Attribute::new(name, v));
            }
        }
        None => list.retain(|a| a.name != name),
    }
}
