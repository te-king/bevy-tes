//! Sub-structures that appear inside several different record types (spell effects,
//! inventory items, AI data and packages, etc.). Keeping them here avoids duplicating
//! the parsers across the item/actor record modules.
//!
//! String-bearing types borrow their text from the input as [`&L1Str`](crate::L1Str)
//! (decoded on demand); purely numeric types ([`Effect`], [`AiData`], [`AmbientLight`])
//! are `Copy`.

use super::common::{Color, color, enumeration, fixed_l1str, flags};
use crate::macros::enum_field;
use nom::IResult;
use nom::number::complete::{le_f32, le_i8, le_i32, le_u8, le_u16, le_u32};
use tes_core::L1Str;

bitflags::bitflags! {
    /// Services offered by an actor or auto-calculated for a class. Shared by the
    /// `AIDT` flags (CREA/NPC_) and the CLAS auto-calc field, which use the same layout.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct ServiceFlags: u32 {
        const BARTERS_WEAPONS = 0x0000_0001;
        const BARTERS_ARMOR = 0x0000_0002;
        const BARTERS_CLOTHING = 0x0000_0004;
        const BARTERS_BOOKS = 0x0000_0008;
        const BARTERS_INGREDIENTS = 0x0000_0010;
        const BARTERS_PICKS = 0x0000_0020;
        const BARTERS_PROBES = 0x0000_0040;
        const BARTERS_LIGHTS = 0x0000_0080;
        const BARTERS_APPARATUS = 0x0000_0100;
        const BARTERS_REPAIR_ITEMS = 0x0000_0200;
        const BARTERS_MISC = 0x0000_0400;
        const BARTERS_SPELLS = 0x0000_0800;
        const BARTERS_MAGIC_ITEMS = 0x0000_1000;
        const BARTERS_POTIONS = 0x0000_2000;
        const OFFERS_TRAINING = 0x0000_4000;
        const OFFERS_SPELLMAKING = 0x0000_8000;
        const OFFERS_ENCHANTING = 0x0001_0000;
        const OFFERS_REPAIR = 0x0002_0000;
    }
}

enum_field! {
    /// A class or skill specialization. Shared by CLAS and SKIL.
    pub enum Specialization: u32 {
        Combat = 0,
        Magic = 1,
        Stealth = 2,
    }
}

enum_field! {
    /// Delivery range of a magic effect (stored as `Self` in the editor).
    pub enum EffectRange: u32 {
        OnSelf = 0,
        Touch = 1,
        Target = 2,
    }
}

/// A single magic effect entry (`ENAM`, 24 bytes). Shared by SPEL, ENCH and ALCH.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Effect {
    pub effect_index: u16,
    /// Skill affected, or `-1` if not applicable.
    pub skill: i8,
    /// Attribute affected, or `-1` if not applicable.
    pub attribute: i8,
    pub range: EffectRange,
    pub area: u32,
    pub duration: u32,
    pub magnitude_min: u32,
    pub magnitude_max: u32,
}

pub fn effect(input: &[u8]) -> IResult<&[u8], Effect> {
    let (input, effect_index) = le_u16(input)?;
    let (input, skill) = le_i8(input)?;
    let (input, attribute) = le_i8(input)?;
    let (input, range) = enumeration(input)?;
    let (input, area) = le_u32(input)?;
    let (input, duration) = le_u32(input)?;
    let (input, magnitude_min) = le_u32(input)?;
    let (input, magnitude_max) = le_u32(input)?;
    Ok((
        input,
        Effect {
            effect_index,
            skill,
            attribute,
            range,
            area,
            duration,
            magnitude_min,
            magnitude_max,
        },
    ))
}

/// A carried/contained inventory entry (`NPCO`, 36 bytes). Shared by CONT, CREA, NPC_.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InventoryItem<'a> {
    /// Object count; negative counts indicate restocking.
    pub count: i32,
    /// ID of the contained object.
    pub object: &'a L1Str,
}

