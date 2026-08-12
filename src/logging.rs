//! Internal logging facade.
//!
//! Logging backend is selected via Cargo features:
//! - `log`: use the `log` crate (std and no_std compatible)
//! - `defmt`: use `defmt` (embedded friendly), takes precedence if both are enabled
//! - neither: all log statements compile to no-ops

#[cfg(feature = "defmt")]
#[allow(unused_imports)]
pub(crate) use defmt::{debug, error, info, trace};

#[cfg(all(feature = "log", not(feature = "defmt")))]
#[allow(unused_imports)]
pub(crate) use log::{debug, error, info, trace};

#[cfg(not(any(feature = "log", feature = "defmt")))]
mod noop {
    macro_rules! noop {
        ($($arg:tt)*) => {{}};
    }
    pub(crate) use noop as debug;
    pub(crate) use noop as error;
    pub(crate) use noop as info;
    pub(crate) use noop as trace;
}

#[cfg(not(any(feature = "log", feature = "defmt")))]
#[allow(unused_imports)]
pub(crate) use noop::{debug, error, info, trace};

/// Hex dump wrapper for byte slices.
///
/// Formats as lowercase hex with both backends:
/// - `log`/std: via `Debug` (`{:?}`), e.g. `[de, ad, be, ef]`
/// - `defmt`: via `defmt::Format` (`{:?}`)
#[allow(dead_code)]
pub(crate) struct Hex<'a>(pub &'a [u8]);

impl core::fmt::Debug for Hex<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("[")?;
        for (i, b) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{:02x}", b)?;
        }
        f.write_str("]")
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for Hex<'_> {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{:02x}", self.0);
    }
}

/// Adapter to log any `core::fmt::Display` value with `{}` on both backends
/// (`String`, `std::io::Error` and friends don't implement `defmt::Format`).
#[allow(dead_code)]
pub(crate) struct Fmt<'a, T: core::fmt::Display + ?Sized>(pub &'a T);

impl<T: core::fmt::Display + ?Sized> core::fmt::Display for Fmt<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(feature = "defmt")]
impl<T: core::fmt::Display + ?Sized> defmt::Format for Fmt<'_, T> {
    fn format(&self, fmt: defmt::Formatter) {
        defmt::write!(fmt, "{}", defmt::Display2Format(self.0));
    }
}
