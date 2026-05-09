use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct HangDetector {
    last_output_at: Instant,
    hang_after: Duration,
}

impl HangDetector {
    pub fn new(now: Instant, hang_after: Duration) -> Self {
        Self {
            last_output_at: now,
            hang_after,
        }
    }

    pub fn observe_output(&mut self, now: Instant) {
        self.last_output_at = now;
    }

    pub fn quiet_for(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.last_output_at)
    }

    pub fn is_hung(&self, now: Instant) -> bool {
        self.quiet_for(now) >= self.hang_after
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::HangDetector;

    #[test]
    fn hang_detector_tracks_quiet_processes() {
        let start = Instant::now();
        let mut detector = HangDetector::new(start, Duration::from_secs(30));

        assert!(!detector.is_hung(start + Duration::from_secs(29)));
        assert!(detector.is_hung(start + Duration::from_secs(30)));

        detector.observe_output(start + Duration::from_secs(40));

        assert!(!detector.is_hung(start + Duration::from_secs(60)));
        assert!(detector.is_hung(start + Duration::from_secs(70)));
    }
}
