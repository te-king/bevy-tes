//! `GMST` — a game setting.

use crate::common::{Subrecord, finish, l1, le_f32, le_i32};
use tes_core::L1Str;
use tes3_esm_derive::TesRecord;

/// A game setting's value. The type is determined by which value subrecord is present;
/// a setting may also have no value at all.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum GmstValue<'a> {
    #[default]
    None,
    Float(f32),
    Int(i32),
    Str(&'a L1Str),
}

#[derive(Debug, Clone, PartialEq, Default, TesRecord)]
#[tes(unmapped = Self::value_field)]
pub struct Gmst<'a> {
    #[tes(tag = b"NAME", decode = l1)]
    pub id: &'a L1Str,
    #[tes(skip)]
    pub value: GmstValue<'a>,
}

impl<'a> Gmst<'a> {
    fn value_field(&mut self, sub: Subrecord<'a>) {
        match &sub.tag.0 {
            b"FLTV" => self.value = GmstValue::Float(finish(le_f32(sub.data)).unwrap_or(0.0)),
            b"INTV" => self.value = GmstValue::Int(finish(le_i32(sub.data)).unwrap_or(0)),
            b"STRV" => self.value = GmstValue::Str(l1(sub.data)),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_tags_win_in_input_order_even_when_malformed() {
        let integer = 17i32.to_le_bytes();
        let float = 2.5f32.to_le_bytes();
        let text = b"borrowed\0";
        let subs = [
            Subrecord {
                tag: (*b"STRV").into(),
                data: text,
            },
            Subrecord {
                tag: (*b"NAME").into(),
                data: b"iSetting\0",
            },
            Subrecord {
                tag: (*b"INTV").into(),
                data: &integer,
            },
            Subrecord {
                tag: (*b"FLTV").into(),
                data: &float,
            },
            Subrecord {
                tag: (*b"INTV").into(),
                data: &[1],
            },
            Subrecord {
                tag: (*b"FLTV").into(),
                data: &[],
            },
            Subrecord {
                tag: (*b"STRV").into(),
                data: text,
            },
            Subrecord {
                tag: (*b"????").into(),
                data: &integer,
            },
        ];
        let expected = [
            GmstValue::Str(l1(text)),
            GmstValue::Str(l1(text)),
            GmstValue::Int(17),
            GmstValue::Float(2.5),
            GmstValue::Int(0),
            GmstValue::Float(0.0),
            GmstValue::Str(l1(text)),
            GmstValue::Str(l1(text)),
        ];
        for (index, value) in expected.into_iter().enumerate() {
            let setting = Gmst::from_subrecords(subs[..=index].iter().copied());
            assert_eq!(setting.value, value);
        }
        let setting = Gmst::from_subrecords(subs.into_iter());
        assert_eq!(setting.id, l1(b"iSetting\0"));
        let GmstValue::Str(value) = setting.value else {
            panic!("expected string")
        };
        assert!(std::ptr::eq(value, l1(text)));
        assert_eq!(
            Gmst::from_subrecords(std::iter::empty()).value,
            GmstValue::None
        );
    }
}
