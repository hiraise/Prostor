//! Fractional ranks for manual order (architecture.md §6): values in a set, fields in a project,
//! later cards and articles. A rank is a string; ranks sort by their bytes (PostgreSQL columns use
//! the `C` collation), and a new rank fits between any two neighbours without renumbering others.
//!
//! A rank is an integer part followed by a fraction. The first character of the integer part
//! gives its length (`a`…`z`: 2 to 27 characters, `A`…`Z` for the ones below), so appending at
//! either end only grows the integer, and ranks stay short however long a list grows. Digits are
//! base 62 in byte order (`0`–`9`, `A`–`Z`, `a`–`z`); a fraction never ends with `0`, so there is
//! always room between two ranks.

use std::fmt;

const DIGITS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const ZERO: u8 = b'0';
const LAST: u8 = b'z';
/// The smallest integer part; nothing can come before a rank starting with it.
const SMALLEST: &str = "A00000000000000000000000000";

/// Why a rank could not be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RankError {
    /// The text is not a rank.
    Invalid(String),
    /// The lower neighbour is not below the upper one.
    Order,
    /// No room left at that end (after about 62²⁶ insertions there).
    Exhausted,
}

impl fmt::Display for RankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RankError::Invalid(rank) => write!(f, "invalid rank {rank:?}"),
            RankError::Order => f.write_str("the lower rank is not below the upper one"),
            RankError::Exhausted => f.write_str("no room for another rank at this end"),
        }
    }
}

impl std::error::Error for RankError {}

fn digit(c: u8) -> Option<usize> {
    DIGITS.iter().position(|d| *d == c)
}

fn integer_length(head: u8) -> Option<usize> {
    match head {
        b'a'..=b'z' => Some(usize::from(head - b'a') + 2),
        b'A'..=b'Z' => Some(usize::from(b'Z' - head) + 2),
        _ => None,
    }
}

/// Splits a rank into its integer part and fraction, checking its form.
fn split(rank: &str) -> Result<(&str, &str), RankError> {
    let invalid = || RankError::Invalid(rank.to_owned());
    let bytes = rank.as_bytes();
    let head = *bytes.first().ok_or_else(invalid)?;
    let length = integer_length(head).ok_or_else(invalid)?;
    if bytes.len() < length || rank == SMALLEST {
        return Err(invalid());
    }
    if !bytes[1..].iter().all(|c| digit(*c).is_some()) {
        return Err(invalid());
    }
    let (integer, fraction) = rank.split_at(length);
    if fraction.as_bytes().last() == Some(&ZERO) {
        return Err(invalid());
    }
    Ok((integer, fraction))
}

/// Checks that a stored or received text is a rank.
pub fn validate(rank: &str) -> Result<(), RankError> {
    split(rank).map(|_| ())
}

/// A fraction strictly between `a` and `b` (`None`: above every fraction). `a < b`; neither ends
/// with the zero digit.
fn midpoint(a: &[u8], b: Option<&[u8]>) -> Vec<u8> {
    if let Some(b) = b {
        // Keep the common prefix (`a` padded with zeros).
        let mut n = 0;
        while n < b.len() && a.get(n).copied().unwrap_or(ZERO) == b[n] {
            n += 1;
        }
        if n > 0 {
            let mut out = b[..n].to_vec();
            out.extend(midpoint(a.get(n..).unwrap_or_default(), Some(&b[n..])));
            return out;
        }
    }
    let low = a.first().and_then(|c| digit(*c)).unwrap_or(0);
    let high = b
        .and_then(|b| b.first())
        .and_then(|c| digit(*c))
        .unwrap_or(DIGITS.len());
    if high - low > 1 {
        let middle = (low + high).div_ceil(2);
        return vec![DIGITS[middle]];
    }
    // The first digits are neighbours.
    match b {
        Some(b) if b.len() > 1 => vec![b[0]],
        _ => {
            let mut out = vec![DIGITS[low]];
            out.extend(midpoint(a.get(1..).unwrap_or_default(), None));
            out
        }
    }
}

fn increment(integer: &str) -> Option<String> {
    let bytes = integer.as_bytes();
    let head = bytes[0];
    let mut digits = bytes[1..].to_vec();
    let mut carry = true;
    for d in digits.iter_mut().rev() {
        if !carry {
            break;
        }
        let next = digit(*d).unwrap_or(0) + 1;
        if next == DIGITS.len() {
            *d = ZERO;
        } else {
            *d = DIGITS[next];
            carry = false;
        }
    }
    if carry {
        if head == b'Z' {
            return Some(format!("a{}", char::from(ZERO)));
        }
        if head == b'z' {
            return None;
        }
        let head = head + 1;
        if head > b'a' {
            digits.push(ZERO);
        } else {
            digits.pop();
        }
        let mut out = vec![head];
        out.extend(digits);
        return String::from_utf8(out).ok();
    }
    let mut out = vec![head];
    out.extend(digits);
    String::from_utf8(out).ok()
}

