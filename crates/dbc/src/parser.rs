//! Error-tolerant recursive-descent DBC parser.
//!
//! The parser works directly on characters so it can be context-sensitive where the DBC grammar
//! is (`@1+` versus `-40`, receivers that end at the end of a line, the indented `NS_` block).
//! A statement that fails to parse is recorded as an error and skipped; the rest of the file
//! still loads.

use crate::model::*;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}:{}: {}", self.line, self.col, self.message)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParseOutput {
    pub dbc: Dbc,
    pub errors: Vec<ParseError>,
}

/// Parse DBC text, collecting errors instead of stopping at the first one.
pub fn parse(src: &str) -> ParseOutput {
    let mut p = Parser::new(src);
    p.run();
    ParseOutput {
        dbc: p.dbc,
        errors: p.errors,
    }
}

/// Parse DBC text, failing on the first error.
pub fn parse_strict(src: &str) -> Result<Dbc, ParseError> {
    let out = parse(src);
    match out.errors.into_iter().next() {
        Some(e) => Err(e),
        None => Ok(out.dbc),
    }
}

type PResult<T> = Result<T, String>;

const KEYWORDS: &[&str] = &[
    "VERSION",
    "NS_",
    "BS_",
    "BU_",
    "VAL_TABLE_",
    "BO_",
    "SG_",
    "BO_TX_BU_",
    "EV_",
    "ENVVAR_DATA_",
    "CM_",
    "BA_DEF_",
    "BA_DEF_REL_",
    "BA_DEF_DEF_",
    "BA_DEF_DEF_REL_",
    "BA_",
    "BA_REL_",
    "VAL_",
    "SIG_GROUP_",
    "SIG_VALTYPE_",
    "SG_MUL_VAL_",
    "SGTYPE_",
    "SGTYPE_VAL_",
    "BA_DEF_SGTYPE_",
    "BA_SGTYPE_",
    "SIG_TYPE_REF_",
    "VAL_TABLE_",
    "SIGTYPE_VALTYPE_",
    "CAT_DEF_",
    "CAT_",
    "FILTER",
];

struct Parser<'a> {
    src: &'a str,
    chars: Vec<char>,
    pos: usize,
    dbc: Dbc,
    errors: Vec<ParseError>,
    /// Deferred `SIG_VALTYPE_` entries so they apply regardless of statement order.
    valtypes: Vec<(u32, String, u8)>,
    /// Deferred `VAL_` entries for signals (message id, signal) or env vars.
    vals: Vec<(Option<u32>, String, Vec<ValueEntry>)>,
    comments: Vec<(CommentTarget, String)>,
    attrs: Vec<(CommentTarget, Attribute)>,
    ext_mux: Vec<(u32, String, ExtMux)>,
    tx: Vec<(u32, Vec<String>)>,
    env_data: Vec<(String, u32)>,
}

