//! Without headphones the mic hears the other side from the speakers, so the same words land on both tracks.
//! The remote copy is clean, so the mic's copy is the one dropped, and only once the audio shows real leakage.
use std::collections::HashSet;

fn words(t: &str) -> Vec<String> {
    t.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect()
}

/// Share of the mic line's words that also appear in the remote text.
pub fn overlap(mic: &str, remote: &str) -> f32 {
    let m = words(mic);
    let r: HashSet<String> = words(remote).into_iter().collect();
    if m.is_empty() { return 0.0 }
    m.iter().filter(|w| r.contains(*w)).count() as f32 / m.len() as f32
}

/// Is this mic line a copy of what the remote side just said? Short lines ("yes", "okay") are the person's own.
/// `leaking`: the audio showed the mic following the system track.
pub fn is_echo(mic: &str, remote_recent: &str, leaking: bool) -> bool {
    if !leaking { return false }
    let n = words(mic).len();
    n >= 6 && overlap(mic, remote_recent) >= if n >= 8 { 0.6 } else { 0.8 }
}

/// Evidence over the meeting: windows where both sides had sound, and how many of them correlated.
#[derive(Default)]
pub struct Leak { active: u32, correlated: u32 }

impl Leak {
    pub fn add(&mut self, corr: f32) {
        self.active += 1;
        if corr > 0.6 { self.correlated += 1 }
    }
    /// A rate, never a count: two people talking over each other correlate now and then.
    pub fn leaking(&self) -> bool { self.active >= 3 && self.correlated * 10 >= self.active * 4 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_drop_only_with_leak_evidence() {
        let remote = "so we agreed to move the launch to the fifteenth of november";
        let mic = "we agreed to move the launch to the fifteenth";
        assert!(is_echo(mic, remote, true));
        assert!(!is_echo(mic, remote, false));
        assert!(!is_echo("yes I can send it", remote, true));
        assert!(!is_echo("I think the budget needs another review before then", remote, true));
    }

    #[test]
    fn leak_needs_a_rate() {
        let mut l = Leak::default();
        for c in [0.9, 0.2, 0.1, 0.1, 0.0] { l.add(c) }
        assert!(!l.leaking());
        for c in [0.9, 0.8, 0.9] { l.add(c) }
        assert!(l.leaking());
    }
}
