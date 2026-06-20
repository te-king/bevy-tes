//! `ENCH` — an enchantment.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, le_u32, parse_or_default};
use crate::esm::shared::{Effect, effect};
use nom::IResult;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EnchantData {
    /// 0 = Cast Once, 1 = Cast Strikes, 2 = Cast when Used, 3 = Constant Effect.
    pub kind: u32,
    pub cost: u32,
    pub charge: u32,
    /// `0x1` = Autocalc.
    pub flags: u32,
}

fn enchant_data(input: &[u8]) -> IResult<&[u8], EnchantData> {
    let (input, kind) = le_u32(input)?;
    let (input, cost) = le_u32(input)?;
    let (input, charge) = le_u32(input)?;
    let (input, flags) = le_u32(input)?;
    Ok((
        input,
        EnchantData {
            kind,
            cost,
            charge,
            flags,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ench<'a> {
    pub id: &'a L1Str,
    pub data: EnchantData,
    pub effects: Vec<Effect>,
}

impl<'a> Ench<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Ench<'a> {
        let mut out = Ench::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"ENDT" => out.data = parse_or_default(enchant_data, sub.data),
                b"ENAM" => out.effects.push(parse_or_default(effect, sub.data)),
                _ => {}
            }
        }
        out
    }
}
