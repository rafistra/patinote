//! Unix seconds for date properties, stored in `properties.value_num` so that
//! date ranges use a numeric index (docs/design/0001, section 4).

/// Days from 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn num(s: &str) -> Option<i64> {
    s.bytes()
        .all(|b| b.is_ascii_digit())
        .then(|| s.parse().ok())
        .flatten()
}

/// `YYYY-MM-DD` at 00:00 UTC.
pub fn date(s: &str) -> Option<i64> {
    let (y, m, d) = (num(s.get(0..4)?)?, num(s.get(5..7)?)?, num(s.get(8..10)?)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d) * 86_400)
}

/// `YYYY-MM-DDTHH:MM[:SS[.fff]][Z|±HH:MM]`. Without a zone it is read as UTC.
pub fn datetime(s: &str) -> Option<i64> {
    let day = date(s.get(0..10)?)?;
    let (h, mi) = (num(s.get(11..13)?)?, num(s.get(14..16)?)?);
    let mut rest = &s[16..];
    let mut sec = 0;
    if let Some(r) = rest.strip_prefix(':') {
        sec = num(r.get(0..2)?)?;
        rest = &r[2..];
        if let Some(frac) = rest.strip_prefix('.') {
            rest = frac.trim_start_matches(|c: char| c.is_ascii_digit());
        }
    }
    let offset = match rest {
        "" | "Z" => 0,
        z if z.len() >= 5 && (z.starts_with('+') || z.starts_with('-')) => {
            let sign = if z.starts_with('-') { -1 } else { 1 };
            let digits: String = z[1..].chars().filter(char::is_ascii_digit).collect();
            let (oh, om) = (num(digits.get(0..2)?)?, num(digits.get(2..4)?)?);
            sign * (oh * 3600 + om * 60)
        }
        _ => return None,
    };
    Some(day + h * 3600 + mi * 60 + sec - offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(date("1970-01-01"), Some(0));
        assert_eq!(date("2000-03-01"), Some(951_868_800));
        assert_eq!(date("2026-13-01"), None);
        assert_eq!(datetime("1970-01-01T01:00"), Some(3600));
        assert_eq!(datetime("1970-01-01T01:00:30Z"), Some(3630));
        assert_eq!(datetime("1970-01-01T08:00:00+07:00"), Some(3600));
        assert_eq!(datetime("1970-01-01T08:00:00.5+0700"), Some(3600));
        assert_eq!(datetime("1970-01-01T08:00 x"), None);
    }
}
