//! Plain arithmetic in chosen programs, for measuring only (diagnostics).
//!
//! The console's legacy multiply (`0 × anything = 0`) and its maximum and
//! minimum (which order -0 below +0) have no single DXIL operation, so the
//! lowering writes each as several: a legacy multiply as a multiply, two
//! compares, an `or` and a select; a maximum or minimum as DXIL's, three
//! integer operations, a compare and a select.
//! The two switches below write them as the one DXIL operation instead, in
//! the programs they name, so that a timing run shows what the exact forms
//! cost the card. Each takes `all` or 16-digit shader hashes as a run's log
//! names them (`hash=` in "first bind"), separated by commas:
//!
//! - `VICTORY_PLAIN_MUL`: legacy multiplies (`v_mul_legacy_f32`,
//!   `v_mac_legacy_f32`, `v_mad_legacy_f32`) as plain ones. 0 × infinity
//!   and 0 × NaN then give NaN, and a zero product can be -0.
//! - `VICTORY_PLAIN_MINMAX`: maxima and minima (`v_max_f32`, `v_min_f32`,
//!   their three-operand and 16-bit forms) as DXIL's FMax and FMin. Of a -0
//!   and a +0, either can then be the result.
//!
//! The picture of a run with either switch is not valid. Without the
//! `diagnostics` feature both are off. They are part of the translation,
//! so the shader cache keys on them (`victory-gpu`'s does).

use std::cell::Cell;
use std::sync::OnceLock;

/// Legacy multiplies as plain ones (`VICTORY_PLAIN_MUL`).
pub const MUL: u8 = 1;
/// Maxima and minima as DXIL's (`VICTORY_PLAIN_MINMAX`).
pub const MINMAX: u8 = 2;

thread_local! {
    /// The forms the translation running on this thread writes plainly.
    static ON: Cell<u8> = const { Cell::new(0) };
}

/// The plain forms (bits of [`MUL`] and [`MINMAX`]) that the switches ask
/// for in the program with this hash. 0 without the `diagnostics` feature.
pub fn for_program(hash: &str) -> u8 {
    static VALUES: OnceLock<[Option<String>; 2]> = OnceLock::new();
    let [mul, minmax] = VALUES.get_or_init(|| {
        if cfg!(feature = "diagnostics") {
            [
                std::env::var("VICTORY_PLAIN_MUL").ok(),
                std::env::var("VICTORY_PLAIN_MINMAX").ok(),
            ]
        } else {
            [None, None]
        }
    });
    let mut bits = 0;
    if names(mul.as_deref(), hash) {
        bits |= MUL;
    }
    if names(minmax.as_deref(), hash) {
        bits |= MINMAX;
    }
    bits
}

/// Whether a switch's value names the program with this hash. Pure, for
/// testing.
fn names(value: Option<&str>, hash: &str) -> bool {
    value.is_some_and(|value| {
        value.split(',').map(str::trim).any(|item| {
            item.eq_ignore_ascii_case("all")
                || (item.len() == 16 && item.eq_ignore_ascii_case(hash))
        })
    })
}

/// Runs `f`, a translation, with the plain forms `bits` names.
pub fn with<R>(bits: u8, f: impl FnOnce() -> R) -> R {
    struct Restore(u8);
    impl Drop for Restore {
        fn drop(&mut self) {
            ON.set(self.0);
        }
    }
    let _restore = Restore(ON.replace(bits));
    f()
}

/// Whether the translation running on this thread writes `form` plainly.
pub(super) fn on(form: u8) -> bool {
    ON.get() & form != 0
}

#[cfg(test)]
mod tests {
    use super::{MINMAX, MUL, names, on, with};

    #[test]
    fn a_switch_names_all_programs_or_listed_hashes() {
        let hash = "5cf00dc39042edaf";
        assert!(!names(None, hash));
        assert!(!names(Some(""), hash));
        assert!(names(Some("all"), hash));
        assert!(names(Some(" ALL "), hash));
        assert!(names(Some("982f3cccd5c04294, 5CF00DC39042EDAF"), hash));
        assert!(!names(Some("982f3cccd5c04294"), hash));
        // A prefix is not a hash.
        assert!(!names(Some("5cf00dc3"), hash));
    }

    #[test]
    fn the_forms_hold_inside_the_call_only() {
        assert!(!on(MUL));
        with(MUL, || {
            assert!(on(MUL));
            assert!(!on(MINMAX));
            with(MUL | MINMAX, || assert!(on(MINMAX)));
            assert!(!on(MINMAX));
        });
        assert!(!on(MUL));
        let _ = std::panic::catch_unwind(|| with(MINMAX, || panic!("a translation that panics")));
        assert!(!on(MINMAX));
    }
}
