//! `TES3` — the plugin/master file header (always the first record).

use crate::common::{Subrecord, finish, fixed_l1str, l1, le_f32, le_u32, le_u64};
use nom::IResult;
use tes_core::L1String;

/// A master file this plugin depends on (a `MAST`/`DATA` pair).
#[derive(Debug, Clone, PartialEq)]
pub struct Master {
    pub name: L1String,
    /// Size of the master file in bytes, used for version tracking.
    pub size: u64,
}

/// The `TES3` header record.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tes3 {
    /// File format version (1.2 for Morrowind, 1.3 for Tribunal/Bloodmoon).
    pub version: f32,
    /// Header flags; `0x1` means the file is treated as a master regardless of extension.
    pub flags: u32,
    pub company: L1String,
    pub description: L1String,
    /// Number of records following this header.
    pub num_records: u32,
    pub masters: Vec<Master>,
}

/// Decoded `HEDR` fields: (version, flags, company, description, record count).
type HedrFields = (f32, u32, L1String, L1String, u32);

/// Parse the 300-byte `HEDR` payload.
fn hedr(input: &[u8]) -> IResult<&[u8], HedrFields> {
    let (input, version) = le_f32(input)?;
    let (input, flags) = le_u32(input)?;
    let (input, company) = fixed_l1str(32)(input)?;
    let (input, description) = fixed_l1str(256)(input)?;
    let (input, num_records) = le_u32(input)?;
    Ok((input, (version, flags, company, description, num_records)))
}

impl Tes3 {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Tes3 {
        let mut out = Tes3::default();
        let mut pending_master: Option<L1String> = None;
        for sub in subs {
            match &sub.tag.0 {
                b"HEDR" => {
                    if let Some((version, flags, company, description, num_records)) =
                        finish(hedr(sub.data))
                    {
                        out.version = version;
                        out.flags = flags;
                        out.company = company;
                        out.description = description;
                        out.num_records = num_records;
                    }
                }
                b"MAST" => {
                    // A new MAST begins a master entry; flush any dangling one first.
                    if let Some(name) = pending_master.take() {
                        out.masters.push(Master { name, size: 0 });
                    }
                    pending_master = Some(l1(sub.data));
                }
                b"DATA" => {
                    let size = finish(le_u64(sub.data)).unwrap_or(0);
                    if let Some(name) = pending_master.take() {
                        out.masters.push(Master { name, size });
                    }
                }
                _ => {}
            }
        }
        if let Some(name) = pending_master.take() {
            out.masters.push(Master { name, size: 0 });
        }
        out
    }
}
