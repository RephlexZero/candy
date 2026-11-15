//! # candy-dbc
//!
//! The engine behind CANdy: a lossless, error-tolerant CAN DBC parser and canonical writer, plus
//! bit-layout analysis, frame decoding/encoding, lint rules with fixes, naming conventions and
//! structural diff/merge. Pure Rust with no platform dependencies.
//!
//! ```
//! let src = "VERSION \"\"\nBU_: ECU\nBO_ 256 Engine: 8 ECU\n SG_ Rpm : 0|16@1+ (0.25,0) [0|16383.75] \"rpm\" Vector__XXX\n";
//! let dbc = candy_dbc::parse(src).dbc;
//! let msg = dbc.message(256).unwrap();
//! let decoded = candy_dbc::decode::decode_message(msg, &[0x40, 0x1F, 0, 0, 0, 0, 0, 0]);
//! assert_eq!(decoded[0].physical, 2000.0);
//! ```

pub mod decode;
pub mod diff;
pub mod layout;
pub mod lint;
pub mod model;
pub mod naming;
pub mod parser;
pub mod stats;
pub mod writer;

pub use model::*;
pub use parser::{ParseError, ParseOutput, parse, parse_strict};
pub use writer::write;
