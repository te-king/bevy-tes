//! The crate's code-generation macros, gathered in one place.
//!
//! Each macro turns a single declarative table into the repetitive impls the format
//! demands: [`records!`] builds the `Record` enum and its tag dispatch, [`parse_struct!`]
//! builds fixed-layout subrecord parsers, and [`enum_field!`] builds typed enums for
//! integer discriminant fields.

/// Generate the [`Record`](crate::Record) enum and its tag dispatch from one
/// `Variant(Type) = b"TAG"` table, so each record type is listed exactly once instead of
/// three times (variant, tag accessor, parser dispatch).
macro_rules! records {
    ($( $variant:ident($ty:ty) = $tag:literal, )*) => {
        /// A single parsed record. One variant per known TES3 record type, plus
        /// [`Record::Unknown`] as a safety net for tags not modeled by this crate.
        /// Records own their data and are `'static`.
        #[derive(Debug, Clone, PartialEq)]
        pub enum Record {
            $( $variant($ty), )*
            /// A record whose 4-byte tag is not recognized; its raw payload is preserved.
            Unknown {
                tag: Tag,
                flags: RecordFlags,
                data: Vec<u8>,
            },
        }

        impl Record {
            /// The 4-byte tag of this record.
            pub fn tag(&self) -> Tag {
                match self {
                    $( Record::$variant(_) => Tag(*$tag), )*
                    Record::Unknown { tag, .. } => *tag,
                }
            }

            /// Build a typed record from its tag, header flags and data block.
            fn from_parts(tag: Tag, flags: RecordFlags, data: &[u8]) -> Record {
                // Subrecords are parsed lazily from `data`; a malformed/truncated
                // subrecord just ends iteration (the record keeps whatever fields parsed
                // before it). Only one match arm runs, so moving `subs` into it is fine.
                let subs = Subrecords::new(data);
                match &tag.0 {
                    $( $tag => Record::$variant(<$ty>::from_subrecords(subs)), )*
                    _ => Record::Unknown {
                        tag,
                        flags,
                        data: data.to_vec(),
                    },
                }
            }
        }
    };
}
pub(crate) use records;

/// Generate a nom parser for a fixed-layout subrecord struct from a single
/// `field: parser` table, so parse order and struct construction cannot drift apart
/// (the struct definition stays hand-written — it carries the field docs):
///
/// ```ignore
/// parse_struct! {
///     fn misc_data -> MiscData {
///         weight: le_f32,
///         value: le_u32,
///         flags: le_u32,
///     }
/// }
/// ```
///
/// Parsers are any `Fn(&[u8]) -> IResult<&[u8], T>` expression (`le_u32`,
/// `fixed_l1str(32)`, a local helper, …). The `fn` takes an optional visibility
/// (`pub fn` for the parsers `shared` exports). Layouts with padding to skip, computed
/// fields or loops don't fit and stay hand-written.
macro_rules! parse_struct {
    ($(#[$meta:meta])* $vis:vis fn $name:ident -> $ty:ident {
        $( $field:ident : $parser:expr ),+ $(,)?
    }) => {
        $(#[$meta])*
        $vis fn $name(input: &[u8]) -> nom::IResult<&[u8], $ty> {
            $( let (input, $field) = ($parser)(input)?; )+
            Ok((input, $ty { $( $field ),+ }))
        }
    };
}
pub(crate) use parse_struct;

/// Generate a typed enum for an integer discriminant field from one `Variant = value`
/// table: `From` conversions in both directions (unmodeled values round-trip verbatim
/// through an `Unknown(raw)` variant), a `Default` of the zero value, and the
/// [`EnumField`](crate::common::EnumField) hook that lets
/// [`enumeration`](crate::common::enumeration) parse it straight into a struct field.
macro_rules! enum_field {
    ($(#[$meta:meta])* $vis:vis enum $name:ident: $bits:ty {
        $( $(#[$vmeta:meta])* $variant:ident = $value:literal ),+ $(,)?
    }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $name {
            $( $(#[$vmeta])* $variant, )+
            /// A value not modeled by this crate; preserved verbatim.
            Unknown($bits),
        }

        impl From<$bits> for $name {
            fn from(value: $bits) -> $name {
                match value {
                    $( $value => $name::$variant, )+
                    other => $name::Unknown(other),
                }
            }
        }

        impl From<$name> for $bits {
            fn from(value: $name) -> $bits {
                match value {
                    $( $name::$variant => $value, )+
                    $name::Unknown(other) => other,
                }
            }
        }

        impl Default for $name {
            fn default() -> $name {
                $name::from(0 as $bits)
            }
        }

        impl $crate::common::EnumField for $name {
            type Bits = $bits;
            fn from_bits(bits: $bits) -> $name {
                $name::from(bits)
            }
        }
    };
}
pub(crate) use enum_field;
