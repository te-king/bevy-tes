//! `SCPT` — a script.

use crate::common::{fixed_l1str, l1, le_u32, parse_or_default};
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

#[derive(Debug, Clone, PartialEq, Default, TesPayload)]
#[tes(parser = script_header)]
pub struct ScriptHeader<'a> {
    #[tes(read = fixed_l1str(32))]
    pub name: &'a L1Str,
    #[tes(read = le_u32)]
    pub num_shorts: u32,
    #[tes(read = le_u32)]
    pub num_longs: u32,
    #[tes(read = le_u32)]
    pub num_floats: u32,
    #[tes(read = le_u32)]
    pub script_data_size: u32,
    #[tes(read = le_u32)]
    pub local_var_size: u32,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
pub struct Scpt<'a> {
    #[tes(tag = b"SCHD", decode = |bytes| parse_or_default(script_header, bytes))]
    pub header: ScriptHeader<'a>,
    /// Local variable names (NUL-separated in the `SCVR` subrecord).
    #[tes(tag = b"SCVR", decode = variable_names)]
    pub variables: Vec<&'a L1Str>,
    /// Compiled script byte code.
    #[tes(tag = b"SCDT", decode = |bytes| bytes)]
    pub data: &'a [u8],
    /// Human-readable script text.
    #[tes(tag = b"SCTX", decode = |bytes| Some(l1(bytes)))]
    pub text: Option<&'a L1Str>,
}

fn variable_names(bytes: &[u8]) -> Vec<&L1Str> {
    bytes
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(l1)
        .collect()
}
