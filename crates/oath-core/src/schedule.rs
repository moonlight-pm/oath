//! A service's `schedule` (R48): a cron pattern, croner's (five fields,
//! or six with seconds; `@daily`; `L`, `W`, `#`), in the machine's own
//! time zone (`host:local.timezone`, which PID 1 sets as its `TZ`), so a
//! job at 04:00 stays at 04:00 across daylight saving.

use chrono::{DateTime, Local, TimeZone};
use croner::parser::{CronParser, Seconds};
use croner::Cron;

/// The pattern, or why it is not one.
pub fn parse(pattern: &str) -> Result<Cron, String> {
    CronParser::builder()
        .seconds(Seconds::Optional)
        .build()
        .parse(pattern.trim())
        .map_err(|e| format!("schedule {pattern:?} is not a cron pattern: {e}"))
}

/// The first time after `t` the pattern matches.
pub fn next_after<Tz: TimeZone>(pattern: &str, t: &DateTime<Tz>) -> Option<DateTime<Tz>> {
    parse(pattern).ok()?.find_next_occurrence(t, false).ok()
}

/// The last time at or before `t` the pattern matched.
pub fn last_at<Tz: TimeZone>(pattern: &str, t: &DateTime<Tz>) -> Option<DateTime<Tz>> {
    parse(pattern).ok()?.find_previous_occurrence(t, true).ok()
}

/// Whether a run is owed now: a match passed since the last run (`last`),
/// while the machine was down or PID 1 was not watching. With no last run
/// it is not: a new schedule waits for its first match.
pub fn owed(pattern: &str, last: Option<DateTime<Local>>, now: &DateTime<Local>) -> bool {
    match (last, last_at(pattern, now)) {
        (Some(last), Some(due)) => last < due,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn at(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    #[test]
    fn a_daily_pattern_matches_once_a_day() {
        assert!(parse("0 4 * * *").is_ok());
        assert!(parse("0 0 4 * * *").is_ok(), "six fields: seconds");
        assert!(parse("@daily").is_ok());
        assert!(parse("0 4 * *").is_err());
        assert!(parse("at four").is_err());
        let t = at("2026-10-31T09:30:00-06:00");
        assert_eq!(next_after("0 4 * * *", &t).unwrap(), at("2026-11-01T04:00:00-06:00"));
        assert_eq!(last_at("0 4 * * *", &t).unwrap(), at("2026-10-31T04:00:00-06:00"));
        // The month's last day, at 03:30.
        assert_eq!(next_after("30 3 L * *", &t).unwrap(), at("2026-11-30T03:30:00-06:00"));
    }

    #[test]
    fn a_missed_run_is_owed_once() {
        let now = Local::now();
        let p = "0 4 * * *";
        let due = last_at(p, &now).unwrap();
        assert!(owed(p, Some(due - chrono::Duration::hours(1)), &now), "down at 04:00");
        assert!(!owed(p, Some(due), &now), "ran at 04:00");
        assert!(!owed(p, None, &now), "a new schedule waits for its first match");
    }
}
