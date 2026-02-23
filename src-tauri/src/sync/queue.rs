use std::time::{Duration, Instant};

const BACKOFF_STEPS: &[u64] = &[5, 15, 45, 120, 300, 1800];

pub struct RetryQueue {
    consecutive_failures: usize,
    last_attempt: Option<Instant>,
}

impl RetryQueue {
    pub fn new() -> Self {
        Self {
            consecutive_failures: 0,
            last_attempt: None,
        }
    }

    pub fn should_retry(&self) -> bool {
        let Some(last) = self.last_attempt else {
            return true;
        };
        let delay = self.current_delay();
        last.elapsed() >= delay
    }

    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.last_attempt = Some(Instant::now());
    }

    pub fn record_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_attempt = Some(Instant::now());
    }

    pub fn current_delay(&self) -> Duration {
        let idx = self
            .consecutive_failures
            .saturating_sub(1)
            .min(BACKOFF_STEPS.len() - 1);
        Duration::from_secs(BACKOFF_STEPS[idx])
    }

    pub fn reset(&mut self) {
        self.consecutive_failures = 0;
        self.last_attempt = None;
    }
}

impl Default for RetryQueue {
    fn default() -> Self {
        Self::new()
    }
}
