//! Pause bookkeeping for the drain thread, kept apart from the loop so it can be tested.
//! Pausing flushes what is buffered once, then discards audio; the paused time is taken off the meeting's clock.
use std::time::Duration;

#[derive(Debug, PartialEq)]
pub enum Step {
    /// normal pass
    Run,
    /// pause just pressed: drain everything buffered, once
    Flush,
    /// paused: discard the buffers and wait
    Skip,
}

#[derive(Default)]
pub struct Pause {
    was: bool,
    total: Duration,
}

impl Pause {
    /// `tick`: time since the previous call. A stop request while paused still runs, so the loop can end.
    pub fn step(&mut self, paused: bool, stopping: bool, tick: Duration) -> Step {
        if paused && self.was && !stopping {
            self.total += tick;
            return Step::Skip;
        }
        let flush = paused && !self.was;
        self.was = paused;
        if flush { Step::Flush } else { Step::Run }
    }

    /// Time the meeting has really been recording: wall time less the pauses.
    pub fn active(&self, elapsed: Duration) -> Duration {
        elapsed.saturating_sub(self.total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const T: Duration = Duration::from_millis(100);

    #[test]
    fn flushes_once_then_skips_and_counts_the_pause() {
        let mut p = Pause::default();
        assert_eq!(p.step(false, false, T), Step::Run);
        assert_eq!(p.step(true, false, T), Step::Flush);
        assert_eq!(p.step(true, false, T), Step::Skip);
        assert_eq!(p.step(true, false, T), Step::Skip);
        assert_eq!(p.step(false, false, T), Step::Run);
        // two skipped ticks come off the clock
        assert_eq!(p.active(Duration::from_secs(10)), Duration::from_millis(9_800));
    }

    #[test]
    fn stopping_while_paused_still_runs() {
        let mut p = Pause::default();
        p.step(true, false, T);
        assert_eq!(p.step(true, true, T), Step::Run);
    }

    #[test]
    fn a_second_pause_flushes_again() {
        let mut p = Pause::default();
        for (paused, want) in [(true, Step::Flush), (true, Step::Skip), (false, Step::Run), (true, Step::Flush)] {
            assert_eq!(p.step(paused, false, T), want);
        }
    }
}
