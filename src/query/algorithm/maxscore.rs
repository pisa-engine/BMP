// use crate::query::cursor::{DocId, MaxScoreCursor};
// use crate::query::topk_heap::TopKHeap;
// use std::ops::{Add, AddAssign};

// const MAX_DOCID: DocId = DocId(u32::MAX);

// /// Calculates cumulative upper bounds by folding over the `postings_lists`.
// fn calc_upper_bounds<C: MaxScoreCursor>(postings_lists: &[C]) -> Vec<C::Score>
// where
//     C::Score: Default + Copy + PartialOrd + AddAssign,
// {
//     postings_lists
//         .iter()
//         .map(C::max_score)
//         .scan(C::Score::default(), |cumsum, max_score| {
//             *cumsum += max_score;
//             Some(*cumsum)
//         })
//         .collect()
// }

// pub struct MaxScore {}

// impl MaxScore {
//     pub fn run<C: MaxScoreCursor>(
//         mut postings_lists: Vec<C>,
//         k: usize,
//         initial_threshold: C::Score,
//     ) -> Result<TopKHeap<C::Score>, C::Error>
//     where
//         C::Score: Default + Copy + PartialOrd + Add<Output = C::Score> + AddAssign,
//     {
//         if postings_lists.is_empty() {
//             return Ok(TopKHeap::new(k));
//         }

//         let mut topk = TopKHeap::with_threshold(k, initial_threshold);

//         // Sort the `postings_lists` in ascending order based on their maximum scores.
//         postings_lists.sort_by(|a, b| {
//             a.max_score()
//                 .partial_cmp(&b.max_score())
//                 .unwrap_or(std::cmp::Ordering::Less)
//         });

//         let upper_bounds: Vec<C::Score> = calc_upper_bounds(&postings_lists);

//         let Some(mut num_non_essential) =
//             upper_bounds.iter().position(|&ub| ub > initial_threshold)
//         else {
//             return Ok(topk);
//         };

//         // we initialize all the cursors so .doc() will be on first document
//         for pl in &mut postings_lists {
//             pl.next();
//         }

//         // find first candidate id to be the smallest id (taking required attributes into consideration)
//         let mut next_doc_id = postings_lists
//             .iter()
//             .map(|pl| pl.docid())
//             .min()
//             .unwrap_or(MAX_DOCID);

//         // Split the `postings_lists` into two mutable slices:
//         // - `non_essential` contains the first `num_non_essential` elements
//         // - `essential` contains the remaining elements

//         let (mut non_essential, mut essential) = postings_lists.split_at_mut(num_non_essential);

//         while next_doc_id != MAX_DOCID {
//             let current_doc_id = next_doc_id;
//             let mut current_score = C::Score::default();
//             next_doc_id = MAX_DOCID;

//             // Iterate over the essential lists, accumulating scores for matching document IDs
//             // and updating the next_doc_id to be the smallest document ID across essential lists.
//             for list in essential.iter_mut() {
//                 if list.docid() == current_doc_id {
//                     current_score += list.score();
//                     list.next();
//                 }
//                 // next doc id is smallest doc id in the essentials lists
//                 if list.docid() < next_doc_id {
//                     next_doc_id = list.docid();
//                 }
//             }

//             // Iterate over the non-essential lists along with their corresponding upper bounds
//             // in reverse order. Stop iterating if the cumulative score plus the upper bound does not
//             // allow entry into the top-k results. Otherwise, update the score by adding the score of
//             // the current document in the list if it matches the current document ID.
//             for (current_list, &current_upper_bound) in non_essential
//                 .iter_mut()
//                 .zip(upper_bounds[..num_non_essential].iter())
//                 .rev()
//             {
//                 if !topk.would_enter(current_score + current_upper_bound) {
//                     break;
//                 }
//                 current_list.next_geq(current_doc_id);
//                 if current_list.docid() == current_doc_id {
//                     current_score += current_list.score();
//                 }
//             }

//             // Attempt to insert the current document ID and score into the top-k results.
//             // If successful, find the position of the first upper bound greater than the new threshold
//             // among the upper bounds corresponding to non-essential lists. If found, update the
//             // number of non-essential lists and split the mutable `postings_lists` accordingly.
//             if let Some(updated_threshold) = topk.insert(current_doc_id, current_score) {
//                 if let Some(pos) = upper_bounds[num_non_essential..]
//                     .iter()
//                     .position(|&ub| ub > updated_threshold)
//                 {
//                     if pos != 0 {
//                         num_non_essential += pos;
//                         (non_essential, essential) = postings_lists.split_at_mut(num_non_essential);
//                     }
//                 } else {
//                     break;
//                 }
//             }
//         }
//         Ok(topk)
//     }
// }

// // #[cfg(test)]
// // mod test {
// //     use super::*;
// //     use crate::index::posting_list::{PostingList, PostingListIterator};

// //     #[test]
// //     fn test_cumulative_sums() {
// //         let posting_lists = vec![
// //             PostingList::new(vec![], 8.0),
// //             PostingList::new(vec![], 10.0),
// //             PostingList::new(vec![], 20.0),
// //         ];
// //         let iterators: Vec<PostingListIterator> = posting_lists
// //             .iter()
// //             .map(|item| PostingListIterator::new(item, 1_f32))
// //             .collect();
// //         assert_eq!(calc_upper_bounds(&iterators), vec![8.0, 18.0, 38.0]);
// //     }
// // }
