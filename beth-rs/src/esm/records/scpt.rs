//! `SCPT` — a script.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, fixed_l1str, le_u32, parse_or_default};
use nom::IResult;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScriptHeader<'a> {
    pub name: &'a L1Str,
    pub num_shorts: u32,
    pub num_longs: u32,
    pub num_floats: u32,
    pub script_data_size: u32,
    pub local_var_size: u32,
}

fn script_header(input: &[u8]) -> IResult<&[u8], ScriptHeader<'_>> {
    let (input, name) = fixed_l1str(32)(input)?;
    let (input, num_shorts) = le_u32(input)?;
    let (input, num_longs) = le_u32(input)?;
    let (input, num_floats) = le_u32(input)?;
    let (input, script_data_size) = le_u32(input)?;
    let (input, local_var_size) = le_u32(input)?;
    Ok((
        input,
        ScriptHeader {
            name,
            num_shorts,
            num_longs,
            num_floats,
            script_data_size,
            local_var_size,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scpt<'a> {
    pub header: ScriptHeader<'a>,
    /// Local variable names (NUL-separated in the `SCVR` subrecord).
    pub variables: Vec<&'a L1Str>,
    /// Compiled script byte code (borrowed from the source buffer).
    pub data: &'a [u8],
    /// Human-readable script text.
    pub text: Option<&'a L1Str>,
}

impl<'a> Scpt<'a> {
    pub fn from_subrecords(subs: &[Subrecord<'a>]) -> Scpt<'a> {
        let mut out = Scpt::default();
        for sub in subs {
            match &sub.tag {
                b"SCHD" => out.header = parse_or_default(script_header, sub.data),
                b"SCVR" => {
                    out.variables = sub
                        .data
                        .split(|&b| b == 0)
                        .filter(|s| !s.is_empty())
                        .map(l1)
                        .collect();
                }
                b"SCDT" => out.data = sub.data,
                b"SCTX" => out.text = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
