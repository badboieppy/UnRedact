#[cfg(target_family = "wasm")]
#[derive(Debug, Clone, Copy)]
pub struct Instant {
    started_ms: f64,
}

#[cfg(target_family = "wasm")]
impl Instant {
    #[inline]
    pub fn now() -> Self {
        Self {
            started_ms: js_sys::Date::now(),
        }
    }

    #[inline]
    pub fn elapsed(&self) -> std::time::Duration {
        let elapsed_ms = (js_sys::Date::now() - self.started_ms).max(0.0_f64);
        std::time::Duration::from_secs_f64(elapsed_ms / 1000.0_f64)
    }
}

#[cfg(not(target_family = "wasm"))]
pub use std::time::Instant;
