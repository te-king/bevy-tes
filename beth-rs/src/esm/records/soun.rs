//! `SOUN` — a sound effect.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, le_u8, parse_or_default};
use nom::IResult;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SoundData {
    /// Volume, 0 = 0.00 … 255 = 1.00.
    pub volume: u8,
    pub min_range: u8,
    pub max_range: u8,
}

fn sound_data(input: &[u8]) -> IResult<&[u8], SoundData> {
    let (input, volume) = le_u8(input)?;
    let (input, min_range) = le_u8(input)?;
    let (input, max_range) = le_u8(input)?;
    Ok((
        input,
        SoundData {
            volume,
            min_range,
            max_range,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Soun<'a> {
    pub id: &'a L1Str,
    pub filename: &'a L1Str,
    pub data: SoundData,
}

impl<'a> Soun<'a> {
    pub fn from_subrecords(subs: &[Subrecord<'a>]) -> Soun<'a> {
        let mut out = Soun::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"FNAM" => out.filename = l1(sub.data),
                b"DATA" => out.data = parse_or_default(sound_data, sub.data),
                _ => {}
            }
        }
        out
    }
}
