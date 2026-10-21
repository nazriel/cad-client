//! Environment variables that switch something off.

/// Whether `value` is an explicit off word: `0`, `false`, `no` or `off`, in any
/// case, with surrounding whitespace ignored.
pub(crate) fn is_off_word(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off"
    )
}
