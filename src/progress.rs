use std::io::{self, Write};

pub fn line(stage: &str, done: usize, total: usize) {
    let pct = if total == 0 { 100.0 } else { done as f64 * 100.0 / total as f64 };
    let width = 28usize;
    let filled = ((pct / 100.0) * width as f64).round() as usize;
    print!("\\r{stage} [{}{}] {done}/{total} ({pct:.1}%)", "█".repeat(filled.min(width)), "░".repeat(width.saturating_sub(filled)));
    let _ = io::stdout().flush();
}
