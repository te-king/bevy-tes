//! `CELL` — an interior or exterior cell, including its object references.
//!
//! After the cell's own header fields, a CELL contains a flat list of object
//! references. Each reference begins with an `FRMR` id and a `NAME` object id, followed
//! by a variable set of fields describing that placement. Cells may also contain
//! "moved references" (`MVRF`-led) recording objects relocated from another cell.

use crate::common::{
    Color, Subrecord, color, finish, l1, le_f32, le_i32, le_u32, parse_or_default,
};
use crate::shared::{AmbientLight, TravelDestination, ambient_light, travel_destination};
use nom::IResult;
use tes_core::L1String;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CellData {
    /// `0x01` = Interior, `0x02` = Has Water, `0x04` = No Sleeping, `0x80` = like exterior.
    pub flags: u32,
    pub grid_x: i32,
    pub grid_y: i32,
}

fn cell_data(input: &[u8]) -> IResult<&[u8], CellData> {
    let (input, flags) = le_u32(input)?;
    let (input, grid_x) = le_i32(input)?;
    let (input, grid_y) = le_i32(input)?;
    Ok((
        input,
        CellData {
            flags,
            grid_x,
            grid_y,
        },
    ))
}

/// Position + rotation of a placed reference (`DATA`, 24 bytes; rotations in radians).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReferenceTransform {
    pub position: [f32; 3],
    pub rotation: [f32; 3],
}

fn reference_transform(input: &[u8]) -> IResult<&[u8], ReferenceTransform> {
    let (input, px) = le_f32(input)?;
    let (input, py) = le_f32(input)?;
    let (input, pz) = le_f32(input)?;
    let (input, rx) = le_f32(input)?;
    let (input, ry) = le_f32(input)?;
    let (input, rz) = le_f32(input)?;
    Ok((
        input,
        ReferenceTransform {
            position: [px, py, pz],
            rotation: [rx, ry, rz],
        },
    ))
}

/// A single object reference placed within a cell.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reference {
    /// `FRMR` reference id.
    pub id: u32,
    /// `NAME` object id.
    pub object: L1String,
    pub blocked: Option<u8>,
    pub scale: Option<f32>,
    pub owner_npc: Option<L1String>,
    pub owner_global: Option<L1String>,
    pub owner_faction: Option<L1String>,
    pub owner_faction_rank: Option<u32>,
    pub soul: Option<L1String>,
    pub enchant_charge: Option<f32>,
    pub remaining_usage: Option<u32>,
    pub value: Option<u32>,
    pub destinations: Vec<TravelDestination>,
    pub lock_level: Option<u32>,
    pub key: Option<L1String>,
    pub trap: Option<L1String>,
    pub disabled: Option<u8>,
    pub transform: Option<ReferenceTransform>,
}

/// A reference that was relocated from another cell.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MovedReference {
    /// `MVRF` reference id (matches the moved reference's `FRMR`).
    pub reference_id: u32,
    /// Destination interior cell name (`CNAM`).
    pub dest_cell: Option<L1String>,
    /// Destination exterior grid (`CNDT`).
    pub dest_grid: Option<(i32, i32)>,
    /// The form reference that was moved.
    pub reference: Reference,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cell {
    /// Cell name (human-readable; may be empty for exterior cells).
    pub name: L1String,
    pub data: CellData,
    pub region: Option<L1String>,
    pub map_color: Option<Color>,
    pub water_height: Option<f32>,
    pub ambient: Option<AmbientLight>,
    pub references: Vec<Reference>,
    pub moved_references: Vec<MovedReference>,
}

/// Tracks where in the CELL layout the sequential scan currently is.
enum Phase {
    Header,
    Reference,
    MovedHeader,
}

/// Flush the in-progress reference into either the plain or moved-reference list.
fn flush(
    out: &mut Cell,
    current: &mut Option<Reference>,
    pending_move: &mut Option<MovedReference>,
) {
    if let Some(reference) = current.take() {
        if let Some(mut moved) = pending_move.take() {
            moved.reference = reference;
            out.moved_references.push(moved);
        } else {
            out.references.push(reference);
        }
    }
}

