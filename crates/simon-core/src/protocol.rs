//! #4 (M7) — protocol hardening: **replay (nonce)**, **rate-limit**, **message size limit**,
//! cancellation propagation. Deterministic, network-free, CPU-testable.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Maximum size of a single protocol message (bytes).
pub const MAX_MESSAGE_BYTES: usize = 1 << 20; // 1 MiB
pub const MAX_WIADOMOSC_BAJTOW: usize = MAX_MESSAGE_BYTES;

/// Protocol rejection error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolError {
    ReplayNonce,
    NonceTooOld,
    MessageTooLarge,
    RateLimit,
}

pub type BladProtokolu = ProtocolError;

pub use ProtocolError::{
    MessageTooLarge as WiadomoscZaDuza, NonceTooOld as NonceZaStary, ReplayNonce as NoncePowtorzony,
};

/// Anti-replay window. Accepts nonces in window `(max - window, …]`, each **only once**.
#[derive(Debug, Clone)]
pub struct NonceWindow {
    max: Option<u64>,
    window: u64,
    seen: BTreeSet<u64>,
}

pub type OknoNonce = NonceWindow;

impl NonceWindow {
    pub fn new(window: u64) -> Self {
        Self {
            max: None,
            window,
            seen: BTreeSet::new(),
        }
    }

    pub fn nowy(okno: u64) -> Self {
        Self::new(okno)
    }

    /// Accepts nonce or returns rejection error.
    pub fn accept(&mut self, nonce: u64) -> Result<(), ProtocolError> {
        if let Some(m) = self.max {
            if nonce <= m.saturating_sub(self.window) {
                return Err(ProtocolError::NonceTooOld);
            }
        }
        if !self.seen.insert(nonce) {
            return Err(ProtocolError::ReplayNonce);
        }
        if self.max.map(|m| nonce > m).unwrap_or(true) {
            self.max = Some(nonce);
        }
        if let Some(m) = self.max {
            let boundary = m.saturating_sub(self.window);
            self.seen.retain(|n| *n > boundary);
        }
        Ok(())
    }

    pub fn przyjmij(&mut self, nonce: u64) -> Result<(), ProtocolError> {
        self.accept(nonce)
    }
}

/// Message size check.
pub fn check_message_size(n: usize) -> Result<(), ProtocolError> {
    if n > MAX_MESSAGE_BYTES {
        Err(ProtocolError::MessageTooLarge)
    } else {
        Ok(())
    }
}

pub use check_message_size as sprawdz_rozmiar;

/// Token bucket for rate limiting: `capacity` = burst, `rate` = tokens/s.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    capacity: f64,
    rate: f64,
    available: f64,
    last_us: u64,
}

pub type Kubelek = TokenBucket;

impl TokenBucket {
    pub fn new(capacity: f64, rate_per_sec: f64, now_us: u64) -> Self {
        Self {
            capacity,
            rate: rate_per_sec,
            available: capacity,
            last_us: now_us,
        }
    }

    pub fn nowy(pojemnosc: f64, tempo_na_s: f64, teraz_us: u64) -> Self {
        Self::new(pojemnosc, tempo_na_s, teraz_us)
    }

    /// Allows one message or returns `RateLimit`.
    pub fn allow(&mut self, now_us: u64) -> Result<(), ProtocolError> {
        let dt = now_us.saturating_sub(self.last_us) as f64 / 1_000_000.0;
        self.last_us = self.last_us.max(now_us);
        self.available = (self.available + dt * self.rate).min(self.capacity);
        if self.available >= 1.0 {
            self.available -= 1.0;
            Ok(())
        } else {
            Err(ProtocolError::RateLimit)
        }
    }

    pub fn zezwol(&mut self, teraz_us: u64) -> Result<(), ProtocolError> {
        self.allow(teraz_us)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_replay_and_old_rejected() {
        let mut w = NonceWindow::new(10);
        assert!(w.accept(100).is_ok());
        assert_eq!(w.accept(100), Err(ProtocolError::ReplayNonce));
        assert!(w.accept(105).is_ok());
        assert!(w.accept(96).is_ok());
        assert_eq!(w.accept(95), Err(ProtocolError::NonceTooOld));
        assert_eq!(w.accept(80), Err(ProtocolError::NonceTooOld));
    }

    #[test]
    fn message_size_rejects_oversized() {
        assert!(check_message_size(1024).is_ok());
        assert!(check_message_size(MAX_MESSAGE_BYTES).is_ok());
        assert_eq!(
            check_message_size(MAX_MESSAGE_BYTES + 1),
            Err(ProtocolError::MessageTooLarge)
        );
    }

    #[test]
    fn bucket_allows_burst_then_rate_limits() {
        let mut b = TokenBucket::new(3.0, 1.0, 0);
        assert!(b.allow(0).is_ok());
        assert!(b.allow(0).is_ok());
        assert!(b.allow(0).is_ok());
        assert_eq!(b.allow(0), Err(ProtocolError::RateLimit));
        assert_eq!(b.allow(500_000), Err(ProtocolError::RateLimit));
        assert!(b.allow(1_000_000).is_ok());
        assert_eq!(b.allow(1_000_000), Err(ProtocolError::RateLimit));
    }

    #[test]
    fn bucket_resilient_to_backward_clock_jump() {
        let mut b = TokenBucket::new(2.0, 1.0, 1_000_000);
        assert!(b.allow(1_000_000).is_ok());
        assert_eq!(b.allow(500_000), Ok(()));
        assert_eq!(b.allow(500_000), Err(ProtocolError::RateLimit));
    }
}
