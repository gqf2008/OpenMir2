//! 每 token 令牌桶限流（卡 A5）。
//!
//! 当前部署**只有一个 token**，服务启动时分配一个桶 ⇒「每 token 一桶」退化为
//! 「每服务一桶」；未来多 token 时按 token 摘要分桶即可（桶本身与 token 值无关，
//! 不存不记 token）。超限 ⇒ 429（`Retry-After: 1`）。
//!
//! 时间注入（[`RateLimiter::allow_at`]）让单测不依赖墙钟。

use std::sync::Mutex;
use std::time::Instant;

/// 令牌桶：容量 `capacity`，按 `refill_per_sec` 匀速回填。
#[derive(Debug)]
pub struct TokenBucket {
    capacity: f64,
    refill_per_sec: f64,
    tokens: f64,
    last: Instant,
}

impl TokenBucket {
    /// 以当前时刻为起点建桶（满桶）。
    #[must_use]
    pub fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self::new_at(capacity, refill_per_sec, Instant::now())
    }

    /// 以指定时刻为起点建桶（单测注入用；生产经 [`Self::new`] 走当前时刻）。
    #[must_use]
    pub fn new_at(capacity: f64, refill_per_sec: f64, now: Instant) -> Self {
        Self {
            capacity,
            refill_per_sec,
            tokens: capacity,
            last: now,
        }
    }

    fn refill(&mut self, now: Instant) {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        if elapsed > 0.0 {
            self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
            self.last = now;
        }
    }

    /// 尝试取一个令牌；成功 `true`。
    pub fn try_acquire_at(&mut self, now: Instant) -> bool {
        self.refill(now);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// 线程安全包装：请求处理路径从多任务并发调用 [`RateLimiter::allow`]。
#[derive(Debug)]
pub struct RateLimiter {
    bucket: Mutex<TokenBucket>,
}

impl RateLimiter {
    /// 以当前时刻建桶。
    #[must_use]
    pub fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self::new_at(capacity, refill_per_sec, Instant::now())
    }

    /// 指定起点建桶（单测注入时间）。
    #[must_use]
    pub fn new_at(capacity: f64, refill_per_sec: f64, now: Instant) -> Self {
        Self {
            bucket: Mutex::new(TokenBucket::new_at(capacity, refill_per_sec, now)),
        }
    }

    /// 请求路径入口：现在时刻尝试取令牌。
    pub fn allow(&self) -> bool {
        self.allow_at(Instant::now())
    }

    /// 指定时刻尝试取令牌（单测用）。
    pub fn allow_at(&self, now: Instant) -> bool {
        self.bucket
            .lock()
            .expect("限流锁不会 poison：临界区内无 panic 路径")
            .try_acquire_at(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn burst_then_refill() {
        let t0 = Instant::now();
        let limiter = RateLimiter::new_at(2.0, 1.0, t0);
        assert!(limiter.allow_at(t0));
        assert!(limiter.allow_at(t0));
        assert!(!limiter.allow_at(t0), "容量 2 用尽后同一时刻必须拒绝");

        let after_500ms = t0 + Duration::from_millis(500);
        assert!(!limiter.allow_at(after_500ms), "0.5 秒只回填 0.5 个令牌");

        let after_1100ms = t0 + Duration::from_millis(1100);
        assert!(limiter.allow_at(after_1100ms), "1.1 秒回填 ≥1 个令牌");
        assert!(!limiter.allow_at(after_1100ms), "只回填了一个");
    }

    #[test]
    fn refill_is_capped_at_capacity() {
        let t0 = Instant::now();
        let limiter = RateLimiter::new_at(2.0, 1.0, t0);
        assert!(limiter.allow_at(t0));
        assert!(limiter.allow_at(t0));
        let much_later = t0 + Duration::from_secs(60);
        assert!(limiter.allow_at(much_later));
        assert!(
            limiter.allow_at(much_later),
            "长时间空闲后仍是满桶（容量 2）"
        );
        assert!(!limiter.allow_at(much_later), "不能超过容量");
    }
}
