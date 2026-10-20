pub mod engine;

use engine::TranscriptChunk;
use std::sync::Mutex;

pub const LOOKBACK_MS: u64 = 45_000;
pub const LOOKAHEAD_MS: u64 = 10_000;

pub type Transcript = Mutex<Vec<TranscriptChunk>>;

/// Converts millisecond-based session timestamp to seconds for Foundry Engine.
pub fn ms_to_secs(ms: u64) -> f64 {
    ms as f64 / 1000.0
}

/// Calculates the temporal window bounds [t_min, t_max] for a given timestamp in ms.
pub fn calculate_window_bounds(ts_ms: u64) -> (f64, f64) {
    let t_min = (ms_to_secs(ts_ms.saturating_sub(LOOKBACK_MS))).max(0.0);
    let t_max = ms_to_secs(ts_ms + LOOKAHEAD_MS);
    (t_min, t_max)
}

/// Slices the temporal lookback window [ts - 45s, ts + 10s].
pub fn get_window_chunks(tr: &Transcript, ts_ms: u64) -> Vec<TranscriptChunk> {
    let t_curr = ms_to_secs(ts_ms);
    let (t_min, t_max) = calculate_window_bounds(ts_ms);

    let history = tr.lock().unwrap();
    let chunks: Vec<TranscriptChunk> = history
        .iter()
        .filter(|c| c.end >= t_min && c.start <= t_max)
        .cloned()
        .collect();

    eprintln!(
        "[Syntari AI Window] Note ts={:.1}s -> window [{:.1}s, {:.1}s] ({} chunks matched)",
        t_curr, t_min, t_max, chunks.len()
    );
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_window_normal() {
        // T = 60.0s -> 60_000ms
        let (t_min, t_max) = calculate_window_bounds(60_000);
        assert_eq!(t_min, 15.0);
        assert_eq!(t_max, 70.0);
    }

    #[test]
    fn test_context_window_boundary_clamped() {
        // T = 10.0s -> 10_000ms, clamped to 0.0
        let (t_min, t_max) = calculate_window_bounds(10_000);
        assert_eq!(t_min, 0.0);
        assert_eq!(t_max, 20.0);
    }
}

