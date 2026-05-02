use crate::WhenExpression;
use chrono::{DateTime, Utc};
use std::time::SystemTime;

pub struct Due {
    pub is_due: bool,
    pub reason: String,
}

pub fn check(
    when_expression: &WhenExpression,
    time_when_done: &Option<SystemTime>,
    time_now: &SystemTime,
) -> Due {
    if let WhenExpression::Never = when_expression {
        return Due {
            is_due: false,
            reason: String::from("it should never be done"),
        };
    }
    match time_when_done {
        None => Due {
            is_due: true,
            reason: String::from("it has never been done before"),
        },
        Some(time_when_done) => match when_expression {
            WhenExpression::Never => Due {
                is_due: false,
                reason: String::from("it should never be done"),
            },
            WhenExpression::Always => {
                if time_when_done.lt(time_now) {
                    Due {
                        is_due: true,
                        reason: String::from("it should always be done"),
                    }
                } else {
                    Due {
                        is_due: false,
                        reason: String::from("it was snoozed"),
                    }
                }
            }
            WhenExpression::EveryNDays { n } => {
                let then: DateTime<Utc> = DateTime::from(*time_when_done);
                let now: DateTime<Utc> = DateTime::from(*time_now);
                let diff = (now.date_naive() - then.date_naive()).num_days();
                if diff >= (*n).into() {
                    Due {
                        is_due: true,
                        reason: format!("{} or more days have passed", diff),
                    }
                } else if diff == 0 {
                    Due {
                        is_due: false,
                        reason: String::from("it has been done today already"),
                    }
                } else {
                    Due {
                        is_due: false,
                        reason: format!("only {} days have passed", diff),
                    }
                }
            }
            WhenExpression::EveryNHours { n } => {
                let then: DateTime<Utc> = DateTime::from(*time_when_done);
                let now: DateTime<Utc> = DateTime::from(*time_now);
                let diff = (now - then).num_hours();
                if diff >= (*n).into() {
                    Due {
                        is_due: true,
                        reason: format!("{} or more hours have passed", diff),
                    }
                } else if diff == 0 {
                    Due {
                        is_due: false,
                        reason: String::from("it has been done this hour already"),
                    }
                } else {
                    Due {
                        is_due: false,
                        reason: format!("only {} hours have passed", diff),
                    }
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, NaiveDate};
    use std::time::SystemTime;

    #[test]
    fn daily_means_the_next_day() {
        let some_day = NaiveDate::from_ymd_opt(2007, 10, 3).unwrap();
        let next_day = some_day + Duration::days(1);

        assert_is_not_due(check(
            &WhenExpression::EveryNDays { n: 1 },
            &Some(SystemTime::from(
                some_day.and_hms_opt(10, 0, 0).unwrap().and_utc(),
            )),
            &SystemTime::from(some_day.and_hms_opt(11, 0, 0).unwrap().and_utc()),
        ));

        assert_is_due(check(
            &WhenExpression::EveryNDays { n: 1 },
            &Some(SystemTime::from(
                some_day.and_hms_opt(10, 0, 0).unwrap().and_utc(),
            )),
            &SystemTime::from(next_day.and_hms_opt(11, 0, 0).unwrap().and_utc()),
        ));

        assert_is_due(check(
            &WhenExpression::EveryNDays { n: 1 },
            &Some(SystemTime::from(
                some_day.and_hms_opt(10, 0, 0).unwrap().and_utc(),
            )),
            &SystemTime::from(next_day.and_hms_opt(9, 0, 0).unwrap().and_utc()),
        ))
    }

    #[test]
    fn never_is_never_due() {
        let some_day = NaiveDate::from_ymd_opt(2007, 10, 3).unwrap();
        let now = SystemTime::from(some_day.and_hms_opt(11, 0, 0).unwrap().and_utc());

        assert_is_not_due(check(&WhenExpression::Never, &None, &now));
        assert_is_not_due(check(
            &WhenExpression::Never,
            &Some(SystemTime::from(
                some_day.and_hms_opt(10, 0, 0).unwrap().and_utc(),
            )),
            &now,
        ));
    }

    fn assert_is_due(due: Due) {
        assert!(due.is_due)
    }

    fn assert_is_not_due(due: Due) {
        assert!(!due.is_due)
    }
}
