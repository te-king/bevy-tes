//! The crate's code-generation macros, gathered in one place.
//!
//! Each macro turns a single declarative table into the repetitive impls the format
//! demands: [`records!`] builds the `Record` enum and its tag dispatch, and
//! [`enum_field!`] builds typed enums for integer discriminant fields.

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
