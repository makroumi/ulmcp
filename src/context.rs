//! Context window tracking and token budget management.

use ulmen_core::tokens;

/// Tracks token usage within a context window.
#[derive(Debug, Clone)]
pub struct ContextTracker {
    pub budget: usize,
    pub used: usize,
    pub reserved: usize,
}

impl ContextTracker {
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            used: 0,
            reserved: 0,
        }
    }

    pub fn available(&self) -> usize {
        self.budget.saturating_sub(self.used + self.reserved)
    }

    pub fn use_tokens(&mut self, n: usize) -> bool {
        if self.used + n + self.reserved > self.budget {
            false
        } else {
            self.used += n;
            true
        }
    }

    pub fn reserve(&mut self, n: usize) -> bool {
        if self.used + self.reserved + n > self.budget {
            false
        } else {
            self.reserved += n;
            true
        }
    }

    pub fn release_reserve(&mut self, n: usize) {
        self.reserved = self.reserved.saturating_sub(n);
    }

    pub fn usage_ratio(&self) -> f64 {
        if self.budget == 0 {
            return 0.0;
        }
        (self.used + self.reserved) as f64 / self.budget as f64
    }

    pub fn estimate_text(&self, text: &str) -> usize {
        tokens::count_tokens(text)
    }

    /// Check if text fits in remaining budget.
    pub fn fits(&self, text: &str) -> bool {
        tokens::count_tokens(text) <= self.available()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_tracking() {
        let mut ctx = ContextTracker::new(1000);
        assert_eq!(ctx.available(), 1000);
        assert!(ctx.use_tokens(200));
        assert_eq!(ctx.available(), 800);
        assert_eq!(ctx.used, 200);
    }

    #[test]
    fn budget_exceeded() {
        let mut ctx = ContextTracker::new(100);
        assert!(ctx.use_tokens(90));
        assert!(!ctx.use_tokens(20)); // would exceed
        assert_eq!(ctx.used, 90); // unchanged
    }

    #[test]
    fn reservation() {
        let mut ctx = ContextTracker::new(100);
        assert!(ctx.reserve(30));
        assert_eq!(ctx.available(), 70);
        assert!(ctx.use_tokens(60));
        assert_eq!(ctx.available(), 10);
        assert!(!ctx.use_tokens(20)); // would exceed with reservation
    }

    #[test]
    fn release_reserve() {
        let mut ctx = ContextTracker::new(100);
        ctx.reserve(50);
        assert_eq!(ctx.available(), 50);
        ctx.release_reserve(30);
        assert_eq!(ctx.available(), 80);
    }

    #[test]
    fn usage_ratio() {
        let mut ctx = ContextTracker::new(100);
        ctx.use_tokens(75);
        assert!((ctx.usage_ratio() - 0.75).abs() < 0.01);
    }

    #[test]
    fn fits_text() {
        let ctx = ContextTracker::new(10000);
        assert!(ctx.fits("hello world"));
    }
}