impl Cell {
    pub fn from_subrecords<'a>(subs: impl Iterator<Item = Subrecord<'a>>) -> Cell {
        let mut out = Cell::default();
        let mut phase = Phase::Header;
        let mut current: Option<Reference> = None;
        let mut pending_move: Option<MovedReference> = None;

        for sub in subs {
            match &sub.tag {
                b"FRMR" => {
                    flush(&mut out, &mut current, &mut pending_move);
                    current = Some(Reference {
                        id: finish(le_u32(sub.data)).unwrap_or(0),
                        ..Reference::default()
                    });
                    phase = Phase::Reference;
                }
                b"MVRF" => {
                    flush(&mut out, &mut current, &mut pending_move);
                    pending_move = Some(MovedReference {
                        reference_id: finish(le_u32(sub.data)).unwrap_or(0),
                        ..MovedReference::default()
                    });
                    phase = Phase::MovedHeader;
                }
                _ => match phase {
                    Phase::Header => header_field(&mut out, &sub),
                    Phase::MovedHeader => moved_header_field(pending_move.as_mut(), &sub),
                    Phase::Reference => reference_field(current.as_mut(), &sub),
                },
            }
        }
        flush(&mut out, &mut current, &mut pending_move);
        out
    }
}

fn header_field(out: &mut Cell, sub: &Subrecord<'_>) {
    match &sub.tag {
        b"NAME" => out.name = l1(sub.data),
        b"DATA" => out.data = parse_or_default(cell_data, sub.data),
        b"RGNN" => out.region = Some(l1(sub.data)),
        b"NAM5" => out.map_color = finish(color(sub.data)),
        b"WHGT" => out.water_height = finish(le_f32(sub.data)),
        b"AMBI" => out.ambient = finish(ambient_light(sub.data)),
        _ => {}
    }
}

fn moved_header_field(moved: Option<&mut MovedReference>, sub: &Subrecord<'_>) {
    let Some(moved) = moved else { return };
    match &sub.tag {
        b"CNAM" => moved.dest_cell = Some(l1(sub.data)),
        b"CNDT" => {
            if let Some((x, y)) = finish(grid(sub.data)) {
                moved.dest_grid = Some((x, y));
            }
        }
        _ => {}
    }
}

fn grid(input: &[u8]) -> IResult<&[u8], (i32, i32)> {
    let (input, x) = le_i32(input)?;
    let (input, y) = le_i32(input)?;
    Ok((input, (x, y)))
}

fn reference_field(reference: Option<&mut Reference>, sub: &Subrecord<'_>) {
    let Some(r) = reference else { return };
    match &sub.tag {
        b"NAME" => r.object = l1(sub.data),
        b"UNAM" => r.blocked = sub.data.first().copied(),
        b"XSCL" => r.scale = finish(le_f32(sub.data)),
        b"ANAM" => r.owner_npc = Some(l1(sub.data)),
        b"BNAM" => r.owner_global = Some(l1(sub.data)),
        b"CNAM" => r.owner_faction = Some(l1(sub.data)),
        b"INDX" => r.owner_faction_rank = finish(le_u32(sub.data)),
        b"XSOL" => r.soul = Some(l1(sub.data)),
        b"XCHG" => r.enchant_charge = finish(le_f32(sub.data)),
        b"INTV" => r.remaining_usage = finish(le_u32(sub.data)),
        b"NAM9" => r.value = finish(le_u32(sub.data)),
        b"DODT" => {
            if let Some(dest) = finish(travel_destination(sub.data)) {
                r.destinations.push(dest);
            }
        }
        b"DNAM" => {
            if let Some(last) = r.destinations.last_mut() {
                last.cell = Some(l1(sub.data));
            }
        }
        b"FLTV" => r.lock_level = finish(le_u32(sub.data)),
        b"KNAM" => r.key = Some(l1(sub.data)),
        b"TNAM" => r.trap = Some(l1(sub.data)),
        b"ZNAM" => r.disabled = sub.data.first().copied(),
        b"DATA" => r.transform = finish(reference_transform(sub.data)),
        _ => {}
    }
}
