/// Current UTC timestamp in ISO 8601 format without milliseconds.
/// Returns a string like "2024-01-15T14:30:00Z".
pub fn utc_timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    let days_since_epoch = secs / 86400;
    let (year, month, day) = days_to_date(days_since_epoch as i64);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Current UTC timestamp with milliseconds, using space separator.
/// Returns a string like "2024-01-15 14:30:00.123".
pub fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    let secs = now.as_secs();
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;
    let millis = now.subsec_millis();

    let days_since_epoch = secs / 86400;
    let (year, month, day) = days_to_date(days_since_epoch as i64);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        year, month, day, hours, minutes, seconds, millis
    )
}

/// Convert days since Unix epoch (1970-01-01) to a (year, month, day) tuple.
pub fn days_to_date(days: i64) -> (i64, u32, u32) {
    let mut year = 1970;
    let mut remaining_days = days;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let month_days = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for &days_in_month in &month_days {
        if remaining_days < days_in_month as i64 {
            break;
        }
        remaining_days -= days_in_month as i64;
        month += 1;
    }

    (year, month, (remaining_days + 1) as u32)
}

/// Check if a year is a leap year.
pub fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_timestamp_iso_format() {
        let ts = utc_timestamp();
        assert_eq!(ts.len(), 20, "expected ISO 8601 length: got {ts}");
        assert!(ts.ends_with('Z'), "expected Z suffix: got {ts}");
        assert_eq!(&ts[10..11], "T", "expected T separator: got {ts}");
    }

    #[test]
    fn chrono_now_has_millis_and_space() {
        let ts = chrono_now();
        assert!(ts.contains(' '), "expected space separator: got {ts}");
        assert!(ts.contains('.'), "expected millis: got {ts}");
    }

    #[test]
    fn days_to_date_epoch() {
        assert_eq!(days_to_date(0), (1970, 1, 1));
    }

    #[test]
    fn days_to_date_known_dates() {
        assert_eq!(days_to_date(1), (1970, 1, 2));
        assert_eq!(days_to_date(364), (1970, 12, 31));
    }

    #[test]
    fn days_to_date_leap_year() {
        let days = days_since_epoch(2020, 2, 29);
        assert_eq!(days_to_date(days), (2020, 2, 29));
    }

    #[test]
    fn is_leap_year_true_for_2000() {
        assert!(is_leap_year(2000));
    }

    #[test]
    fn is_leap_year_false_for_1900() {
        assert!(!is_leap_year(1900));
    }

    #[test]
    fn is_leap_year_false_for_2001() {
        assert!(!is_leap_year(2001));
    }

    #[test]
    fn is_leap_year_true_for_2024() {
        assert!(is_leap_year(2024));
    }

    fn days_since_epoch(year: i64, month: u32, day: u32) -> i64 {
        let mut days = 0i64;
        for y in 1970..year {
            days += if is_leap_year(y) { 366 } else { 365 };
        }
        let month_days = if is_leap_year(year) {
            [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        } else {
            [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
        };
        for m in 1..month {
            days += month_days[(m - 1) as usize] as i64;
        }
        days + (day as i64) - 1
    }
}
