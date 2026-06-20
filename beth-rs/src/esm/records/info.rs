//! `INFO` — a dialogue response (child of the preceding `DIAL` record).

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, finish, le_f32, le_u32, parse_or_default};
use nom::IResult;
use nom::number::complete::{le_i8, le_u8};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct InfoData {
    /// Duplicates the parent DIAL's dialogue type.
    pub dialogue_type: u8,
    /// Disposition threshold, or journal index for journal entries.
    pub disposition: u32,
    /// Required NPC rank, or `-1`.
    pub rank: i8,
    /// -1 = none, 0 = male, 1 = female.
    pub gender: i8,
    /// Required PC rank, or `-1`.
    pub pc_rank: i8,
}

fn info_data(input: &[u8]) -> IResult<&[u8], InfoData> {
    let (input, dialogue_type) = le_u8(input)?;
    let (input, _unused) = nom::bytes::complete::take(3usize)(input)?;
    let (input, disposition) = le_u32(input)?;
    let (input, rank) = le_i8(input)?;
    let (input, gender) = le_i8(input)?;
    let (input, pc_rank) = le_i8(input)?;
    let (input, _unused) = le_u8(input)?;
    Ok((
        input,
        InfoData {
            dialogue_type,
            disposition,
            rank,
            gender,
            pc_rank,
        },
    ))
}

/// A select/filter function applied to a response (`SCVR` plus an optional value).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Filter<'a> {
    pub text: &'a L1Str,
    pub int_value: Option<u32>,
    pub float_value: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Info<'a> {
    /// Unique info ID (`INAM`).
    pub id: &'a L1Str,
    /// Previous info ID in the topic's linked list.
    pub prev: &'a L1Str,
    /// Next info ID in the topic's linked list.
    pub next: &'a L1Str,
    pub data: Option<InfoData>,
    pub actor: Option<&'a L1Str>,
    pub race: Option<&'a L1Str>,
    pub class: Option<&'a L1Str>,
    pub faction: Option<&'a L1Str>,
    pub cell: Option<&'a L1Str>,
    pub pc_faction: Option<&'a L1Str>,
    pub sound: Option<&'a L1Str>,
    /// Response text (`NAME`).
    pub response: Option<&'a L1Str>,
    pub filters: Vec<Filter<'a>>,
    /// Result script text (`BNAM`).
    pub result: Option<&'a L1Str>,
    /// Journal flags.
    pub quest_name: bool,
    pub quest_finished: bool,
    pub quest_restart: bool,
}

impl<'a> Info<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Info<'a> {
        let mut out = Info::default();
        for sub in subs {
            match &sub.tag {
                b"INAM" => out.id = l1(sub.data),
                b"PNAM" => out.prev = l1(sub.data),
                b"NNAM" => out.next = l1(sub.data),
                b"DATA" => out.data = Some(parse_or_default(info_data, sub.data)),
                b"ONAM" => out.actor = Some(l1(sub.data)),
                b"RNAM" => out.race = Some(l1(sub.data)),
                b"CNAM" => out.class = Some(l1(sub.data)),
                b"FNAM" => out.faction = Some(l1(sub.data)),
                b"ANAM" => out.cell = Some(l1(sub.data)),
                b"DNAM" => out.pc_faction = Some(l1(sub.data)),
                b"SNAM" => out.sound = Some(l1(sub.data)),
                b"NAME" => out.response = Some(l1(sub.data)),
                b"SCVR" => out.filters.push(Filter {
                    text: l1(sub.data),
                    int_value: None,
                    float_value: None,
                }),
                b"INTV" => {
                    if let Some(last) = out.filters.last_mut() {
                        last.int_value = finish(le_u32(sub.data));
                    }
                }
                b"FLTV" => {
                    if let Some(last) = out.filters.last_mut() {
                        last.float_value = finish(le_f32(sub.data));
                    }
                }
                b"BNAM" => out.result = Some(l1(sub.data)),
                b"QSTN" => out.quest_name = sub.data.first().is_some_and(|&b| b != 0),
                b"QSTF" => out.quest_finished = sub.data.first().is_some_and(|&b| b != 0),
                b"QSTR" => out.quest_restart = sub.data.first().is_some_and(|&b| b != 0),
                _ => {}
            }
        }
        out
    }
}
