use std::time::{SystemTime, UNIX_EPOCH};

const DAY: u64 = 86_400;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

pub fn stamp_seconds(stamp: &str) -> Option<u64> {
    let digits: String = stamp.chars().filter(char::is_ascii_digit).collect();
    if digits.len() != 14 {
        return None;
    }
    let field = |range: std::ops::Range<usize>| digits[range].parse::<i64>().ok();
    let days = days_from_civil(field(0..4)?, field(4..6)?, field(6..8)?);
    let seconds = days * DAY as i64 + field(8..10)? * 3600 + field(10..12)? * 60 + field(12..14)?;
    u64::try_from(seconds).ok()
}

pub fn describe(seconds: u64) -> (String, String) {
    let now = self::seconds();
    let age = now.saturating_sub(seconds);
    let ago = match age {
        0..60 => "just now".to_owned(),
        60..3600 => format!("{} min ago", age / 60),
        3600..86_400 => format!("{} h ago", age / 3600),
        _ => format!("{} days ago", age / 86_400),
    };
    let (_, month, day) = civil((seconds / DAY) as i64);
    let rest = seconds % DAY;
    let date = format!(
        "{day} {} {:02}:{:02} UTC",
        MONTHS[(month - 1) as usize],
        rest / 3600,
        rest % 3600 / 60
    );
    (ago, date)
}

pub fn now() -> String {
    let (date, time) = parts(seconds());
    format!("{date} {time}")
}

pub fn stamp() -> String {
    let (date, time) = parts(seconds());
    format!("{}-{}", date.replace('-', ""), time.replace(':', ""))
}

fn seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn parts(seconds: u64) -> (String, String) {
    let (year, month, day) = civil((seconds / DAY) as i64);
    let rest = seconds % DAY;
    (
        format!("{year:04}-{month:02}-{day:02}"),
        format!(
            "{:02}:{:02}:{:02}",
            rest / 3600,
            rest % 3600 / 60,
            rest % 60
        ),
    )
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::{parts, stamp_seconds};

    #[test]
    fn parses_snapshot_stamps() {
        assert_eq!(stamp_seconds("19700101T000000Z"), Some(0));
        assert_eq!(stamp_seconds("20000229T000000Z"), Some(951_782_400));
        assert_eq!(stamp_seconds("20261111T134224Z"), Some(1_794_404_544));
        assert_eq!(stamp_seconds("nope"), None);
    }

    #[test]
    fn formats_utc_dates() {
        assert_eq!(parts(0), ("1970-01-01".into(), "00:00:00".into()));
        assert_eq!(parts(951_782_400), ("2000-02-29".into(), "00:00:00".into()));
        assert_eq!(
            parts(1_794_404_544),
            ("2026-11-11".into(), "13:42:24".into())
        );
    }
}
