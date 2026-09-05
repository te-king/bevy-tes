//! `INFO` — a dialogue response (child of the preceding `DIAL` record).

use crate::common::{Subrecord, enumeration, finish, l1, le_f32, le_u32, parse_or_default};
use crate::records::dial::DialogueKind;
use nom::Parser;
use nom::bytes::complete::take;
use nom::number::complete::{le_i8, le_u8};
use nom::sequence::terminated;
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

#[derive(Debug, Clone, Copy, PartialEq, Default, TesPayload)]
#[tes(parser = info_data)]
pub struct InfoData {
    /// Duplicates the parent DIAL's dialogue type.
    #[tes(read = |input| terminated(enumeration, take(3usize)).parse(input))]
    pub dialogue_type: DialogueKind,
    /// Disposition threshold, or journal index for journal entries.
    #[tes(read = le_u32)]
    pub disposition: u32,
    /// Required NPC rank, or `-1`.
    #[tes(read = le_i8)]
    pub rank: i8,
    /// -1 = none, 0 = male, 1 = female.
    #[tes(read = le_i8)]
    pub gender: i8,
    /// Required PC rank, or `-1`.
    #[tes(read = |input| terminated(le_i8, le_u8).parse(input))]
    pub pc_rank: i8,
}

/// A select/filter function applied to a response (`SCVR` plus an optional value).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Filter<'a> {
    pub text: &'a L1Str,
    pub int_value: Option<u32>,
    pub float_value: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
#[tes(unmapped = Self::filter_field)]
pub struct Info<'a> {
    /// Unique info ID (`INAM`).
    #[tes(tag = b"INAM", decode = l1)]
    pub id: &'a L1Str,
    /// Previous info ID in the topic's linked list.
    #[tes(tag = b"PNAM", decode = l1)]
    pub prev: &'a L1Str,
    /// Next info ID in the topic's linked list.
    #[tes(tag = b"NNAM", decode = l1)]
    pub next: &'a L1Str,
    #[tes(tag = b"DATA", decode = |bytes| Some(parse_or_default(info_data, bytes)))]
    pub data: Option<InfoData>,
    #[tes(tag = b"ONAM", decode = |bytes| Some(l1(bytes)))]
    pub actor: Option<&'a L1Str>,
    #[tes(tag = b"RNAM", decode = |bytes| Some(l1(bytes)))]
    pub race: Option<&'a L1Str>,
    #[tes(tag = b"CNAM", decode = |bytes| Some(l1(bytes)))]
    pub class: Option<&'a L1Str>,
    #[tes(tag = b"FNAM", decode = |bytes| Some(l1(bytes)))]
    pub faction: Option<&'a L1Str>,
    #[tes(tag = b"ANAM", decode = |bytes| Some(l1(bytes)))]
    pub cell: Option<&'a L1Str>,
    #[tes(tag = b"DNAM", decode = |bytes| Some(l1(bytes)))]
    pub pc_faction: Option<&'a L1Str>,
    #[tes(tag = b"SNAM", decode = |bytes| Some(l1(bytes)))]
    pub sound: Option<&'a L1Str>,
    /// Response text (`NAME`).
    #[tes(tag = b"NAME", decode = |bytes| Some(l1(bytes)))]
    pub response: Option<&'a L1Str>,
    #[tes(skip)]
    pub filters: Vec<Filter<'a>>,
    /// Result script text (`BNAM`).
    #[tes(tag = b"BNAM", decode = |bytes| Some(l1(bytes)))]
    pub result: Option<&'a L1Str>,
    /// Journal flags.
    #[tes(tag = b"QSTN", decode = |bytes: &[u8]| bytes.first().is_some_and(|&b| b != 0))]
    pub quest_name: bool,
    #[tes(tag = b"QSTF", decode = |bytes: &[u8]| bytes.first().is_some_and(|&b| b != 0))]
    pub quest_finished: bool,
    #[tes(tag = b"QSTR", decode = |bytes: &[u8]| bytes.first().is_some_and(|&b| b != 0))]
    pub quest_restart: bool,
}

impl<'a> Info<'a> {
    fn filter_field(&mut self, sub: Subrecord<'a>) {
        match &sub.tag.0 {
            b"SCVR" => self.filters.push(Filter {
                text: l1(sub.data),
                int_value: None,
                float_value: None,
            }),
            b"INTV" => {
                if let Some(last) = self.filters.last_mut() {
                    last.int_value = finish(le_u32(sub.data));
                }
            }
            b"FLTV" => {
                if let Some(last) = self.filters.last_mut() {
                    last.float_value = finish(le_f32(sub.data));
                }
            }
            _ => {}
        }
    }
}
