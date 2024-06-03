/// A type-safe representation of a document ID.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DocId(pub u32);

/// Cursor over a posting list.
pub trait Cursor {
    type Value;
    type Error;
    fn reset(&mut self);
}

pub enum RangeMaxScore<'a> {
    Compressed(&'a [crate::index::posting_list::CompressedBlock]),
    Raw(&'a [u8]),
}

pub trait RangeMaxScoreCursor: Cursor {
    fn range_max_scores(&self) -> RangeMaxScore;
}
