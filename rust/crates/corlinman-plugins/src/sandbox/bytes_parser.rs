//! Parse docker-style byte strings (`"256m"`, `"1g"`, `"512k"`, `"1024"`).
//!
//! Kept tiny on purpose — the only consumer is `DockerSandbox` translating
//! `manifest.sandbox.memory` into `HostConfig.memory` (bytes as `i64`).
//!
//! Grammar (case-insensitive, no whitespace tolerated):
//!   ```text
//!   value   := number [unit]
//!   number  := DIGIT+ ('.' DIGIT+)?
//!   unit    := 'b' | 'k' | 'm' | 'g' | 't'
//!              | 'kb' | 'mb' | 'gb' | 'tb'   (decimal aliases, same base)
//!   ```
//!
//! Units are powers of 1024 (docker's convention). An empty / missing unit
//! is treated as bytes.

use corlinman_core::CorlinmanError;

const KIB: u64 = 1024;
const MIB: u64 = KIB * 1024;
const GIB: u64 = MIB * 1024;
const TIB: u64 = GIB * 1024;

/// Parse a docker-style size string into bytes.
///
/// Returns `CorlinmanError::Config` when the input is empty, lacks digits,
/// carries an unknown unit, or overflows `u64`.
pub fn parse_bytes(raw: &str) -> Result<u64, CorlinmanError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CorlinmanError::Config(
            "sandbox.memory: empty string".into(),
        ));
    }

    // Split numeric prefix from alphabetic suffix.
    let split_at = trimmed
        .find(|c: char| c.is_ascii_alphabetic())
        .unwrap_or(trimmed.len());
    let (num_part, unit_part) = trimmed.split_at(split_at);

    if num_part.is_empty() {
        return Err(CorlinmanError::Config(format!(
            "sandbox.memory: no numeric prefix in '{raw}'"
        )));
    }

    let value: f64 = num_part.parse().map_err(|_| {
        CorlinmanError::Config(format!("sandbox.memory: invalid number in '{raw}'"))
    })?;
    if value < 0.0 || !value.is_finite() {
        return Err(CorlinmanError::Config(format!(
            "sandbox.memory: non-finite / negative number in '{raw}'"
        )));
    }

    let multiplier: u64 = match unit_part.to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" => KIB,
        "m" | "mb" => MIB,
        "g" | "gb" => GIB,
        "t" | "tb" => TIB,
        other => {
            return Err(CorlinmanError::Config(format!(
                "sandbox.memory: unknown unit '{other}' in '{raw}'"
            )));
        }
    };

    let product = value * multiplier as f64;
    if product > u64::MAX as f64 {
        return Err(CorlinmanError::Config(format!(
            "sandbox.memory: '{raw}' overflows u64"
        )));
    }
    Ok(product as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_bytes() {
        assert_eq!(parse_bytes("1024").unwrap(), 1024);
    }

    #[test]
    fn parses_kilobytes() {
        assert_eq!(parse_bytes("512k").unwrap(), 512 * 1024);
        assert_eq!(parse_bytes("512KB").unwrap(), 512 * 1024);
    }

    #[test]
    fn parses_megabytes() {
        assert_eq!(parse_bytes("256m").unwrap(), 256 * 1024 * 1024);
    }

    #[test]
    fn parses_gigabytes() {
        assert_eq!(parse_bytes("1g").unwrap(), 1u64 << 30);
        assert_eq!(parse_bytes("2G").unwrap(), 2u64 << 30);
    }

    #[test]
    fn parses_terabytes() {
        assert_eq!(parse_bytes("1t").unwrap(), 1u64 << 40);
    }

    #[test]
    fn parses_fractional_megabytes() {
        assert_eq!(parse_bytes("1.5m").unwrap(), (1.5 * MIB as f64) as u64);
    }

    #[test]
    fn empty_string_errors() {
        assert!(matches!(parse_bytes(""), Err(CorlinmanError::Config(_))));
        assert!(matches!(parse_bytes("   "), Err(CorlinmanError::Config(_))));
    }

    #[test]
    fn unknown_unit_errors() {
        assert!(matches!(
            parse_bytes("10zz"),
            Err(CorlinmanError::Config(_))
        ));
    }

    #[test]
    fn missing_number_errors() {
        assert!(matches!(parse_bytes("m"), Err(CorlinmanError::Config(_))));
    }
}
