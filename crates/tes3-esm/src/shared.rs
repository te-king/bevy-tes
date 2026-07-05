//! Sub-structures that appear inside several different record types (spell effects,
//! inventory items, AI data and packages, etc.). Keeping them here avoids duplicating
//! the parsers across the item/actor record modules.
//!
//! String-bearing types own their text as [`L1String`](crate::L1String) (decoded on
//! demand); purely numeric types ([`Effect`], [`AiData`], [`AmbientLight`]) are `Copy`.

use super::common::{Color, color, fixed_l1str, parse_struct};
use nom::IResult;
use nom::number::complete::{le_f32, le_i8, le_i32, le_u8, le_u16, le_u32};
use tes_core::L1String;

/// A single magic effect entry (`ENAM`, 24 bytes). Shared by SPEL, ENCH and ALCH.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Effect {
    pub effect_index: u16,
    /// Skill affected, or `-1` if not applicable.
    pub skill: i8,
    /// Attribute affected, or `-1` if not applicable.
    pub attribute: i8,
    /// 0 = Self, 1 = Touch, 2 = Target.
    pub range: u32,
    pub area: u32,
    pub duration: u32,
    pub magnitude_min: u32,
    pub magnitude_max: u32,
}

parse_struct! {
    pub fn effect -> Effect {
        effect_index: le_u16,
        skill: le_i8,
        attribute: le_i8,
        range: le_u32,
        area: le_u32,
        duration: le_u32,
        magnitude_min: le_u32,
        magnitude_max: le_u32,
    }
}

/// A carried/contained inventory entry (`NPCO`, 36 bytes). Shared by CONT, CREA, NPC_.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InventoryItem {
    /// Object count; negative counts indicate restocking.
    pub count: i32,
    /// ID of the contained object.
    pub object: L1String,
}

parse_struct! {
    pub fn inventory_item -> InventoryItem {
        count: le_i32,
        object: fixed_l1str(32),
    }
}

/// AI behavior data (`AIDT`, 12 bytes). Shared by CREA and NPC_.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AiData {
    pub hello: u8,
    pub fight: u8,
    pub flee: u8,
    pub alarm: u8,
    /// Services offered / auto-calc flags (see record docs).
    pub flags: u32,
}

pub fn ai_data(input: &[u8]) -> IResult<&[u8], AiData> {
    let (input, hello) = le_u8(input)?;
    let (input, _unknown) = le_u8(input)?;
    let (input, fight) = le_u8(input)?;
    let (input, flee) = le_u8(input)?;
    let (input, alarm) = le_u8(input)?;
    let (input, _pad) = nom::bytes::complete::take(3usize)(input)?;
    let (input, flags) = le_u32(input)?;
    Ok((
        input,
        AiData {
            hello,
            fight,
            flee,
            alarm,
            flags,
        },
    ))
}

/// A cell travel destination (`DODT` 24 bytes) plus an optional interior cell name
/// (`DNAM`). Shared by CREA, NPC_ and CELL references.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TravelDestination {
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    /// Interior cell name, from a following `DNAM` subrecord.
    pub cell: Option<L1String>,
}

/// Parse the 24-byte `DODT` payload (position + rotation); `cell` is filled in later.
pub fn travel_destination(input: &[u8]) -> IResult<&[u8], TravelDestination> {
    let (input, px) = le_f32(input)?;
    let (input, py) = le_f32(input)?;
    let (input, pz) = le_f32(input)?;
    let (input, rx) = le_f32(input)?;
    let (input, ry) = le_f32(input)?;
    let (input, rz) = le_f32(input)?;
    Ok((
        input,
        TravelDestination {
            position: [px, py, pz],
            rotation: [rx, ry, rz],
            cell: None,
        },
    ))
}

