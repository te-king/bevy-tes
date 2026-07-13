//! `ENCH` — an enchantment.

use crate::common::{Subrecord, enumeration, flags, l1, le_u32, parse_or_default};
use crate::macros::enum_field;
use crate::shared::{Effect, effect};
use nom::IResult;
use tes_core::L1String;

bitflags::bitflags! {
    /// Enchantment flags (`ENDT`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct EnchantFlags: u32 {
        const AUTOCALC = 0x1;
    }
}

enum_field! {
    /// Enchantment trigger (`ENDT`).
    pub enum EnchantKind: u32 {
        CastOnce = 0,
        CastOnStrike = 1,
        CastWhenUsed = 2,
        ConstantEffect = 3,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EnchantData {
    pub kind: EnchantKind,
    pub cost: u32,
    pub charge: u32,
    pub flags: EnchantFlags,
}

fn enchant_data(input: &[u8]) -> IResult<&[u8], EnchantData> {
    let (input, kind) = enumeration(input)?;
    let (input, cost) = le_u32(input)?;
    let (input, charge) = le_u32(input)?;
    let (input, flags) = flags(input)?;
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
pub struct Ench {
    pub id: L1String,
    pub data: EnchantData,
    pub effects: Vec<Effect>,
}

impl Ench {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Ench {
        let mut out = Ench::default();
        for sub in subs {
            match &sub.tag.0 {
                b"NAME" => out.id = l1(sub.data),
                b"ENDT" => out.data = parse_or_default(enchant_data, sub.data),
                b"ENAM" => out.effects.push(parse_or_default(effect, sub.data)),
                _ => {}
            }
        }
        out
    }
}
