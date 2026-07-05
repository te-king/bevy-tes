//! A bounds-checked sequential cursor over the raw byte stream.
//!
//! NIF 4.0.0.2 has no block-size table, so every read is sequential and a short read is
//! the only low-level failure mode — [`ReadError`] carries it until a caller attaches
//! context via [`ReadError::at`].

use tes_core::L1String;

use crate::NifError;
use crate::blocks::BlockRef;

/// A simple sequential cursor over the byte stream with bounds-checked little-endian reads.
/// All reads advance the cursor; a short read produces a [`ReadError`].
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

/// A short read while decoding the stream. Carries a description set via [`ReadError::at`].
#[derive(Debug)]
pub(crate) struct ReadError(String);

impl ReadError {
    pub(crate) fn at(self, context: impl Into<String>) -> NifError {
        NifError::Parse(format!("{}: {}", context.into(), self.0))
    }
}

pub(crate) type RResult<T> = Result<T, ReadError>;

impl<'a> Reader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Reader<'a> {
        Reader { data, pos: 0 }
    }

    pub(crate) fn take(&mut self, n: usize) -> RResult<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len());
        match end {
            Some(end) => {
                let slice = &self.data[self.pos..end];
                self.pos = end;
                Ok(slice)
            }
            None => Err(ReadError(format!(
                "unexpected end of data (wanted {n} bytes at offset {})",
                self.pos
            ))),
        }
    }

    /// Read up to and including the next `\n`, returning the bytes before it.
    pub(crate) fn line(&mut self) -> RResult<&'a [u8]> {
        let len = self.data[self.pos..]
            .iter()
            .position(|&b| b == b'\n')
            .ok_or_else(|| ReadError(format!("no newline after offset {}", self.pos)))?;
        let line = self.take(len)?;
        self.skip(1)?; // the newline itself
        Ok(line)
    }

    pub(crate) fn u8(&mut self) -> RResult<u8> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> RResult<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub(crate) fn u32(&mut self) -> RResult<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub(crate) fn i32(&mut self) -> RResult<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub(crate) fn f32(&mut self) -> RResult<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    /// A boolean. In NIF ≤ 4.0.0.2 a `bool` is serialized as a 4-byte `uint`.
    pub(crate) fn boolean(&mut self) -> RResult<bool> {
        Ok(self.u32()? != 0)
    }

    /// A `u32`-length-prefixed string (the framing used by block type names and by every
    /// `SizedString` field; not null-terminated).
    pub(crate) fn string(&mut self) -> RResult<L1String> {
        let len = self.u32()? as usize;
        Ok(L1String::from_bytes(self.take(len)?.to_vec()))
    }

    pub(crate) fn vec3(&mut self) -> RResult<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    pub(crate) fn skip(&mut self, n: usize) -> RResult<()> {
        self.take(n).map(|_| ())
    }

    /// Bytes left after the cursor.
    pub(crate) fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    /// A single block reference (an `i32`, negative meaning "none").
    pub(crate) fn block_ref(&mut self) -> RResult<BlockRef> {
        Ok(BlockRef(self.i32()?))
    }

    /// A `u32` count followed by that many block references.
    pub(crate) fn refs(&mut self) -> RResult<Vec<BlockRef>> {
        let n = self.u32()? as usize;
        (0..n).map(|_| self.block_ref()).collect()
    }
}