/// An AI package attached to an actor (CREA / NPC_). The order of packages defines
/// their priority.
#[derive(Debug, Clone, PartialEq)]
pub enum AiPackage {
    /// `AI_A` — activate a named object.
    Activate { name: L1String },
    /// `AI_E` — escort to a position (with optional destination cell from `CNDT`).
    Escort {
        position: [f32; 3],
        duration: u16,
        id: L1String,
        cell: Option<L1String>,
    },
    /// `AI_F` — follow a target (with optional destination cell from `CNDT`).
    Follow {
        position: [f32; 3],
        duration: u16,
        id: L1String,
        cell: Option<L1String>,
    },
    /// `AI_T` — travel to a position.
    Travel { position: [f32; 3] },
    /// `AI_W` — wander within a distance.
    Wander {
        distance: u16,
        duration: u16,
        time_of_day: u8,
        idles: [u8; 8],
    },
}

pub fn ai_activate(input: &[u8]) -> IResult<&[u8], AiPackage> {
    let (input, name) = fixed_l1str(32)(input)?;
    Ok((input, AiPackage::Activate { name }))
}

/// Shared decoded body of the `AI_E`/`AI_F` packages: (position, duration, id).
type EscortFollow = ([f32; 3], u16, L1String);

/// Shared body of the `AI_E` (escort) and `AI_F` (follow) 48-byte packages.
fn ai_escort_follow(input: &[u8]) -> IResult<&[u8], EscortFollow> {
    let (input, x) = le_f32(input)?;
    let (input, y) = le_f32(input)?;
    let (input, z) = le_f32(input)?;
    let (input, duration) = le_u16(input)?;
    let (input, id) = fixed_l1str(32)(input)?;
    Ok((input, ([x, y, z], duration, id)))
}

pub fn ai_escort(input: &[u8]) -> IResult<&[u8], AiPackage> {
    let (input, (position, duration, id)) = ai_escort_follow(input)?;
    Ok((
        input,
        AiPackage::Escort {
            position,
            duration,
            id,
            cell: None,
        },
    ))
}

pub fn ai_follow(input: &[u8]) -> IResult<&[u8], AiPackage> {
    let (input, (position, duration, id)) = ai_escort_follow(input)?;
    Ok((
        input,
        AiPackage::Follow {
            position,
            duration,
            id,
            cell: None,
        },
    ))
}

pub fn ai_travel(input: &[u8]) -> IResult<&[u8], AiPackage> {
    let (input, x) = le_f32(input)?;
    let (input, y) = le_f32(input)?;
    let (input, z) = le_f32(input)?;
    Ok((
        input,
        AiPackage::Travel {
            position: [x, y, z],
        },
    ))
}

pub fn ai_wander(input: &[u8]) -> IResult<&[u8], AiPackage> {
    let (input, distance) = le_u16(input)?;
    let (input, duration) = le_u16(input)?;
    let (input, time_of_day) = le_u8(input)?;
    let mut input = input;
    let mut idles = [0u8; 8];
    for slot in idles.iter_mut() {
        let (rest, b) = le_u8(input)?;
        *slot = b;
        input = rest;
    }
    Ok((
        input,
        AiPackage::Wander {
            distance,
            duration,
            time_of_day,
            idles,
        },
    ))
}

/// A biped equipment slot entry (`INDX` + optional `BNAM`/`CNAM`). Shared by ARMO and
/// CLOT, describing which body part a model piece applies to.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BipedItem {
    /// Biped object index (body part slot).
    pub index: u8,
    pub male_model: Option<L1String>,
    pub female_model: Option<L1String>,
}

/// Ambient lighting block (`AMBI`, 16 bytes) found in interior CELL records.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AmbientLight {
    pub ambient: Color,
    pub sunlight: Color,
    pub fog: Color,
    pub fog_density: f32,
}

parse_struct! {
    pub fn ambient_light -> AmbientLight {
        ambient: color,
        sunlight: color,
        fog: color,
        fog_density: le_f32,
    }
}
