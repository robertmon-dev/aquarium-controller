use super::{CivilTime, CronExpr, Firing, REPEAT_HORIZON_S};

fn at(minute: u8, hour: u8, day: u8, month: u8, weekday: u8) -> CivilTime {
    CivilTime {
        minute,
        hour,
        day,
        month,
        weekday,
    }
}

fn parse(text: &str) -> CronExpr {
    CronExpr::parse(text).unwrap_or_else(|reason| panic!("{text}: {reason}"))
}

#[test]
fn stars_match_every_minute() {
    let every = parse("* * * * *");

    assert!(every.matches(at(0, 0, 1, 1, 0)));
    assert!(every.matches(at(59, 23, 31, 12, 6)));
}

#[test]
fn lists_ranges_and_steps_select_values() {
    let lists = parse("0,30 8-10 * * *");
    assert!(lists.matches(at(30, 9, 5, 5, 2)));
    assert!(!lists.matches(at(15, 9, 5, 5, 2)));
    assert!(!lists.matches(at(0, 11, 5, 5, 2)));

    let quarter = parse("*/15 * * * *");
    assert!(quarter.matches(at(45, 3, 5, 5, 2)));
    assert!(!quarter.matches(at(50, 3, 5, 5, 2)));

    let stepped_range = parse("10-40/10 * * * *");
    for minute in [10, 20, 30, 40] {
        assert!(stepped_range.matches(at(minute, 3, 5, 5, 2)));
    }
    assert!(!stepped_range.matches(at(0, 3, 5, 5, 2)));
    assert!(!stepped_range.matches(at(50, 3, 5, 5, 2)));
}

#[test]
fn a_single_value_with_a_step_runs_to_the_end_of_the_range() {
    let late = parse("50/5 * * * *");

    assert!(late.matches(at(55, 3, 5, 5, 2)));
    assert!(!late.matches(at(45, 3, 5, 5, 2)));
}

#[test]
fn weekday_names_and_both_sunday_numbers_are_accepted() {
    assert!(parse("0 10 * * sun").matches(at(0, 10, 4, 5, 0)));
    assert!(parse("0 10 * * 7").matches(at(0, 10, 4, 5, 0)));
    assert!(parse("0 10 * * 0").matches(at(0, 10, 4, 5, 0)));

    let workdays = parse("0 10 * * mon-fri");
    assert!(workdays.matches(at(0, 10, 6, 5, 3)));
    assert!(!workdays.matches(at(0, 10, 9, 5, 6)));
}

#[test]
fn day_fields_combine_with_or_when_both_are_restricted() {
    let either = parse("0 10 1,15 * sun");

    assert!(either.matches(at(0, 10, 1, 5, 3)));
    assert!(either.matches(at(0, 10, 2, 5, 0)));
    assert!(!either.matches(at(0, 10, 2, 5, 3)));
}

#[test]
fn a_star_day_of_month_keeps_the_weekday_restriction() {
    let sundays = parse("0 10 * * sun");

    assert!(!sundays.matches(at(0, 10, 1, 5, 3)));
}

#[test]
fn malformed_expressions_are_rejected() {
    for text in [
        "",
        "* * * *",
        "* * * * * *",
        "60 * * * *",
        "* 24 * * *",
        "* * 0 * *",
        "* * * 13 *",
        "* * * * 8",
        "*/0 * * * *",
        "5-1 * * * *",
        "a * * * *",
        "1,,2 * * * *",
    ] {
        assert!(CronExpr::parse(text).is_err(), "accepted: {text:?}");
    }
}

fn days_from_civil(year: i64, month: u8, day: u8) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let month = i64::from(month);
    let shifted = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * shifted + 2) / 5 + i64::from(day) - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

fn month_length(year: i64, month: u8) -> u8 {
    let next = if month == 12 {
        days_from_civil(year + 1, 1, 1)
    } else {
        days_from_civil(year, month + 1, 1)
    };
    (next - days_from_civil(year, month, 1)) as u8
}

fn calendar(year: i64) -> Vec<CivilTime> {
    (1..=12)
        .flat_map(|month| {
            (1..=month_length(year, month)).map(move |day| CivilTime {
                minute: 0,
                hour: 0,
                day,
                month,
                weekday: (days_from_civil(year, month, day) + 4).rem_euclid(7) as u8,
            })
        })
        .collect()
}

fn firing_dates(text: &str, year: i64) -> Vec<(u8, u8)> {
    let cron = parse(text);
    calendar(year)
        .into_iter()
        .filter(|at| cron.matches(*at))
        .map(|at| (at.month, at.day))
        .collect()
}

#[test]
fn the_test_calendar_knows_month_lengths_and_weekdays() {
    assert_eq!(calendar(2026).len(), 365);
    assert_eq!(calendar(2028).len(), 366);
    assert_eq!(month_length(2026, 2), 28);
    assert_eq!(month_length(2028, 2), 29);
    assert_eq!(month_length(2100, 2), 28);
    assert_eq!(month_length(2000, 2), 29);
    let first = calendar(2026)[0];
    assert_eq!((first.month, first.day, first.weekday), (1, 1, 4));
}

