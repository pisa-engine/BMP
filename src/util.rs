use crate::query::topk_heap::TopKHeap;

#[must_use]
pub fn progress_bar(name: &str, limit: usize) -> indicatif::ProgressBar {
    let pb = indicatif::ProgressBar::new(limit as u64);
    pb.set_draw_delta(limit as u64 / 200);
    pb.set_style(indicatif::ProgressStyle::default_bar().template(
        &format!("{}: {}",name,"{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] ({pos}/{len}, ETA {eta}, SPEED: {per_sec})")));
    pb
}

// Function to convert query results to TREC format and print to stdout
pub fn to_trec(
    query_ids: &[String],
    mut results: Vec<TopKHeap<u16>>,
    doc_lexicon: &[String],
) -> String {
    let mut output = String::new();

    for (id, result) in query_ids.iter().zip(results.iter_mut()) {
        for (rank, r) in result.to_sorted_vec().iter().enumerate() {
            let line = format!(
                "{} Q0 {} {} {} BMP\n",
                id,
                doc_lexicon[r.doc_id.0 as usize],
                rank + 1,
                r.score,
            );
            output.push_str(&line);
        }
    }
    output
}
