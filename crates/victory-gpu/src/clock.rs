//! The layer's stopwatch.
//!
//! The layer's own accounting (what a frame line says each phase took, the
//! diagnostics counters) reads the time many thousands of times a frame.
//! `std::time::Instant::now` is `QueryPerformanceCounter`, and under Wine
//! that is a system call each time, through Wine's dispatcher, which saves
//! the thread's register state on the way in: about 100,000 a frame on the
//! thread that submits and tens of thousands on each recording thread
//! (run 38-prof2, an RTX 5080 under Proton Experimental 11.0).
//!
//! [`Stamp`] reads the processor's time-stamp counter instead where that
//! counter runs at one rate on every core, whatever the cores do
//! (`CPUID` says so), and the system's clock where it does not.
//! `VICTORY_STOPWATCH=system`: the system's clock always, as before.

use std::sync::OnceLock;
use std::time::Duration;

/// A moment: nanoseconds since the first reading. Used as
/// `std::time::Instant` is, for the lengths of things; not for waking up at
/// a given time (the two clocks need not agree to the microsecond).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Stamp(u64);

/// How the counter's ticks become nanoseconds: its count at the first
/// reading, and nanoseconds per tick times 2^32.
struct Rate {
    first: u64,
    per_tick: u64,
}

/// `None`: the counter is not used.
static RATE: OnceLock<Option<Rate>> = OnceLock::new();
/// The first reading of the system's clock, where that is the one used.
static EPOCH: OnceLock<std::time::Instant> = OnceLock::new();

impl Stamp {
    #[inline]
    pub(crate) fn now() -> Stamp {
        match RATE.get_or_init(measure) {
            Some(rate) => {
                let ticks = ticks().wrapping_sub(rate.first);
                Stamp(((u128::from(ticks) * u128::from(rate.per_tick)) >> 32) as u64)
            }
            None => Stamp(
                EPOCH
                    .get_or_init(std::time::Instant::now)
                    .elapsed()
                    .as_nanos() as u64,
            ),
        }
    }

    /// The time since this moment.
    #[inline]
    pub(crate) fn elapsed(&self) -> Duration {
        Stamp::now().duration_since(*self)
    }

    /// The time from `earlier` to this moment (none if it is the later).
    #[inline]
    pub(crate) fn duration_since(&self, earlier: Stamp) -> Duration {
        Duration::from_nanos(self.0.saturating_sub(earlier.0))
    }
}

impl std::ops::Add<Duration> for Stamp {
    type Output = Stamp;
    fn add(self, more: Duration) -> Stamp {
        Stamp(self.0.saturating_add(more.as_nanos() as u64))
    }
}

impl std::ops::Sub<Stamp> for Stamp {
    type Output = Duration;
    fn sub(self, earlier: Stamp) -> Duration {
        self.duration_since(earlier)
    }
}

/// The time-stamp counter.
#[cfg(target_arch = "x86_64")]
#[inline]
fn ticks() -> u64 {
    // SAFETY: the instruction is there on every x86-64 processor.
    unsafe { core::arch::x86_64::_rdtsc() }
}

#[cfg(not(target_arch = "x86_64"))]
fn ticks() -> u64 {
    0
}

/// Whether the counter counts at one rate on every core, in every power
/// state (an "invariant" counter: `CPUID` leaf 8000_0007h, bit 8 of EDX).
#[cfg(target_arch = "x86_64")]
fn steady() -> bool {
    use core::arch::x86_64::__cpuid;
    __cpuid(0x8000_0000).eax >= 0x8000_0007 && __cpuid(0x8000_0007).edx & (1 << 8) != 0
}

#[cfg(not(target_arch = "x86_64"))]
fn steady() -> bool {
    false
}

/// The counter's rate, measured once against the system's clock over 20 ms
/// (to about one part in 100,000: this is a stopwatch for accounting).
fn measure() -> Option<Rate> {
    if !steady() || std::env::var_os("VICTORY_STOPWATCH").is_some_and(|v| v == "system") {
        return None;
    }
    let (began, first) = (std::time::Instant::now(), ticks());
    std::thread::sleep(Duration::from_millis(20));
    let (ns, counted) = (
        began.elapsed().as_nanos() as u64,
        ticks().wrapping_sub(first),
    );
    if ns == 0 || counted == 0 {
        return None;
    }
    let per_tick = ((u128::from(ns) << 32) / u128::from(counted)) as u64;
    // (A counter of 100 MHz to 100 GHz: anything else is a broken reading.)
    let sane = per_tick > (1 << 32) / 100 && per_tick < 10 * (1 << 32);
    tracing::info!(
        mhz = counted * 1000 / ns.max(1),
        used = sane,
        "the layer's stopwatch: the processor's time-stamp counter, measured against the system's clock (VICTORY_STOPWATCH=system reads the system's clock instead)"
    );
    sane.then_some(Rate { first, per_tick })
}

#[cfg(test)]
mod tests {
    use super::Stamp;
    use std::time::{Duration, Instant};

    /// The stopwatch agrees with the system's clock over a sleep, and does
    /// not run backwards.
    #[test]
    fn the_stopwatch_agrees_with_the_systems_clock() {
        let (stamp, instant) = (Stamp::now(), Instant::now());
        std::thread::sleep(Duration::from_millis(60));
        let (ours, theirs) = (stamp.elapsed(), instant.elapsed());
        let off = ours.abs_diff(theirs);
        assert!(
            off < Duration::from_millis(2),
            "{ours:?} against {theirs:?}"
        );
        let mut last = Stamp::now();
        for _ in 0..10_000 {
            let now = Stamp::now();
            assert!(now >= last);
            last = now;
        }
        assert_eq!(
            last.duration_since(last + Duration::from_millis(5)),
            Duration::ZERO
        );
        assert_eq!(
            (last + Duration::from_millis(5)) - last,
            Duration::from_millis(5)
        );
    }
}
