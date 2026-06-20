//! `REGN` — a world region.

use crate::types::latin1::L1String;
use crate::esm::common::{Color, Subrecord, color, l1, finish, fixed_l1str, le_u8};
use nom::IResult;

/// Per-weather-type spawn chances. Snow/blizzard are only present in v1.3 files.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WeatherChances {
    pub clear: u8,
    pub cloudy: u8,
    pub foggy: u8,
    pub overcast: u8,
    pub rain: u8,
    pub thunder: u8,
    pub ash: u8,
    pub blight: u8,
    pub snow: u8,
    pub blizzard: u8,
}

fn weather(input: &[u8]) -> IResult<&[u8], WeatherChances> {
    // 8 bytes in v1.2, 10 in v1.3; read what's available and leave the rest at 0.
    let mut w = WeatherChances::default();
    let mut input = input;
    let fields: [&mut u8; 10] = [
        &mut w.clear,
        &mut w.cloudy,
        &mut w.foggy,
        &mut w.overcast,
        &mut w.rain,
        &mut w.thunder,
        &mut w.ash,
        &mut w.blight,
        &mut w.snow,
        &mut w.blizzard,
    ];
    for field in fields {
        if input.is_empty() {
            break;
        }
        let (rest, b) = le_u8(input)?;
        *field = b;
        input = rest;
    }
    Ok((input, w))
}

/// A sound that may play in the region (`SNAM`, 33 bytes).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SoundChance {
    pub sound: L1String,
    pub chance: u8,
}

fn sound_chance(input: &[u8]) -> IResult<&[u8], SoundChance> {
    let (input, sound) = fixed_l1str(32)(input)?;
    let (input, chance) = le_u8(input)?;
    Ok((input, SoundChance { sound, chance }))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Regn {
    pub id: L1String,
    pub name: L1String,
    pub weather: WeatherChances,
    /// Creature spawned while sleeping.
    pub sleep_creature: Option<L1String>,
    pub map_color: Color,
    pub sounds: Vec<SoundChance>,
}

impl Regn {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Regn {
        let mut out = Regn::default();
        for sub in subs {
            match &sub.tag {
                b"NAME" => out.id = l1(sub.data),
                b"FNAM" => out.name = l1(sub.data),
                b"WEAT" => out.weather = finish(weather(sub.data)).unwrap_or_default(),
                b"BNAM" => out.sleep_creature = Some(l1(sub.data)),
                b"CNAM" => out.map_color = finish(color(sub.data)).unwrap_or_default(),
                b"SNAM" => {
                    if let Some(sc) = finish(sound_chance(sub.data)) {
                        out.sounds.push(sc);
                    }
                }
                _ => {}
            }
        }
        out
    }
}
