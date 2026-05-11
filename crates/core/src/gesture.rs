//! Pure-Rust gesture state machine.
//!
//! Consumes raw mouse events and emits high-level outcomes ("fire OCR",
//! "cancelled", "still holding"). Decoupled from Win32 so it is unit-testable.

use std::time::{Duration, Instant};

/// Raw input events fed in from the mouse hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureEvent {
    LeftDown,
    LeftUp,
    RightDown,
    RightUp,
    Move {
        dx: i32,
        dy: i32,
    },
    /// A periodic tick (e.g. every 16ms) so the state machine can detect the
    /// hold threshold without depending on wall-clock polling externally.
    Tick {
        now: Instant,
    },
}

/// What the gesture state machine wants the daemon to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureOutcome {
    /// No visible change; keep listening.
    Idle,
    /// Both buttons pressed; render the progress ring at the cursor.
    HoldStarted { began_at: Instant },
    /// Hold cancelled (button released early, or cursor moved too far).
    /// Daemon should hide the ring. If both buttons are still up, no native
    /// right-click suppression is needed; if a button is still down, let
    /// the OS handle it normally.
    HoldCancelled,
    /// Hold threshold reached. Daemon should:
    /// 1. Suppress the imminent right-click context menu (set a flag the
    ///    mouse hook checks before forwarding the eventual `RightUp`).
    /// 2. Capture the cursor position.
    /// 3. Call `capture::capture_region` + `ocr::ocr_frame`.
    Fire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Neutral,
    OneDown,
    Holding { began_at: Instant, drift: i32 },
    Fired,
}

#[derive(Debug)]
pub struct Gesture {
    state: State,
    left_down: bool,
    right_down: bool,
    /// Hold duration before firing.
    pub hold_threshold: Duration,
    /// Cursor drift allowed during the hold, in physical pixels.
    pub drift_limit_px: i32,
}

impl Default for Gesture {
    fn default() -> Self {
        Self {
            state: State::Neutral,
            left_down: false,
            right_down: false,
            hold_threshold: Duration::from_millis(250),
            drift_limit_px: 5,
        }
    }
}

impl Gesture {
    pub fn new(hold_threshold: Duration, drift_limit_px: i32) -> Self {
        Self {
            hold_threshold,
            drift_limit_px,
            ..Self::default()
        }
    }

    /// Process a single event and return what the daemon should do.
    pub fn process(&mut self, event: GestureEvent) -> GestureOutcome {
        match event {
            GestureEvent::LeftDown => {
                self.left_down = true;
                self.maybe_begin_hold(Instant::now())
            }
            GestureEvent::RightDown => {
                self.right_down = true;
                self.maybe_begin_hold(Instant::now())
            }
            GestureEvent::LeftUp => {
                self.left_down = false;
                self.maybe_cancel()
            }
            GestureEvent::RightUp => {
                self.right_down = false;
                self.maybe_cancel()
            }
            GestureEvent::Move { dx, dy } => self.accumulate_drift(dx.abs() + dy.abs()),
            GestureEvent::Tick { now } => self.maybe_fire(now),
        }
    }

    fn maybe_begin_hold(&mut self, now: Instant) -> GestureOutcome {
        if self.left_down && self.right_down {
            if matches!(self.state, State::Neutral | State::OneDown) {
                self.state = State::Holding {
                    began_at: now,
                    drift: 0,
                };
                return GestureOutcome::HoldStarted { began_at: now };
            }
        } else if self.left_down || self.right_down {
            if matches!(self.state, State::Neutral) {
                self.state = State::OneDown;
            }
        }
        GestureOutcome::Idle
    }

    fn maybe_cancel(&mut self) -> GestureOutcome {
        let was_holding = matches!(self.state, State::Holding { .. } | State::Fired);
        if !self.left_down && !self.right_down {
            self.state = State::Neutral;
        } else {
            self.state = State::OneDown;
        }
        if was_holding && !matches!(self.state, State::Fired) {
            GestureOutcome::HoldCancelled
        } else {
            GestureOutcome::Idle
        }
    }

    fn accumulate_drift(&mut self, delta: i32) -> GestureOutcome {
        if let State::Holding { began_at, drift } = self.state {
            let new = drift + delta;
            if new > self.drift_limit_px {
                self.state = State::OneDown;
                return GestureOutcome::HoldCancelled;
            }
            self.state = State::Holding {
                began_at,
                drift: new,
            };
        }
        GestureOutcome::Idle
    }

    fn maybe_fire(&mut self, now: Instant) -> GestureOutcome {
        if let State::Holding { began_at, .. } = self.state {
            if now.duration_since(began_at) >= self.hold_threshold {
                self.state = State::Fired;
                return GestureOutcome::Fire;
            }
        }
        GestureOutcome::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn single_button_does_not_start_hold() {
        let mut g = Gesture::default();
        assert_eq!(g.process(GestureEvent::LeftDown), GestureOutcome::Idle);
        assert_eq!(
            g.process(GestureEvent::Tick {
                now: Instant::now()
            }),
            GestureOutcome::Idle
        );
    }

    #[test]
    fn both_down_starts_hold() {
        let mut g = Gesture::default();
        g.process(GestureEvent::LeftDown);
        let r = g.process(GestureEvent::RightDown);
        assert!(matches!(r, GestureOutcome::HoldStarted { .. }));
    }

    #[test]
    fn release_early_cancels() {
        let mut g = Gesture::default();
        g.process(GestureEvent::LeftDown);
        g.process(GestureEvent::RightDown);
        assert_eq!(
            g.process(GestureEvent::LeftUp),
            GestureOutcome::HoldCancelled
        );
    }

    #[test]
    fn drift_too_far_cancels() {
        let mut g = Gesture::new(Duration::from_millis(250), 5);
        g.process(GestureEvent::LeftDown);
        g.process(GestureEvent::RightDown);
        assert_eq!(
            g.process(GestureEvent::Move { dx: 4, dy: 3 }),
            GestureOutcome::HoldCancelled
        );
    }

    #[test]
    fn full_hold_fires() {
        let base = Instant::now();
        let mut g = Gesture::new(Duration::from_millis(250), 5);
        g.process(GestureEvent::LeftDown);
        g.process(GestureEvent::RightDown);
        // Tick before threshold: still idle.
        assert_eq!(
            g.process(GestureEvent::Tick { now: at(base, 100) }),
            GestureOutcome::Idle
        );
        // Tick after threshold: fire.
        assert_eq!(
            g.process(GestureEvent::Tick {
                now: at(base, 1_000)
            }),
            GestureOutcome::Fire
        );
    }
}
