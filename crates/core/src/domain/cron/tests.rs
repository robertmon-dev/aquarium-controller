use super::{CivilTime, CronExpr};

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
