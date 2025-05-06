use crate::query::cursor::{Cursor, RangeMaxScore, RangeMaxScoreCursor};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum BlockData {
    Compressed(Vec<CompressedBlock>),
    Raw(Vec<u8>),
}

impl Default for BlockData {
    fn default() -> Self {
        BlockData::Raw(Vec::new())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompressedBlock {
    pub max_scores: Vec<(usize, u8)>, // pairs of offset and max score
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PostingList {
    range_maxes: BlockData,
    kth_score: Vec<u8>,
}

impl PostingList {
    pub fn new(range_maxes: BlockData, kth_score: Vec<u8>) -> Self {
        PostingList {
            range_maxes,
            kth_score,
        }
    }

    pub fn kth(&self, k: usize) -> u8 {
        let idx = ((k > 10) as usize) + (k > 100) as usize + (k > 1000) as usize;
        self.kth_score.get(idx).copied().unwrap_or(0)
    }

    pub fn iter(&self, term_id: u32, term_weight: u32) -> PostingListIterator<'_> {
        PostingListIterator::new(self, term_id, term_weight)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PostingListIterator<'a> {
    posting_list: &'a PostingList,
    current: usize,
    term_id: u32,
    term_weight: u32,
}

impl<'a> PostingListIterator<'a> {
    pub fn new(posting_list: &'a PostingList, term_id: u32, term_weight: u32) -> Self {
        PostingListIterator {
            posting_list,
            current: usize::MAX,
            term_id,
            term_weight,
        }
    }

    pub fn kth(&self, k: usize) -> u8 {
        return self.posting_list.kth(k);
    }

    pub fn term_weight(&self) -> u8 {
        self.term_weight as u8
    }

    pub fn term_id(&self) -> u32 {
        self.term_id
    }

    pub fn position(&self) -> usize {
        self.current
    }
}

impl<'a> Cursor for PostingListIterator<'a> {
    type Value = u32;

    type Error = ();

    fn reset(&mut self) {
        self.current = usize::MAX;
    }
}

impl<'a> RangeMaxScoreCursor for PostingListIterator<'a> {
    fn range_max_scores(&self) -> RangeMaxScore {
        match &self.posting_list.range_maxes {
            BlockData::Compressed(compressed_block) => RangeMaxScore::Compressed(compressed_block),
            BlockData::Raw(raw_bytes) => RangeMaxScore::Raw(raw_bytes),
        }
    }
}
