//! Memory sizes written like `256M` or `1.5GiB`, matching go-units `RAMInBytes`.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, de};

/// A size in bytes, parsed with binary (1024-based) units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemBytes(pub u64);

impl FromStr for MemBytes {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || format!("invalid size `{s}`");
        let t = s.trim();
        let at = t
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(t.len());
        let (num, unit) = t.split_at(at);
        let unit = unit.strip_prefix(' ').unwrap_or(unit).to_ascii_lowercase();
        let unit = unit.strip_suffix('b').unwrap_or(&unit);
        let unit = unit.strip_suffix('i').unwrap_or(unit);
        let shift = match unit {
            "" => 0,
            "k" => 10,
            "m" => 20,
            "g" => 30,
            "t" => 40,
            "p" => 50,
            _ => return Err(bad()),
        };
        let (int, frac) = num.split_once('.').unwrap_or((num, ""));
        if int.is_empty() || (num.contains('.') && frac.is_empty()) {
            return Err(bad());
        }
        let scale = 1u128 << shift;
        let mul = |n: &str| n.parse::<u128>().ok()?.checked_mul(scale);
        let whole = mul(int).ok_or_else(bad)?;
        let part = match frac {
            "" => 0,
            f => {
                let den = u32::try_from(f.len())
                    .ok()
                    .and_then(|n| 10u128.checked_pow(n))
                    .ok_or_else(bad)?;
                mul(f).ok_or_else(bad)? / den
            }
        };
        whole
            .checked_add(part)
            .and_then(|n| u64::try_from(n).ok())
            .map(Self)
            .ok_or_else(bad)
    }
}

impl fmt::Display for MemBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<'de> Deserialize<'de> for MemBytes {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::MemBytes;

    #[test]
    fn parses_like_go_units() {
        let cases = [
            ("0", 0),
            ("32", 32),
            ("32b", 32),
            ("32 B", 32),
            ("32k", 32 << 10),
            ("32KiB", 32 << 10),
            ("256M", 256 << 20),
            ("256mb", 256 << 20),
            ("128MiB", 128 << 20),
            ("1.5g", 3 << 29),
            ("2T", 2 << 40),
            ("1p", 1 << 50),
        ];
        for (s, n) in cases {
            assert_eq!(s.parse::<MemBytes>(), Ok(MemBytes(n)), "{s}");
        }
    }

    #[test]
    fn rejects_garbage() {
        for s in ["", "m", "-1", "1.", "1x", "1 mb b", "99999999999p"] {
            assert!(s.parse::<MemBytes>().is_err(), "{s}");
        }
    }
}
