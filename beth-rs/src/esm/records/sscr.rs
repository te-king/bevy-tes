//! `SSCR` — a start script (a feature added by Tribunal, also used by Bloodmoon).

use crate::esm::common::{Subrecord, l1};
use crate::types::latin1::L1Str;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sscr<'a> {
    /// Unknown data — a series of ASCII digits.
    pub data: &'a L1Str,
    /// Script name (technically optional).
    pub name: Option<&'a L1Str>,
}

impl<'a> Sscr<'a> {
    pub fn from_subrecords(subs: impl Iterator<Item = Subrecord<'a>>) -> Sscr<'a> {
        let mut out = Sscr::default();
        for sub in subs {
            match &sub.tag {
                b"DATA" => out.data = l1(sub.data),
                b"NAME" => out.name = Some(l1(sub.data)),
                _ => {}
            }
        }
        out
    }
}
