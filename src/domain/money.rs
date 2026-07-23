/// Parses a user-typed euro amount into cents (`"12,34"` → `1234`).
///
/// Strict shape, mirroring the desktop form: optional fractional part of 1-2
/// digits after `,` or `.`, no thousands separator, no sign, no exponent.
/// Integer-only arithmetic — never a float for money.
pub fn parse_eur_to_cents(value: &str) -> Option<i64> {
    let trimmed = value.trim();
    let (euros, cents) = match trimmed.find([',', '.']) {
        Some(separator) => {
            let (euros, rest) = trimmed.split_at(separator);
            let cents = &rest[1..];
            if cents.is_empty() || cents.len() > 2 || !cents.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            (euros, cents)
        }
        None => (trimmed, ""),
    };
    if euros.is_empty() || !euros.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let euros: i64 = euros.parse().ok()?;
    let cents_value = match cents.len() {
        0 => 0_i64,
        1 => i64::from(cents.as_bytes()[0] - b'0') * 10,
        _ => i64::from(cents.as_bytes()[0] - b'0') * 10 + i64::from(cents.as_bytes()[1] - b'0'),
    };
    euros.checked_mul(100)?.checked_add(cents_value)
}

pub fn format_eur(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    let euros = group_thousands(abs / 100);
    let cents_part = abs % 100;
    format!("{sign}{euros},{cents_part:02} €")
}

// French convention: non-breaking space every three digits (1 394,35 €).
fn group_thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push('\u{00A0}');
        }
        grouped.push(digit);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::{format_eur, parse_eur_to_cents};

    #[test]
    fn parses_euro_input_without_floats() {
        assert_eq!(parse_eur_to_cents("0"), Some(0));
        assert_eq!(parse_eur_to_cents("85"), Some(8_500));
        assert_eq!(parse_eur_to_cents("0,85"), Some(85));
        assert_eq!(parse_eur_to_cents("12,34"), Some(1_234));
        assert_eq!(parse_eur_to_cents("12,3"), Some(1_230));
        assert_eq!(parse_eur_to_cents("12,00"), Some(1_200));
    }

    #[test]
    fn accepts_dot_separator_and_surrounding_whitespace() {
        assert_eq!(parse_eur_to_cents("0.85"), Some(85));
        assert_eq!(parse_eur_to_cents(" 12,34 "), Some(1_234));
    }

    #[test]
    fn rejects_non_numeric_and_out_of_shape_input() {
        for value in [
            "", "   ", "abc", "-5", "+5", "1e3", "1,234", "1.2.3", "1,2,3", ",85", "12,", "1 234",
        ] {
            assert_eq!(
                parse_eur_to_cents(value),
                None,
                "{value:?} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_amounts_that_do_not_fit_in_cents() {
        assert_eq!(parse_eur_to_cents("92233720368547758,07"), Some(i64::MAX));
        assert_eq!(parse_eur_to_cents("92233720368547758,08"), None);
        assert_eq!(parse_eur_to_cents("99999999999999999999"), None);
    }

    #[test]
    fn formats_positive_euros_in_french_style() {
        assert_eq!(format_eur(25_000), "250,00 €");
        assert_eq!(format_eur(85), "0,85 €");
    }

    #[test]
    fn groups_thousands_with_non_breaking_space() {
        assert_eq!(format_eur(139_435), "1\u{00A0}394,35 €");
        assert_eq!(format_eur(123_456_789), "1\u{00A0}234\u{00A0}567,89 €");
    }

    #[test]
    fn formats_negative_euros_in_french_style() {
        assert_eq!(format_eur(-25_000), "-250,00 €");
        assert_eq!(format_eur(-85), "-0,85 €");
    }

    #[test]
    fn formats_i64_min_without_overflow() {
        assert_eq!(
            format_eur(i64::MIN),
            "-92\u{00A0}233\u{00A0}720\u{00A0}368\u{00A0}547\u{00A0}758,08 €"
        );
    }
}
