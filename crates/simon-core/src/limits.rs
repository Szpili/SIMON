//! I5 (M7) — resource limits at node boundaries. Deterministic failure.

use serde::{Deserialize, Serialize};

/// Resource limits for a single job request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub deadline_us: u64,
    #[serde(rename = "max_tokens", alias = "max_tokenow")]
    pub max_tokens: u32,
    #[serde(rename = "max_bytes", alias = "max_bajtow")]
    pub max_bytes: usize,
    #[serde(rename = "max_tool_steps", alias = "max_krokow_narzedzi")]
    pub max_tool_steps: u32,
}

pub type Limity = ResourceLimits;

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            deadline_us: 120_000_000, // 120 s
            max_tokens: 4096,
            max_bytes: 8 * 1024 * 1024,
            max_tool_steps: 16,
        }
    }
}

/// Reason for limit abortion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitExceeded {
    Deadline,
    Tokens,
    Bytes,
    ToolSteps,
    Cancelled,
}

pub type Przekroczenie = LimitExceeded;

pub use LimitExceeded::{
    Bytes as Bajty, Cancelled as Anulowane, Tokens as Tokeny, ToolSteps as KrokiNarzedzi,
};

/// Live budget tracking against limits.
#[derive(Debug, Clone)]
pub struct Budget {
    limits: ResourceLimits,
    start_us: u64,
    tokens: u32,
    bytes: usize,
    steps: u32,
    cancelled: bool,
}

pub type Budzet = Budget;

impl Budget {
    pub fn new(limits: ResourceLimits, now_us: u64) -> Self {
        Self {
            limits,
            start_us: now_us,
            tokens: 0,
            bytes: 0,
            steps: 0,
            cancelled: false,
        }
    }

    pub fn nowy(limity: ResourceLimits, teraz_us: u64) -> Self {
        Self::new(limity, teraz_us)
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    pub fn anuluj(&mut self) {
        self.cancel();
    }

    pub fn check(&self, now_us: u64) -> Result<(), LimitExceeded> {
        if self.cancelled {
            return Err(LimitExceeded::Cancelled);
        }
        if now_us.saturating_sub(self.start_us) > self.limits.deadline_us {
            return Err(LimitExceeded::Deadline);
        }
        Ok(())
    }

    pub fn sprawdz(&self, teraz_us: u64) -> Result<(), LimitExceeded> {
        self.check(teraz_us)
    }

    pub fn add_tokens(&mut self, n: u32, now_us: u64) -> Result<(), LimitExceeded> {
        self.check(now_us)?;
        self.tokens = self.tokens.saturating_add(n);
        if self.tokens > self.limits.max_tokens {
            return Err(LimitExceeded::Tokens);
        }
        Ok(())
    }

    pub fn dodaj_tokeny(&mut self, n: u32, teraz_us: u64) -> Result<(), LimitExceeded> {
        self.add_tokens(n, teraz_us)
    }

    pub fn add_bytes(&mut self, n: usize, now_us: u64) -> Result<(), LimitExceeded> {
        self.check(now_us)?;
        self.bytes = self.bytes.saturating_add(n);
        if self.bytes > self.limits.max_bytes {
            return Err(LimitExceeded::Bytes);
        }
        Ok(())
    }

    pub fn dodaj_bajty(&mut self, n: usize, teraz_us: u64) -> Result<(), LimitExceeded> {
        self.add_bytes(n, teraz_us)
    }

    pub fn tool_step(&mut self, now_us: u64) -> Result<(), LimitExceeded> {
        self.check(now_us)?;
        self.steps = self.steps.saturating_add(1);
        if self.steps > self.limits.max_tool_steps {
            return Err(LimitExceeded::ToolSteps);
        }
        Ok(())
    }

    pub fn krok_narzedzia(&mut self, teraz_us: u64) -> Result<(), LimitExceeded> {
        self.tool_step(teraz_us)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_aborts_deterministically() {
        let b = Budget::new(ResourceLimits::default(), 0);
        assert!(b.check(100_000_000).is_ok());
        assert_eq!(b.check(120_000_001), Err(LimitExceeded::Deadline));
    }

    #[test]
    fn token_byte_step_budgets_abort_at_limit() {
        let mut b = Budget::new(ResourceLimits::default(), 0);
        assert!(b.add_tokens(4096, 0).is_ok());
        assert_eq!(b.add_tokens(1, 0), Err(LimitExceeded::Tokens));

        let mut b2 = Budget::new(ResourceLimits::default(), 0);
        assert!(b2.add_bytes(8 * 1024 * 1024, 0).is_ok());
        assert_eq!(b2.add_bytes(1, 0), Err(LimitExceeded::Bytes));

        let mut b3 = Budget::new(ResourceLimits::default(), 0);
        for _ in 0..16 {
            assert!(b3.tool_step(0).is_ok());
        }
        assert_eq!(b3.tool_step(0), Err(LimitExceeded::ToolSteps));
    }

    #[test]
    fn cancellation_wins_over_others() {
        let mut b = Budget::new(ResourceLimits::default(), 0);
        b.cancel();
        assert_eq!(b.check(0), Err(LimitExceeded::Cancelled));
        assert_eq!(b.add_tokens(10, 0), Err(LimitExceeded::Cancelled));
        assert_eq!(b.add_bytes(10, 0), Err(LimitExceeded::Cancelled));
        assert_eq!(b.tool_step(0), Err(LimitExceeded::Cancelled));
    }
}
