//! Formattazione dei numeri e dei tempi mostrati nell'interfaccia.

use std::time::Duration;

/// Numero con il separatore delle migliaia italiano (1.234.567).
pub fn count(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// "1 file copiato", "12 file copiati".
pub fn plural(n: usize, singular: &str, plural: &str) -> String {
    format!("{} {}", count(n), if n == 1 { singular } else { plural })
}

/// Tempo trascorso come mm:ss.
pub fn elapsed(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn italian_thousands() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(1234), "1.234");
        assert_eq!(count(1234567), "1.234.567");
    }

    #[test]
    fn plural_and_elapsed() {
        assert_eq!(plural(1, "file copiato", "file copiati"), "1 file copiato");
        assert_eq!(plural(2000, "file copiato", "file copiati"), "2.000 file copiati");
        assert_eq!(elapsed(Duration::from_secs(125)), "02:05");
    }
}
