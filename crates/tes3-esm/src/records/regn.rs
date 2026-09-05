//! `REGN` — a world region.

use crate::common::{Color, Subrecord, color, finish, fixed_l1str, l1, le_u8};
use nom::IResult;
use tes_core::L1Str;
use tes3_esm_derive::{TesPayload, TesRecord};

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
#[derive(Debug, Clone, PartialEq, Default, TesPayload)]
#[tes(parser = sound_chance)]
pub struct SoundChance<'a> {
    #[tes(read = fixed_l1str(32))]
    pub sound: &'a L1Str,
    #[tes(read = le_u8)]
    pub chance: u8,
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
#[tes(unmapped = Self::sound_field)]
pub struct Regn<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(tag = b"FNAM", decode = l1)]
    pub name: &'a L1Str,
    #[tes(tag = b"WEAT", decode = |bytes| finish(weather(bytes)).unwrap_or_default())]
    pub weather: WeatherChances,
    /// Creature spawned while sleeping.
    #[tes(tag = b"BNAM", decode = |bytes| Some(l1(bytes)))]
    pub sleep_creature: Option<&'a L1Str>,
    #[tes(tag = b"CNAM", decode = |bytes| finish(color(bytes)).unwrap_or_default())]
    pub map_color: Color,
    #[tes(skip)]
    pub sounds: Vec<SoundChance<'a>>,
}

impl<'a> Regn<'a> {
    fn sound_field(&mut self, sub: Subrecord<'a>) {
        if &sub.tag.0 == b"SNAM"
            && let Some(sc) = finish(sound_chance(sub.data))
        {
            self.sounds.push(sc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_keeps_available_bytes_and_leaves_excess_input() {
        let bytes = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
        for len in 0..=bytes.len() {
            let (rest, chances) = weather(&bytes[..len]).unwrap();
            let actual = [
                chances.clear,
                chances.cloudy,
                chances.foggy,
                chances.overcast,
                chances.rain,
                chances.thunder,
                chances.ash,
                chances.blight,
                chances.snow,
                chances.blizzard,
            ];
            let mut expected = [0u8; 10];
            let consumed = len.min(10);
            expected[..consumed].copy_from_slice(&bytes[..consumed]);
            assert_eq!(actual, expected);
            assert_eq!(rest, &bytes[consumed..len]);
        }
    }

    #[test]
    fn sounds_skip_malformed_entries_and_borrow_names() {
        let mut sound = [0u8; 33];
        sound[..5].copy_from_slice(b"sound");
        sound[32] = 75;
        let subs = [
            Subrecord {
                tag: (*b"SNAM").into(),
                data: &sound,
            },
            Subrecord {
                tag: (*b"SNAM").into(),
                data: &sound[..32],
            },
            Subrecord {
                tag: (*b"SNAM").into(),
                data: &sound,
            },
        ];
        let region = Regn::from_subrecords(subs.into_iter());
        assert_eq!(region.sounds.len(), 2);
        assert_eq!(region.sounds[0].chance, 75);
        assert_eq!(region.sounds[0], region.sounds[1]);
        assert!(std::ptr::eq(region.sounds[0].sound, l1(&sound[..32])));
    }
}