fn decrement(integer: &str) -> Option<String> {
    let bytes = integer.as_bytes();
    let head = bytes[0];
    let mut digits = bytes[1..].to_vec();
    let mut borrow = true;
    for d in digits.iter_mut().rev() {
        if !borrow {
            break;
        }
        match digit(*d).unwrap_or(0) {
            0 => *d = LAST,
            value => {
                *d = DIGITS[value - 1];
                borrow = false;
            }
        }
    }
    if borrow {
        if head == b'a' {
            return Some(format!("Z{}", char::from(LAST)));
        }
        if head == b'A' {
            return None;
        }
        let head = head - 1;
        if head < b'Z' {
            digits.push(LAST);
        } else {
            digits.pop();
        }
        let mut out = vec![head];
        out.extend(digits);
        return String::from_utf8(out).ok();
    }
    let mut out = vec![head];
    out.extend(digits);
    String::from_utf8(out).ok()
}

fn join(integer: &str, fraction: &[u8]) -> String {
    let mut out = integer.to_owned();
    out.push_str(&String::from_utf8_lossy(fraction));
    out
}

/// A rank between two neighbours: `after` the lower one, `before` the upper one; `None` for an end
/// of the list. Both `None` gives the first rank of an empty list.
pub fn between(after: Option<&str>, before: Option<&str>) -> Result<String, RankError> {
    let low = after.map(split).transpose()?;
    let high = before.map(split).transpose()?;
    if let (Some(a), Some(b)) = (after, before)
        && a >= b
    {
        return Err(RankError::Order);
    }
    match (low, high) {
        (None, None) => Ok(format!("a{}", char::from(ZERO))),
        (None, Some((integer, fraction))) => {
            if integer == SMALLEST {
                return Ok(join(integer, &midpoint(b"", Some(fraction.as_bytes()))));
            }
            if !fraction.is_empty() {
                return Ok(integer.to_owned());
            }
            decrement(integer).ok_or(RankError::Exhausted)
        }
        (Some((integer, fraction)), None) => match increment(integer) {
            Some(next) => Ok(next),
            None => Ok(join(integer, &midpoint(fraction.as_bytes(), None))),
        },
        (Some((ia, fa)), Some((ib, fb))) => {
            if ia == ib {
                return Ok(join(ia, &midpoint(fa.as_bytes(), Some(fb.as_bytes()))));
            }
            let next = increment(ia).ok_or(RankError::Exhausted)?;
            if before.is_some_and(|b| next.as_str() < b) {
                Ok(next)
            } else {
                Ok(join(ia, &midpoint(fa.as_bytes(), None)))
            }
        }
    }
}

/// `count` ranks in increasing order for a new list (seeds and copies).
pub fn sequence(count: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(count);
    for _ in 0..count {
        let next = between(out.last().map(String::as_str), None).unwrap_or_default();
        out.push(next);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_and_appended_ranks() {
        assert_eq!(between(None, None).unwrap(), "a0");
        assert_eq!(between(Some("a0"), None).unwrap(), "a1");
        assert_eq!(between(Some("az"), None).unwrap(), "b00");
        assert_eq!(between(None, Some("a0")).unwrap(), "Zz");
        assert_eq!(between(Some("a0"), Some("a1")).unwrap(), "a0V");
        assert_eq!(between(Some("a0V"), Some("a1")).unwrap(), "a0l");
        assert_eq!(between(Some("a0"), Some("a0V")).unwrap(), "a0G");
        assert_eq!(between(None, Some("a0V")).unwrap(), "a0");
    }

    #[test]
    fn ranks_keep_their_order_in_any_insertion_pattern() {
        // Insert at the front, at the back and at pseudo-random places many times; the list stays
        // sorted and ranks stay short.
        let mut list: Vec<String> = vec![between(None, None).unwrap()];
        let mut seed: u64 = 7;
        for step in 0..3_000_usize {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let at = match step % 3 {
                0 => 0,
                1 => list.len(),
                _ => usize::try_from(seed >> 33).unwrap() % (list.len() + 1),
            };
            let after = at.checked_sub(1).map(|i| list[i].as_str());
            let before = list.get(at).map(String::as_str);
            let rank = between(after, before).unwrap();
            if let Some(a) = after {
                assert!(a < rank.as_str(), "{a} < {rank}");
            }
            if let Some(b) = before {
                assert!(rank.as_str() < b, "{rank} < {b}");
            }
            validate(&rank).unwrap();
            list.insert(at, rank);
        }
        let mut sorted = list.clone();
        sorted.sort();
        assert_eq!(sorted, list);
        assert!(list.iter().all(|rank| rank.len() < 40));
    }

    #[test]
    fn appending_stays_short() {
        let ranks = sequence(10_000);
        assert!(ranks.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(ranks.iter().all(|rank| rank.len() <= 4));
    }

    #[test]
    fn invalid_ranks_and_order() {
        assert!(matches!(validate(""), Err(RankError::Invalid(_))));
        assert!(matches!(validate("a"), Err(RankError::Invalid(_))));
        assert!(matches!(validate("a00"), Err(RankError::Invalid(_))));
        assert!(matches!(validate("a0-"), Err(RankError::Invalid(_))));
        assert!(validate("a0V").is_ok());
        assert_eq!(between(Some("a1"), Some("a0")), Err(RankError::Order));
        assert_eq!(between(Some("a1"), Some("a1")), Err(RankError::Order));
    }
}
