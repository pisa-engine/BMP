#[must_use]
pub fn progress_bar(name: &str, limit: usize) -> indicatif::ProgressBar {
    let pb = indicatif::ProgressBar::new(limit as u64);
    pb.set_draw_delta(limit as u64 / 200);
    pb.set_style(indicatif::ProgressStyle::default_bar().template(
        &format!("{}: {}",name,"{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] ({pos}/{len}, ETA {eta}, SPEED: {per_sec})")));
    pb
}