pub fn inventory_item(input: &[u8]) -> IResult<&[u8], InventoryItem<'_>> {
    let (input, count) = le_i32(input)?;
    let (input, object) = fixed_l1str(32)(input)?;
    Ok((input, InventoryItem { count, object }))
}

/// AI behavior data (`AIDT`, 12 bytes). Shared by CREA and NPC_.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AiData {
    pub hello: u8,
    pub fight: u8,
    pub flee: u8,
    pub alarm: u8,
    pub flags: ServiceFlags,
}

pub fn ai_data(input: &[u8]) -> IResult<&[u8], AiData> {
    let (input, hello) = le_u8(input)?;
    let (input, _unknown) = le_u8(input)?;
    let (input, fight) = le_u8(input)?;
    let (input, flee) = le_u8(input)?;
    let (input, alarm) = le_u8(input)?;
    let (input, _pad) = nom::bytes::complete::take(3usize)(input)?;
    let (input, flags) = flags(input)?;
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
pub struct TravelDestination<'a> {
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    /// Interior cell name, from a following `DNAM` subrecord.
    pub cell: Option<&'a L1Str>,
}

/// Parse the 24-byte `DODT` payload (position + rotation); `cell` is filled in later.
pub fn travel_destination(input: &[u8]) -> IResult<&[u8], TravelDestination<'_>> {
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
pub enum AiPackage<'a> {
    /// `AI_A` — activate a named object.
    Activate { name: &'a L1Str },
    /// `AI_E` — escort to a position (with optional destination cell from `CNDT`).
    Escort {
        position: [f32; 3],
        duration: u16,
        id: &'a L1Str,
        cell: Option<&'a L1Str>,
    },
    /// `AI_F` — follow a target (with optional destination cell from `CNDT`).
    Follow {
        position: [f32; 3],
        duration: u16,
        id: &'a L1Str,
        cell: Option<&'a L1Str>,
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

pub fn ai_activate(input: &[u8]) -> IResult<&[u8], AiPackage<'_>> {
    let (input, name) = fixed_l1str(32)(input)?;
    Ok((input, AiPackage::Activate { name }))
}

/// Shared decoded body of the `AI_E`/`AI_F` packages: (position, duration, id).
type EscortFollow<'a> = ([f32; 3], u16, &'a L1Str);

/// Shared body of the `AI_E` (escort) and `AI_F` (follow) 48-byte packages.
fn ai_escort_follow(input: &[u8]) -> IResult<&[u8], EscortFollow<'_>> {
    let (input, x) = le_f32(input)?;
    let (input, y) = le_f32(input)?;
    let (input, z) = le_f32(input)?;
    let (input, duration) = le_u16(input)?;
    let (input, id) = fixed_l1str(32)(input)?;
    Ok((input, ([x, y, z], duration, id)))
}

pub fn ai_escort(input: &[u8]) -> IResult<&[u8], AiPackage<'_>> {
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

pub fn ai_follow(input: &[u8]) -> IResult<&[u8], AiPackage<'_>> {
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

pub fn ai_travel(input: &[u8]) -> IResult<&[u8], AiPackage<'_>> {
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

pub fn ai_wander(input: &[u8]) -> IResult<&[u8], AiPackage<'_>> {
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
pub struct BipedItem<'a> {
    /// Biped object index (body part slot).
    pub index: u8,
    pub male_model: Option<&'a L1Str>,
    pub female_model: Option<&'a L1Str>,
}

/// Ambient lighting block (`AMBI`, 16 bytes) found in interior CELL records.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AmbientLight {
    pub ambient: Color,
    pub sunlight: Color,
    pub fog: Color,
    pub fog_density: f32,
}

pub fn ambient_light(input: &[u8]) -> IResult<&[u8], AmbientLight> {
    let (input, ambient) = color(input)?;
    let (input, sunlight) = color(input)?;
    let (input, fog) = color(input)?;
    let (input, fog_density) = le_f32(input)?;
    Ok((
        input,
        AmbientLight {
            ambient,
            sunlight,
            fog,
            fog_density,
        },
    ))
}