#[derive(Debug, Clone)]
enum CommentTarget {
    Network,
    Node(String),
    Message(u32),
    Signal(u32, String),
    EnvVar(String),
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        let src = src.strip_prefix('\u{feff}').unwrap_or(src);
        Self {
            src,
            chars: src.chars().collect(),
            pos: 0,
            dbc: Dbc::default(),
            errors: Vec::new(),
            valtypes: Vec::new(),
            vals: Vec::new(),
            comments: Vec::new(),
            attrs: Vec::new(),
            ext_mux: Vec::new(),
            tx: Vec::new(),
            env_data: Vec::new(),
        }
    }

    fn line_col(&self, pos: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for &c in &self.chars[..pos.min(self.chars.len())] {
            if c == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    fn run(&mut self) {
        let _ = self.src;
        loop {
            self.skip_ws();
            if self.eof() {
                break;
            }
            let start = self.pos;
            let kw = self.peek_ident();
            let res = match kw.as_str() {
                "VERSION" => self.version(),
                "NS_" => self.new_symbols(),
                "BS_" => self.bit_timing(),
                "BU_" => self.nodes(),
                "VAL_TABLE_" => self.value_table(),
                "BO_" => self.message(),
                "BO_TX_BU_" => self.bo_tx_bu(),
                "EV_" => self.env_var(),
                "ENVVAR_DATA_" => self.envvar_data(),
                "CM_" => self.comment(),
                "BA_DEF_" | "BA_DEF_REL_" => self.attr_def(),
                "BA_DEF_DEF_" | "BA_DEF_DEF_REL_" => self.attr_default(),
                "BA_" => self.attribute(),
                "VAL_" => self.val(),
                "SIG_GROUP_" => self.signal_group(),
                "SIG_VALTYPE_" => self.sig_valtype(),
                "SG_MUL_VAL_" => self.sg_mul_val(),
                "SG_" => Err("signal definition outside of a message (SG_ must follow BO_)".into()),
                "" => Err(format!(
                    "unexpected character '{}'",
                    self.peek().unwrap_or(' ')
                )),
                _ => self.raw_statement(),
            };
            if let Err(message) = res {
                let (line, col) = self.line_col(self.pos.max(start));
                self.errors.push(ParseError { line, col, message });
                if self.pos == start {
                    self.pos += 1;
                }
                self.recover();
            }
        }
        self.resolve();
    }

    /// Skip to the start of the next line that begins with a known keyword.
    fn recover(&mut self) {
        while !self.eof() {
            if self.peek() == Some('\n') {
                self.pos += 1;
                let save = self.pos;
                self.skip_inline_ws();
                let id = self.peek_ident();
                if KEYWORDS.contains(&id.as_str()) && id != "SG_" {
                    self.pos = save;
                    return;
                }
            } else {
                self.pos += 1;
            }
        }
    }

    // ---- character helpers -------------------------------------------------------------------

    fn eof(&self) -> bool {
        self.pos >= self.chars.len()
    }
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn peek_at(&self, off: usize) -> Option<char> {
        self.chars.get(self.pos + off).copied()
    }

    fn skip_ws(&mut self) {
        loop {
            while matches!(self.peek(), Some(c) if c.is_whitespace()) {
                self.pos += 1;
            }
            // `//` line comments appear in some hand-written files.
            if self.peek() == Some('/') && self.peek_at(1) == Some('/') {
                while !matches!(self.peek(), None | Some('\n')) {
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
    }

    fn skip_inline_ws(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\r')) {
            self.pos += 1;
        }
    }

    fn at_line_end(&mut self) -> bool {
        self.skip_inline_ws();
        matches!(self.peek(), None | Some('\n'))
    }

    fn is_ident_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_'
    }

    fn peek_ident(&self) -> String {
        let mut i = self.pos;
        let mut s = String::new();
        while let Some(&c) = self.chars.get(i) {
            if Self::is_ident_char(c) {
                s.push(c);
                i += 1;
            } else {
                break;
            }
        }
        s
    }

    fn ident(&mut self) -> PResult<String> {
        self.skip_ws();
        let s = self.peek_ident();
        if s.is_empty() {
            return Err(format!(
                "expected identifier, found {}",
                self.describe_here()
            ));
        }
        self.pos += s.chars().count();
        Ok(s)
    }

    fn keyword(&mut self, kw: &str) -> PResult<()> {
        self.skip_ws();
        let s = self.peek_ident();
        if s == kw {
            self.pos += s.len();
            Ok(())
        } else {
            Err(format!("expected '{kw}', found {}", self.describe_here()))
        }
    }

    fn describe_here(&self) -> String {
        match self.peek() {
            None => "end of file".into(),
            Some('\n') => "end of line".into(),
            Some(c) => {
                let id = self.peek_ident();
                if id.is_empty() {
                    format!("'{c}'")
                } else {
                    format!("'{id}'")
                }
            }
        }
    }

    fn expect(&mut self, c: char) -> PResult<()> {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("expected '{c}', found {}", self.describe_here()))
        }
    }

    fn eat(&mut self, c: char) -> bool {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn eat_inline(&mut self, c: char) -> bool {
        self.skip_inline_ws();
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn number_text(&mut self) -> PResult<String> {
        self.skip_ws();
        let start = self.pos;
        if matches!(self.peek(), Some('+' | '-')) {
            self.pos += 1;
        }
        if self.peek() == Some('0') && matches!(self.peek_at(1), Some('x' | 'X')) {
            self.pos += 2;
            while matches!(self.peek(), Some(c) if c.is_ascii_hexdigit()) {
                self.pos += 1;
            }
        } else {
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.pos += 1;
            }
            if self.peek() == Some('.') {
                self.pos += 1;
                while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
            if matches!(self.peek(), Some('e' | 'E'))
                && matches!(self.peek_at(1), Some(c) if c.is_ascii_digit() || c == '+' || c == '-')
            {
                self.pos += 2;
                while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        if text.is_empty() || text == "+" || text == "-" {
            self.pos = start;
            return Err(format!("expected number, found {}", self.describe_here()));
        }
        Ok(text)
    }

    fn float(&mut self) -> PResult<f64> {
        let t = self.number_text()?;
        parse_f64(&t).ok_or_else(|| format!("invalid number '{t}'"))
    }

    fn int(&mut self) -> PResult<i64> {
        let t = self.number_text()?;
        parse_i64(&t).ok_or_else(|| format!("invalid integer '{t}'"))
    }

    fn uint(&mut self) -> PResult<u32> {
        let v = self.int()?;
        u32::try_from(v).map_err(|_| format!("value {v} out of range"))
    }

    fn string(&mut self) -> PResult<String> {
        self.skip_ws();
        if self.peek() != Some('"') {
            return Err(format!(
                "expected quoted string, found {}",
                self.describe_here()
            ));
        }
        self.pos += 1;
        let mut s = String::new();
        loop {
            match self.peek() {
                None => return Err("unterminated string".into()),
                Some('"') => {
                    self.pos += 1;
                    break;
                }
                Some('\\') => {
                    self.pos += 1;
                    match self.peek() {
                        Some(c @ ('"' | '\\')) => {
                            s.push(c);
                            self.pos += 1;
                        }
                        _ => s.push('\\'),
                    }
                }
                Some(c) => {
                    s.push(c);
                    self.pos += 1;
                }
            }
        }
        Ok(s)
    }

    fn semicolon(&mut self) -> PResult<()> {
        self.expect(';')
    }

    // ---- statements -------------------------------------------------------------------------

    fn version(&mut self) -> PResult<()> {
        self.keyword("VERSION")?;
        self.dbc.version = self.string()?;
        Ok(())
    }

    fn new_symbols(&mut self) -> PResult<()> {
        self.keyword("NS_")?;
        self.expect(':')?;
        // Symbols follow on indented lines until a line starts in column 0.
        loop {
            self.skip_inline_ws();
            if self.peek() == Some('\n') {
                self.pos += 1;
                if !matches!(self.peek(), Some(' ' | '\t' | '\r' | '\n')) {
                    // Column-zero line: end of block, unless it is blank.
                    break;
                }
                continue;
            }
            if self.eof() {
                break;
            }
            let id = self.peek_ident();
            if id.is_empty() {
                break;
            }
            self.pos += id.len();
            self.dbc.new_symbols.push(id);
        }
        Ok(())
    }

    fn bit_timing(&mut self) -> PResult<()> {
        self.keyword("BS_")?;
        self.expect(':')?;
        let start = self.pos;
        while !matches!(self.peek(), None | Some('\n')) {
            self.pos += 1;
        }
        self.dbc.bit_timing = self.chars[start..self.pos]
            .iter()
            .collect::<String>()
            .trim()
            .to_string();
        Ok(())
    }

    fn nodes(&mut self) -> PResult<()> {
        self.keyword("BU_")?;
        self.expect(':')?;
        loop {
            if self.at_line_end() {
                break;
            }
            let name = self.ident()?;
            self.eat_inline(',');
            if !self.dbc.nodes.iter().any(|n| n.name == name) {
                self.dbc.nodes.push(Node {
                    name,
                    ..Default::default()
                });
            }
        }
        Ok(())
    }

    fn value_entries(&mut self) -> PResult<Vec<ValueEntry>> {
        let mut v = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() == Some(';') {
                self.pos += 1;
                break;
            }
            let value = self.float()? as i64;
            let label = self.string()?;
            v.push(ValueEntry { value, label });
        }
        Ok(v)
    }

    fn value_table(&mut self) -> PResult<()> {
        self.keyword("VAL_TABLE_")?;
        let name = self.ident()?;
        let entries = self.value_entries()?;
        self.dbc.value_tables.push(ValueTable { name, entries });
        Ok(())
    }

    fn message(&mut self) -> PResult<()> {
        self.keyword("BO_")?;
        let id = self.int()? as u32;
        let name = self.ident()?;
        self.expect(':')?;
        let size = self.uint()?;
        let transmitter = if self.at_line_end() {
            NO_NODE.to_string()
        } else {
            self.ident()?
        };
        let mut msg = Message {
            id,
            name,
            size,
            transmitter,
            ..Default::default()
        };
        loop {
            self.skip_ws();
            if self.peek_ident() != "SG_" {
                break;
            }
            match self.signal() {
                Ok(s) => msg.signals.push(s),
                Err(e) => {
                    let (line, col) = self.line_col(self.pos);
                    self.errors.push(ParseError {
                        line,
                        col,
                        message: format!("in message {}: {e}", msg.name),
                    });
                    while !matches!(self.peek(), None | Some('\n')) {
                        self.pos += 1;
                    }
                }
            }
        }
        self.dbc.messages.push(msg);
        Ok(())
    }

    fn signal(&mut self) -> PResult<Signal> {
        self.keyword("SG_")?;
        let name = self.ident()?;
        self.skip_ws();
        let mut mux = Mux::None;
        if self.peek() != Some(':') {
            let tok = self.ident()?;
            mux =
                parse_mux(&tok).ok_or_else(|| format!("invalid multiplexer indicator '{tok}'"))?;
        }
        self.expect(':')?;
        let start_bit = self.uint()?;
        self.expect('|')?;
        let size = self.uint()?;
        self.expect('@')?;
        self.skip_ws();
        let byte_order = match self.peek() {
            Some('1') => ByteOrder::LittleEndian,
            Some('0') => ByteOrder::BigEndian,
            _ => {
                return Err(format!(
                    "expected byte order 0 or 1, found {}",
                    self.describe_here()
                ));
            }
        };
        self.pos += 1;
        let value_type = match self.peek() {
            Some('+') => ValueType::Unsigned,
            Some('-') => ValueType::Signed,
            _ => {
                return Err(format!(
                    "expected '+' or '-', found {}",
                    self.describe_here()
                ));
            }
        };
        self.pos += 1;
        self.expect('(')?;
        let factor = self.float()?;
        self.expect(',')?;
        let offset = self.float()?;
        self.expect(')')?;
        self.expect('[')?;
        let min = self.float()?;
        self.expect('|')?;
        let max = self.float()?;
        self.expect(']')?;
        let unit = self.string()?;
        let mut receivers = Vec::new();
        loop {
            if self.at_line_end() {
                break;
            }
            receivers.push(self.ident()?);
            self.eat_inline(',');
        }
        if receivers.is_empty() {
            receivers.push(NO_NODE.into());
        }
        Ok(Signal {
            name,
            mux,
            start_bit,
            size,
            byte_order,
            value_type,
            factor,
            offset,
            min,
            max,
            unit,
            receivers,
            ..Default::default()
        })
    }

    fn bo_tx_bu(&mut self) -> PResult<()> {
        self.keyword("BO_TX_BU_")?;
        let id = self.int()? as u32;
        self.expect(':')?;
        let mut nodes = Vec::new();
        loop {
            self.skip_ws();
            if self.eat(';') {
                break;
            }
            nodes.push(self.ident()?);
            self.eat(',');
        }
        self.tx.push((id, nodes));
        Ok(())
    }

    fn env_var(&mut self) -> PResult<()> {
        self.keyword("EV_")?;
        let name = self.ident()?;
        self.expect(':')?;
        let var_type = self.uint()? as u8;
        self.expect('[')?;
        let min = self.float()?;
        self.expect('|')?;
        let max = self.float()?;
        self.expect(']')?;
        let unit = self.string()?;
        let initial = self.float()?;
        let ev_id = self.uint()?;
        let access = self.ident()?;
        let mut nodes = Vec::new();
        loop {
            self.skip_ws();
            if self.eat(';') {
                break;
            }
            nodes.push(self.ident()?);
            self.eat(',');
        }
        self.dbc.env_vars.push(EnvVar {
            name,
            var_type,
            min,
            max,
            unit,
            initial,
            ev_id,
            access,
            nodes,
            ..Default::default()
        });
        Ok(())
    }

    fn envvar_data(&mut self) -> PResult<()> {
        self.keyword("ENVVAR_DATA_")?;
        let name = self.ident()?;
        self.expect(':')?;
        let size = self.uint()?;
        self.semicolon()?;
        self.env_data.push((name, size));
        Ok(())
    }

    fn target(&mut self) -> PResult<CommentTarget> {
        self.skip_ws();
        let save = self.pos;
        let id = self.peek_ident();
        Ok(match id.as_str() {
            "BU_" => {
                self.pos += 3;
                CommentTarget::Node(self.ident()?)
            }
            "BO_" => {
                self.pos += 3;
                CommentTarget::Message(self.int()? as u32)
            }
            "SG_" => {
                self.pos += 3;
                let id = self.int()? as u32;
                CommentTarget::Signal(id, self.ident()?)
            }
            "EV_" => {
                self.pos += 3;
                CommentTarget::EnvVar(self.ident()?)
            }
            _ => {
                self.pos = save;
                CommentTarget::Network
            }
        })
    }

    fn comment(&mut self) -> PResult<()> {
        self.keyword("CM_")?;
        let target = self.target()?;
        let text = self.string()?;
        self.semicolon()?;
        self.comments.push((target, text));
        Ok(())
    }

    fn attr_def(&mut self) -> PResult<()> {
        let kw = self.ident()?;
        self.skip_ws();
        let object = if kw == "BA_DEF_REL_" {
            match self.ident()?.as_str() {
                "BU_BO_REL_" => AttrObject::NodeMessage,
                "BU_SG_REL_" => AttrObject::NodeSignal,
                "BU_EV_REL_" => AttrObject::NodeEnvVar,
                other => return Err(format!("unknown relation type '{other}'")),
            }
        } else {
            match self.peek_ident().as_str() {
                "BU_" => {
                    self.pos += 3;
                    AttrObject::Node
                }
                "BO_" => {
                    self.pos += 3;
                    AttrObject::Message
                }
                "SG_" => {
                    self.pos += 3;
                    AttrObject::Signal
                }
                "EV_" => {
                    self.pos += 3;
                    AttrObject::EnvVar
                }
                _ => AttrObject::Network,
            }
        };
        let name = self.string()?;
        let ty = self.ident()?;
        let kind = match ty.as_str() {
            "INT" => AttrKind::Int {
                min: self.int()?,
                max: self.int()?,
            },
            "HEX" => AttrKind::Hex {
                min: self.int()?,
                max: self.int()?,
            },
            "FLOAT" => AttrKind::Float {
                min: self.float()?,
                max: self.float()?,
            },
            "STRING" => AttrKind::String,
            "ENUM" => {
                let mut opts = Vec::new();
                loop {
                    self.skip_ws();
                    if self.peek() == Some(';') {
                        break;
                    }
                    opts.push(self.string()?);
                    self.eat(',');
                }
                AttrKind::Enum(opts)
            }
            other => return Err(format!("unknown attribute type '{other}'")),
        };
        self.semicolon()?;
        self.dbc.attr_defs.push(AttrDef { object, name, kind });
        Ok(())
    }

    fn attr_value(&mut self) -> PResult<AttrValue> {
        self.skip_ws();
        if self.peek() == Some('"') {
            return Ok(AttrValue::Str(self.string()?));
        }
        let t = self.number_text()?;
        if t.contains(['.', 'e', 'E']) && !t.starts_with("0x") {
            Ok(AttrValue::Float(
                parse_f64(&t).ok_or_else(|| format!("invalid number '{t}'"))?,
            ))
        } else {
            match parse_i64(&t) {
                Some(i) => Ok(AttrValue::Int(i)),
                None => Ok(AttrValue::Float(
                    parse_f64(&t).ok_or_else(|| format!("invalid number '{t}'"))?,
                )),
            }
        }
    }

    fn attr_default(&mut self) -> PResult<()> {
        let kw = self.ident()?;
        let name = self.string()?;
        let value = self.attr_value()?;
        self.semicolon()?;
        self.dbc.attr_defaults.push(AttrDefault {
            name,
            value,
            relation: kw == "BA_DEF_DEF_REL_",
        });
        Ok(())
    }

    fn attribute(&mut self) -> PResult<()> {
        self.keyword("BA_")?;
        let name = self.string()?;
        let target = self.target()?;
        let value = self.attr_value()?;
        self.semicolon()?;
        self.attrs.push((target, Attribute { name, value }));
        Ok(())
    }

    fn val(&mut self) -> PResult<()> {
        self.keyword("VAL_")?;
        self.skip_ws();
        if matches!(self.peek(), Some(c) if c.is_ascii_digit() || c == '-') {
            let id = self.int()? as u32;
            let sig = self.ident()?;
            let entries = self.value_entries()?;
            self.vals.push((Some(id), sig, entries));
        } else {
            let env = self.ident()?;
            let entries = self.value_entries()?;
            self.vals.push((None, env, entries));
        }
        Ok(())
    }

    fn signal_group(&mut self) -> PResult<()> {
        self.keyword("SIG_GROUP_")?;
        let message_id = self.int()? as u32;
        let name = self.ident()?;
        let repetitions = self.uint()?;
        self.expect(':')?;
        let mut signals = Vec::new();
        loop {
            self.skip_ws();
            if self.eat(';') {
                break;
            }
            signals.push(self.ident()?);
            self.eat(',');
        }
        self.dbc.signal_groups.push(SignalGroup {
            message_id,
            name,
            repetitions,
            signals,
        });
        Ok(())
    }

    fn sig_valtype(&mut self) -> PResult<()> {
        self.keyword("SIG_VALTYPE_")?;
        let id = self.int()? as u32;
        let sig = self.ident()?;
        self.eat(':');
        let t = self.uint()? as u8;
        self.semicolon()?;
        self.valtypes.push((id, sig, t));
        Ok(())
    }

    fn sg_mul_val(&mut self) -> PResult<()> {
        self.keyword("SG_MUL_VAL_")?;
        let id = self.int()? as u32;
        let sig = self.ident()?;
        let switch = self.ident()?;
        let mut ranges = Vec::new();
        loop {
            self.skip_ws();
            if self.eat(';') {
                break;
            }
            // Ranges are written `a-b`; the tokenizer would read `-b` as a negative number.
            let a = self.int()?;
            self.expect('-')?;
            let b = self.int()?;
            ranges.push((a.max(0) as u64, b.max(0) as u64));
            self.eat(',');
        }
        self.ext_mux.push((id, sig, ExtMux { switch, ranges }));
        Ok(())
    }

    /// Keep an unmodelled statement verbatim, up to its terminating `;` (or end of line).
    fn raw_statement(&mut self) -> PResult<()> {
        let start = self.pos;
        let mut in_str = false;
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                '"' => in_str = !in_str,
                '\\' if in_str => self.pos += 1,
                ';' if !in_str => break,
                _ => {}
            }
        }
        let text: String = self.chars[start..self.pos.min(self.chars.len())]
            .iter()
            .collect();
        self.dbc.extra.push(text.trim().to_string());
        Ok(())
    }

    // ---- post-processing --------------------------------------------------------------------

    fn warn(&mut self, message: String) {
        self.errors.push(ParseError {
            line: 0,
            col: 0,
            message,
        });
    }

    fn resolve(&mut self) {
        for (id, nodes) in std::mem::take(&mut self.tx) {
            match self.dbc.message_mut(id) {
                Some(m) => m.extra_transmitters.extend(nodes),
                None => self.warn(format!("BO_TX_BU_ refers to unknown message id {id}")),
            }
        }
        for (name, size) in std::mem::take(&mut self.env_data) {
            match self.dbc.env_vars.iter_mut().find(|e| e.name == name) {
                Some(e) => e.data_size = Some(size),
                None => self.warn(format!("ENVVAR_DATA_ refers to unknown variable {name}")),
            }
        }
        for (target, text) in std::mem::take(&mut self.comments) {
            let slot = self.comment_slot(&target);
            match slot {
                Some(s) => *s = Some(text),
                None => self.warn(format!("comment refers to unknown object {target:?}")),
            }
        }
        for (target, attr) in std::mem::take(&mut self.attrs) {
            let slot = self.attr_slot(&target);
            match slot {
                Some(list) => list.push(attr),
                None => self.warn(format!(
                    "attribute {} refers to unknown object {target:?}",
                    attr.name
                )),
            }
        }
        for (id, sig, entries) in std::mem::take(&mut self.vals) {
            let ok = match id {
                Some(id) => match self.dbc.message_mut(id).and_then(|m| m.signal_mut(&sig)) {
                    Some(s) => {
                        s.value_descriptions = entries;
                        true
                    }
                    None => false,
                },
                None => match self.dbc.env_vars.iter_mut().find(|e| e.name == sig) {
                    Some(e) => {
                        e.value_descriptions = entries;
                        true
                    }
                    None => false,
                },
            };
            if !ok {
                self.warn(format!("VAL_ refers to unknown signal {sig}"));
            }
        }
        for (id, sig, t) in std::mem::take(&mut self.valtypes) {
            match self.dbc.message_mut(id).and_then(|m| m.signal_mut(&sig)) {
                Some(s) => {
                    s.value_type = match t {
                        1 => ValueType::Float32,
                        2 => ValueType::Float64,
                        _ => s.value_type,
                    }
                }
                None => self.warn(format!("SIG_VALTYPE_ refers to unknown signal {sig}")),
            }
        }
        for (id, sig, em) in std::mem::take(&mut self.ext_mux) {
            match self.dbc.message_mut(id).and_then(|m| m.signal_mut(&sig)) {
                Some(s) => s.ext_mux.push(em),
                None => self.warn(format!("SG_MUL_VAL_ refers to unknown signal {sig}")),
            }
        }
    }

    fn comment_slot(&mut self, t: &CommentTarget) -> Option<&mut Option<String>> {
        let d = &mut self.dbc;
        match t {
            CommentTarget::Network => Some(&mut d.comment),
            CommentTarget::Node(n) => d
                .nodes
                .iter_mut()
                .find(|x| &x.name == n)
                .map(|x| &mut x.comment),
            CommentTarget::Message(id) => d.message_mut(*id).map(|m| &mut m.comment),
            CommentTarget::Signal(id, s) => d
                .message_mut(*id)
                .and_then(|m| m.signal_mut(s))
                .map(|s| &mut s.comment),
            CommentTarget::EnvVar(n) => d
                .env_vars
                .iter_mut()
                .find(|e| &e.name == n)
                .map(|e| &mut e.comment),
        }
    }

    fn attr_slot(&mut self, t: &CommentTarget) -> Option<&mut Vec<Attribute>> {
        let d = &mut self.dbc;
        match t {
            CommentTarget::Network => Some(&mut d.attributes),
            CommentTarget::Node(n) => d
                .nodes
                .iter_mut()
                .find(|x| &x.name == n)
                .map(|x| &mut x.attributes),
            CommentTarget::Message(id) => d.message_mut(*id).map(|m| &mut m.attributes),
            CommentTarget::Signal(id, s) => d
                .message_mut(*id)
                .and_then(|m| m.signal_mut(s))
                .map(|s| &mut s.attributes),
            CommentTarget::EnvVar(n) => d
                .env_vars
                .iter_mut()
                .find(|e| &e.name == n)
                .map(|e| &mut e.attributes),
        }
    }
}

pub(crate) fn parse_mux(tok: &str) -> Option<Mux> {
    if tok == "M" {
        return Some(Mux::Multiplexor);
    }
    let rest = tok.strip_prefix('m')?;
    if let Some(n) = rest.strip_suffix('M') {
        return n.parse().ok().map(Mux::MultiplexedMultiplexor);
    }
    rest.parse().ok().map(Mux::Multiplexed)
}

fn parse_i64(t: &str) -> Option<i64> {
    let (neg, body) = match t.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let v = if let Some(h) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        i64::from_str_radix(h, 16).ok()?
    } else {
        body.parse::<i64>()
            .ok()
            .or_else(|| body.parse::<u64>().ok().map(|u| u as i64))?
    };
    Some(if neg { -v } else { v })
}

fn parse_f64(t: &str) -> Option<f64> {
    if t.contains("0x") || t.contains("0X") {
        return parse_i64(t).map(|i| i as f64);
    }
    t.parse().ok()
}
