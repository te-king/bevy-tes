//! `MGEF` — a magic effect.

use crate::types::latin1::L1Str;
use crate::esm::common::{Subrecord, l1, finish, le_f32, le_u32, parse_or_default};
use nom::IResult;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MagicEffectData {
    /// Spell school (0 = Alteration … 5 = Restoration).
    pub school: u32,
    pub base_cost: f32,
    /// See record docs (Harmful, Targets Skill, etc.).
    pub flags: u32,
    pub red: u32,
    pub green: u32,
    pub blue: u32,
    pub speed_x: f32,
    pub size_x: f32,
    pub size_cap: f32,
}

fn magic_effect_data(input: &[u8]) -> IResult<&[u8], MagicEffectData> {
    let (input, school) = le_u32(input)?;
    let (input, base_cost) = le_f32(input)?;
    let (input, flags) = le_u32(input)?;
    let (input, red) = le_u32(input)?;
    let (input, green) = le_u32(input)?;
    let (input, blue) = le_u32(input)?;
    let (input, speed_x) = le_f32(input)?;
    let (input, size_x) = le_f32(input)?;
    let (input, size_cap) = le_f32(input)?;
    Ok((
        input,
        MagicEffectData {
            school,
            base_cost,
            flags,
            red,
            green,
            blue,
            speed_x,
            size_x,
            size_cap,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mgef<'a> {
    /// Effect index (names are hardcoded in the engine).
    pub index: u32,
    pub data: MagicEffectData,
    pub icon: Option<&'a L1Str>,
    pub particle_texture: Option<&'a L1Str>,
    pub bolt_sound: Option<&'a L1Str>,
    pub casting_sound: Option<&'a L1Str>,
    pub hit_sound: Option<&'a L1Str>,
    pub area_sound: Option<&'a L1Str>,
    pub casting_visual: Option<&'a L1Str>,
    pub bolt_visual: Option<&'a L1Str>,
    pub hit_visual: Option<&'a L1Str>,
    pub area_visual: Option<&'a L1Str>,
    pub description: Option<&'a L1Str>,
}

impl<'a> Mgef<'a> {
    pub fn from_subrecords(subs: &[Subrecord<'a>]) -> Mgef<'a> {
        let mut out = Mgef::default();
        for sub in subs {
            match &sub.tag {
                b"INDX" => out.index = finish(le_u32(sub.data)).unwrap_or(0),
                b"MEDT" => out.data = parse_or_default(magic_effect_data, sub.data),
                b"ITEX" => out.icon = Some(l1(sub.data)),
                b"PTEX" => out.particle_texture = Some(l1(sub.data)),
                b"BSND" => out.bolt_sound = Some(l1(sub.data)),
                b"CSND" => out.casting_sound = Some(l1(sub.data)),
                b"HSND" => out.hit_sound = Some(l1(sub.data)),
                b"ASND" => out.area_sound = Some(l1(sub.data)),
                b"CVFX" => out.casting_visual = Some(l1(sub.data)),
                b"BVFX" => out.bolt_visual = Some(l1(sub.data)),
                b"HVFX" => out.hit_visual = Some(l1(sub.data)),
                b"AVFX" => out.area_visual = Some(l1(sub.data)),
                b"DESC" => out.description = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