#[test]
fn day_31_fires_only_in_the_seven_long_months() {
    assert_eq!(
        firing_dates("0 0 31 * *", 2026),
        [
            (1, 31),
            (3, 31),
            (5, 31),
            (7, 31),
            (8, 31),
            (10, 31),
            (12, 31)
        ]
    );
}

#[test]
fn days_29_and_30_skip_february_outside_leap_years() {
    let thirtieth = firing_dates("0 0 30 * *", 2026);
    assert_eq!(thirtieth.len(), 11);
    assert!(!thirtieth.iter().any(|(month, _)| *month == 2));

    assert_eq!(firing_dates("0 0 29 * *", 2026).len(), 11);
    assert_eq!(firing_dates("0 0 29 * *", 2028).len(), 12);
}

#[test]
fn february_29_fires_only_in_leap_years() {
    assert!(firing_dates("0 0 29 2 *", 2026).is_empty());
    assert!(firing_dates("0 0 29 2 *", 2027).is_empty());
    assert_eq!(firing_dates("0 0 29 2 *", 2028), [(2, 29)]);
    assert!(firing_dates("0 0 29 2 *", 2100).is_empty());
    assert_eq!(firing_dates("0 0 29 2 *", 2000), [(2, 29)]);
}

#[test]
fn an_impossible_date_parses_but_never_fires() {
    assert!(firing_dates("0 0 31 2 *", 2028).is_empty());
    assert!(firing_dates("0 0 31 4,6,9,11 *", 2026).is_empty());
    assert!(firing_dates("0 0 30 2 *", 2028).is_empty());
}

#[test]
fn a_restricted_day_and_weekday_fire_on_either() {
    let dates = firing_dates("0 0 13 * fri", 2026);
    let fridays = calendar(2026).iter().filter(|at| at.weekday == 5).count();
    let thirteenths_not_friday = calendar(2026)
        .iter()
        .filter(|at| at.day == 13 && at.weekday != 5)
        .count();

    assert_eq!(dates.len(), fridays + thirteenths_not_friday);
    assert!(dates.contains(&(2, 13)));
    assert!(dates.contains(&(1, 13)));
    assert!(dates.contains(&(1, 2)));
}

#[test]
fn a_full_day_range_still_counts_as_restricted() {
    let every_day = firing_dates("0 0 1-31 * sun", 2026);

    assert_eq!(every_day.len(), 365);
}

#[test]
fn a_stepped_star_day_is_unrestricted_and_ands_with_the_weekday() {
    let dates = firing_dates("0 0 */2 * mon", 2026);

    assert!(!dates.is_empty());
    for (month, day) in &dates {
        let at = calendar(2026)
            .into_iter()
            .find(|at| at.month == *month && at.day == *day)
            .expect("a date of the year");
        assert_eq!(at.weekday, 1);
        assert_eq!(day % 2, 1);
    }
}

#[test]
fn a_star_weekday_with_a_restricted_day_keeps_the_day_only() {
    assert_eq!(firing_dates("0 0 15 * *", 2026).len(), 12);
    assert_eq!(firing_dates("0 0 15 * */1", 2026).len(), 12);
}

#[test]
fn a_day_step_restarts_at_every_month_boundary() {
    let dates = firing_dates("0 0 */10 * *", 2026);

    assert!(dates.contains(&(1, 31)));
    assert!(dates.contains(&(2, 1)));
    assert!(!dates.contains(&(2, 11 - 1)));
    assert!(dates.contains(&(2, 21)));
    assert!(dates.contains(&(3, 1)));
    assert_eq!(dates.len(), 12 * 3 + 7);
}

#[test]
fn a_month_step_counts_from_january() {
    assert_eq!(firing_dates("0 0 1 */5 *", 2026), [(1, 1), (6, 1), (11, 1)]);
    assert_eq!(
        firing_dates("0 0 1 2-12/3 *", 2026),
        [(2, 1), (5, 1), (8, 1), (11, 1)]
    );
}

#[test]
fn a_firing_repeats_only_on_the_same_local_minute_within_a_day() {
    let first = at(30, 2, 25, 10, 0);
    let firing = Firing::new(first, 1_000_000);

    assert!(firing.repeats(first, 1_000_000));
    assert!(firing.repeats(first, 1_003_600));
    assert!(firing.repeats(first, 1_000_000 - 300));
    assert!(!firing.repeats(at(31, 2, 25, 10, 0), 1_000_060));
    assert!(!firing.repeats(at(30, 2, 26, 10, 1), 1_086_400));
    assert!(!firing.repeats(first, 1_000_000 + REPEAT_HORIZON_S));
}

#[test]
fn a_yearly_window_fires_again_a_year_later() {
    let new_year = at(0, 10, 1, 1, 4);
    let firing = Firing::new(new_year, 1_000_000);

    assert!(!firing.repeats(at(0, 10, 1, 1, 5), 1_000_000 + 365 * 86_400));
}
