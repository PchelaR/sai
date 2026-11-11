use std::time::{SystemTime, UNIX_EPOCH};

const DAY: u64 = 86_400;

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
    use super::parts;

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
