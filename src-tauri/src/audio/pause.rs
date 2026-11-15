//! Pause, as the capture callbacks and the drain thread both see it. The command sets it synchronously, so a quick
//! Pause then Resume can never slip past a busy drain thread: while paused the callbacks keep nothing, and the paused
//! time is taken off the meeting's clock so gap padding does not turn it into silence.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct PauseClock {
    flag: AtomicBool,
    /// when the open pause began, and the total of the finished ones
    state: Mutex<(Option<Instant>, Duration)>,
}

impl PauseClock {
    pub fn set(&self, paused: bool) {
        let mut s = self.state.lock().unwrap();
        match (paused, s.0) {
            (true, None) => s.0 = Some(Instant::now()),
            (false, Some(since)) => { s.1 += since.elapsed(); s.0 = None }
            _ => {}
        }
        self.flag.store(paused, Ordering::Relaxed);
    }

    pub fn is_paused(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    /// All paused time so far, including a pause still open.
    pub fn total(&self) -> Duration {
        let s = self.state.lock().unwrap();
        s.1 + s.0.map_or(Duration::ZERO, |t| t.elapsed())
    }

    /// A new meeting starts with a clean clock.
    pub fn reset(&self) {
        *self.state.lock().unwrap() = (None, Duration::ZERO);
        self.flag.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn counts_finished_and_open_pauses() {
        let c = PauseClock::default();
        assert!(!c.is_paused());
        c.set(true);
        sleep(Duration::from_millis(30));
        assert!(c.is_paused() && c.total() >= Duration::from_millis(30));
        c.set(false);
        let done = c.total();
        sleep(Duration::from_millis(20));
        assert!(!c.is_paused());
        assert_eq!(c.total(), done); // time running is not paused time
        c.set(true);
        sleep(Duration::from_millis(20));
        assert!(c.total() >= done + Duration::from_millis(20));
    }

    #[test]
    fn repeated_calls_do_not_double_count() {
        let c = PauseClock::default();
        c.set(false);
        c.set(true);
        sleep(Duration::from_millis(20));
        c.set(true);
        c.set(false);
        assert!(c.total() < Duration::from_millis(200));
    }

    #[test]
    fn a_quick_pause_and_resume_is_still_recorded() {
        let c = PauseClock::default();
        c.set(true);
        sleep(Duration::from_millis(15));
        c.set(false); // before any drain thread could look
        assert!(c.total() >= Duration::from_millis(15));
        c.reset();
        assert_eq!(c.total(), Duration::ZERO);
    }
}
