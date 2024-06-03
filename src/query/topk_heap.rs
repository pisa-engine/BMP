use crate::query::cursor::DocId;
use std::cmp::Ordering;

#[derive(Debug, Clone)]
pub struct Entry<S> {
    pub doc_id: DocId,
    pub score: S,
}

impl<S: Copy + PartialOrd> Ord for Entry<S> {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .partial_cmp(&self.score)
            .unwrap_or_else(|| other.doc_id.cmp(&self.doc_id))
    }
}

impl<S: Copy + PartialOrd> PartialOrd for Entry<S> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<S: Copy + PartialOrd> PartialEq for Entry<S> {
    fn eq(&self, other: &Self) -> bool {
        self.score
            .partial_cmp(&other.score)
            .unwrap_or_else(|| other.doc_id.cmp(&self.doc_id))
            == Ordering::Equal
    }
}

impl<S: Copy + PartialOrd> Eq for Entry<S> {}

#[derive(Clone)]
pub struct TopKHeap<S> {
    // BinaryHeap to efficiently manage the K largest elements.
    heap: std::collections::BinaryHeap<Entry<S>>,
    // The current threshold value used to determine if a new score would qualify for entry.
    threshold: S,
    // The maximum number of elements to be maintained in the heap.
    k: usize,
}

impl<S: Default + Copy + PartialOrd> TopKHeap<S> {
    /// Creates a new `TopKHeap` with the specified K value.
    pub fn new(k: usize) -> Self {
        // Initialize the heap with a default threshold of 0.0.
        Self::with_threshold(k, S::default())
    }

    /// Creates a new `TopKHeap` with the specified K value and initial threshold.
    pub fn with_threshold(k: usize, threshold: S) -> Self {
        Self {
            heap: std::collections::BinaryHeap::with_capacity(k),
            threshold,
            k,
        }
    }

    /// Gets the current threshold value.
    pub fn threshold(&self) -> S {
        self.threshold
    }

    /// Determines if a given score would qualify for entry into the top-k heap.
    pub fn would_enter(&self, score: S) -> bool {
        score > self.threshold
    }

    /// Inserts a document with its score into the top-k heap.
    ///
    /// If the score does not qualify for entry, returns `None`.
    /// Otherwise, inserts the entry into the heap and updates the threshold.
    /// Returns the new threshold if the heap size reaches or exceeds k, otherwise returns `None`.
    pub fn insert(&mut self, doc_id: DocId, score: S) -> Option<S> {
        // Check if the score qualifies for entry into the top-k heap.
        if !self.would_enter(score) {
            return None;
        }
        // Push the new entry into the heap.
        self.heap.push(Entry { doc_id, score });
        // Check if the heap size is within the top-k limit.
        if self.heap.len() <= self.k {
            // If the heap size equals k, update the threshold and return it.
            if self.heap.len() == self.k {
                self.threshold = self.heap.peek().unwrap().score;
                Some(self.threshold)
            } else {
                None
            }
        } else {
            self.heap.pop();
            self.threshold = self.heap.peek().unwrap().score;
            Some(self.threshold)
        }
    }

    /// Converts the TopKHeap into a sorted vector of entries.
    ///
    /// This method efficiently pops elements from the heap until it is empty,
    /// then reverses the vector to obtain the elements in descending order.
    pub fn to_sorted_vec(&mut self) -> Vec<Entry<S>> {
        let mut sorted_vec = Vec::with_capacity(self.heap.len());
        while let Some(elem) = self.heap.pop() {
            sorted_vec.push(elem);
        }
        sorted_vec.reverse();
        sorted_vec
    }
}
