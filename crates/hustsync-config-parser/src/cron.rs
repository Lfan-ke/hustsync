//! Five-field cron expressions, `minute hour day month weekday`, for mirror schedules.

use std::{fmt, str::FromStr};

use chrono::{
    DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc,
};
use serde::{Deserialize, Deserializer, de};

/// A parsed cron schedule; each field is a bit set of allowed values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    src: String,
    minute: u64,
    hour: u64,
    day: u64,
    month: u64,
    weekday: u64,
    any_day: bool,
    any_weekday: bool,
}

impl Cron {
    /// First run strictly after `after`, in local time.
    pub fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.next_in(after, &Local)
    }

    /// First run strictly after `after`, with fields read in `tz`; `None` if nothing matches within five years.
    pub fn next_in<Tz: TimeZone>(&self, after: DateTime<Utc>, tz: &Tz) -> Option<DateTime<Utc>> {
        let start = after.with_timezone(tz).naive_local();
        let mut t = start.with_second(0)?.with_nanosecond(0)? + Duration::minutes(1);
        let end = start + Duration::days(5 * 366);
        while t < end {
            let d = t.date();
            t = if !has(self.month, d.month()) {
                month_after(d)?
            } else if !self.day_ok(d) {
                (d + Duration::days(1)).and_hms_opt(0, 0, 0)?
            } else if !has(self.hour, t.hour()) {
                t.with_minute(0)? + Duration::hours(1)
            } else if !has(self.minute, t.minute()) {
                t + Duration::minutes(1)
            } else if let Some(at) = tz.from_local_datetime(&t).earliest() {
                return Some(at.with_timezone(&Utc));
            } else {
                t + Duration::minutes(1)
            };
        }
        None
    }

    /// Day of month and weekday combine with OR when both are restricted, as in cron.
    fn day_ok(&self, d: NaiveDate) -> bool {
        let dom = has(self.day, d.day());
        let dow = has(self.weekday, d.weekday().num_days_from_sunday());
        match (self.any_day, self.any_weekday) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            (false, false) => dom || dow,
        }
    }
}

impl FromStr for Cron {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let f: Vec<&str> = s.split_whitespace().collect();
        let [minute, hour, day, month, weekday] = f[..] else {
            return Err(format!("cron `{s}` needs five fields"));
        };
        let bad = |what: &str| format!("cron `{s}`: invalid {what}");
        let weekday_bits = bits(weekday, 0, 7).ok_or_else(|| bad("weekday"))?;
        Ok(Self {
            src: s.to_owned(),
            minute: bits(minute, 0, 59).ok_or_else(|| bad("minute"))?,
            hour: bits(hour, 0, 23).ok_or_else(|| bad("hour"))?,
            day: bits(day, 1, 31).ok_or_else(|| bad("day"))?,
            month: bits(month, 1, 12).ok_or_else(|| bad("month"))?,
            weekday: (weekday_bits | weekday_bits >> 7) & 0x7f,
            any_day: day == "*",
            any_weekday: weekday == "*",
        })
    }
}

impl fmt::Display for Cron {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.src)
    }
}

impl<'de> Deserialize<'de> for Cron {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(de::Error::custom)
    }
}

const fn has(set: u64, v: u32) -> bool {
    set >> v & 1 == 1
}

/// Parses one field such as `*/15`, `1-5` or `0,30` into a bit set.
fn bits(field: &str, lo: u32, hi: u32) -> Option<u64> {
    let mut set = 0;
    for item in field.split(',') {
        let (range, step) = match item.split_once('/') {
            Some((r, s)) => (r, s.parse::<usize>().ok().filter(|&n| n > 0)?),
            None => (item, 1),
        };
        let (a, b) = match range.split_once('-') {
            _ if range == "*" => (lo, hi),
            Some((a, b)) => (a.parse().ok()?, b.parse().ok()?),
            None => {
                let a = range.parse().ok()?;
                (a, if step > 1 { hi } else { a })
            }
        };
        if a < lo || b > hi || a > b {
            return None;
        }
        set |= (a..=b).step_by(step).fold(0, |s, v| s | 1 << v);
    }
    Some(set)
}

fn month_after(d: NaiveDate) -> Option<NaiveDateTime> {
    let (y, m) = if d.month() == 12 {
        (d.year() + 1, 1)
    } else {
        (d.year(), d.month() + 1)
    };
    NaiveDate::from_ymd_opt(y, m, 1)?.and_hms_opt(0, 0, 0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::Cron;
    use chrono::{DateTime, FixedOffset, Utc};

    fn next(expr: &str, after: &str) -> String {
        let cst = FixedOffset::east_opt(8 * 3600).unwrap();
        let after: DateTime<Utc> = after.parse().unwrap();
        let cron: Cron = expr.parse().unwrap();
        cron.next_in(after, &cst)
            .unwrap()
            .with_timezone(&cst)
            .to_rfc3339()
    }

    #[test]
    fn finds_the_next_run() {
        let at = "2026-10-15T14:07:30Z";
        assert_eq!(next("*/15 * * * *", at), "2026-10-15T22:15:00+08:00");
        assert_eq!(next("0 */6 * * *", at), "2026-10-16T00:00:00+08:00");
        assert_eq!(next("30 3 * * *", at), "2026-10-16T03:30:00+08:00");
        assert_eq!(next("0 0 1 * *", at), "2026-11-01T00:00:00+08:00");
        assert_eq!(next("0 4 * * 0", at), "2026-10-18T04:00:00+08:00");
        assert_eq!(next("0 4 * * 7", at), "2026-10-18T04:00:00+08:00");
        assert_eq!(next("0 0 29 2 *", at), "2028-02-29T00:00:00+08:00");
        assert_eq!(next("0 12 1 * 1", at), "2026-10-19T12:00:00+08:00");
        assert_eq!(next("5/20 1-2 * * *", at), "2026-10-16T01:05:00+08:00");
    }

    #[test]
    fn rejects_bad_expressions() {
        for s in [
            "",
            "* * * *",
            "60 * * * *",
            "* 24 * * *",
            "* * 0 * *",
            "* * * 13 *",
            "* * * * 8",
            "*/0 * * * *",
            "5-1 * * * *",
            "a * * * *",
        ] {
            assert!(s.parse::<Cron>().is_err(), "{s}");
        }
    }

    #[test]
    fn never_matching_dates_give_none() {
        let cron: Cron = "0 0 31 2 *".parse().unwrap();
        assert!(cron.next_after(Utc::now()).is_none());
    }
}
