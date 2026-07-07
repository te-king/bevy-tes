//! `SCPT` — a script.

use crate::common::{Subrecord, fixed_l1str, l1, le_u32, parse_or_default};
use crate::macros::parse_struct;
use tes_core::L1String;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScriptHeader {
    pub name: L1String,
    pub num_shorts: u32,
    pub num_longs: u32,
    pub num_floats: u32,
    pub script_data_size: u32,
    pub local_var_size: u32,
}

parse_struct! {
    fn script_header -> ScriptHeader {
        name: fixed_l1str(32),
        num_shorts: le_u32,
        num_longs: le_u32,
        num_floats: le_u32,
        script_data_size: le_u32,
        local_var_size: le_u32,
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scpt {
    pub header: ScriptHeader,
    /// Local variable names (NUL-separated in the `SCVR` subrecord).
    pub variables: Vec<L1String>,
    /// Compiled script byte code.
    pub data: Vec<u8>,
    /// Human-readable script text.
    pub text: Option<L1String>,
}

impl Scpt {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Scpt {
        let mut out = Scpt::default();
        for sub in subs {
            match &sub.tag.0 {
                b"SCHD" => out.header = parse_or_default(script_header, sub.data),
                b"SCVR" => {
                    out.variables = sub
                        .data
                        .split(|&b| b == 0)
                        .filter(|s| !s.is_empty())
                        .map(l1)
                        .collect();
                }
                b"SCDT" => out.data = sub.data.to_vec(),
                b"SCTX" => out.text = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
