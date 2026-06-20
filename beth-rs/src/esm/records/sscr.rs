//! `SSCR` — a start script (a feature added by Tribunal, also used by Bloodmoon).

use crate::esm::common::{Subrecord, l1};
use crate::types::latin1::L1String;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sscr {
    /// Unknown data — a series of ASCII digits.
    pub data: L1String,
    /// Script name (technically optional).
    pub name: Option<L1String>,
}

impl Sscr {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Sscr {
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
