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
    use super::format_eur;

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
