use std::{collections::HashMap, iter::Peekable};

use chrono::{
    DateTime, Datelike, Duration, Month, NaiveDate, NaiveTime, TimeZone, Timelike, Utc, Weekday,
};
use chrono_tz::{Europe, Tz};
use lazy_static::lazy_static;

use crate::interface::{Schedule, ScheduleGridBuilder};

lazy_static! {
    static ref TIME_FORMATS: Vec<String> = vec![
        "%H:%M:%S".to_owned(),
        "%H:%M".to_owned(),
    ];
    static ref DATE_FORMATS: Vec<String> = vec![
        "%d.%m.%Y".to_owned(),
        "%d-%m-%Y".to_owned(),
        "%d/%m/%Y".to_owned(),
    ];
    static ref WEEKDAYS: HashMap<String, Weekday> = {
        let mut lookup = HashMap::with_capacity(7);
        lookup.insert("lunedì".to_owned(), Weekday::Mon);
        lookup.insert("lunedi".to_owned(), Weekday::Mon);
        lookup.insert("martedì".to_owned(), Weekday::Tue);
        lookup.insert("martedi".to_owned(), Weekday::Tue);
        lookup.insert("mercoledì".to_owned(), Weekday::Wed);
        lookup.insert("mercoledi".to_owned(), Weekday::Wed);
        lookup.insert("giovedì".to_owned(), Weekday::Thu);
        lookup.insert("giovedi".to_owned(), Weekday::Thu);
        lookup.insert("venerdì".to_owned(), Weekday::Fri);
        lookup.insert("venerdi".to_owned(), Weekday::Fri);
        lookup.insert("sabato".to_owned(), Weekday::Sat);
        lookup.insert("domenica".to_owned(), Weekday::Sun);
        lookup
    };
    static ref MONTHS: HashMap<String, Month> = {
        let mut lookup = HashMap::with_capacity(12);
        lookup.insert("gennaio".to_owned(), Month::January);
        lookup.insert("febbraio".to_owned(), Month::February);
        lookup.insert("marzo".to_owned(), Month::March);
        lookup.insert("aprile".to_owned(), Month::April);
        lookup.insert("maggio".to_owned(), Month::May);
        lookup.insert("giugno".to_owned(), Month::June);
        lookup.insert("luglio".to_owned(), Month::July);
        lookup.insert("agosto".to_owned(), Month::August);
        lookup.insert("settembre".to_owned(), Month::September);
        lookup.insert("ottobre".to_owned(), Month::October);
        lookup.insert("novembre".to_owned(), Month::November);
        lookup.insert("dicembre".to_owned(), Month::December);
        lookup
    };
    static ref DURATION_UNITS: HashMap<String, Duration> = {
        let mut lookup = HashMap::with_capacity(10);
        // Plural
        lookup.insert("secondi".to_owned(), Duration::seconds(1));
        lookup.insert("minuti".to_owned(), Duration::minutes(1));
        lookup.insert("ore".to_owned(), Duration::hours(1));
        lookup.insert("giorni".to_owned(), Duration::days(1));
        lookup.insert("settimane".to_owned(), Duration::weeks(1));
        // Singular
        lookup.insert("secondo".to_owned(), Duration::seconds(1));
        lookup.insert("minuto".to_owned(), Duration::minutes(1));
        lookup.insert("ora".to_owned(), Duration::hours(1));
        lookup.insert("giorno".to_owned(), Duration::days(1));
        lookup.insert("settimana".to_owned(), Duration::weeks(1));
        lookup
    };
    static ref POSITIONS: HashMap<String, u32> = {
        let mut lookup = HashMap::with_capacity(5);
        // Masculine
        lookup.insert("primo".to_owned(), 0);
        lookup.insert("secondo".to_owned(), 1);
        lookup.insert("terzo".to_owned(), 2);
        lookup.insert("quarto".to_owned(), 3);
        lookup.insert("quinto".to_owned(), 4);
        // Feminine
        lookup.insert("prima".to_owned(), 0);
        lookup.insert("seconda".to_owned(), 1);
        lookup.insert("terza".to_owned(), 2);
        lookup.insert("quarta".to_owned(), 3);
        lookup.insert("quinta".to_owned(), 4);
        lookup
    };
}

#[allow(clippy::never_loop)]
pub fn try_parse(tokens: Vec<&str>, now: &DateTime<Utc>) -> Option<Schedule> {
    tracing::warn!("TODO: try_interpret_definition has to be implemented yet!");
    dispatch_category(tokens.into_iter().skip(1).peekable(), now, &Europe::Rome)
}

fn dispatch_category<'a, T: Iterator<Item = &'a str>>(
    mut tokens: Peekable<T>,
    now: &DateTime<Utc>,
    tz: &Tz,
) -> Option<Schedule> {
    let context = now.with_timezone(tz);
    match tokens.peek().copied() {
        Some(x) if WEEKDAYS.contains_key(x) => build_once(tokens, &context),
        Some("ogni") | Some("fino") | Some("dal") | Some("dall") | Some("da") => {
            build_recurrent(tokens, &context, tz)
        }
        Some("alle") | Some("a") | Some("il") | Some("lo") | Some("l") | Some("nel")
        | Some("ad") | Some("tra") => build_once(tokens, &context),
        _ => None,
    }
}

fn build_recurrent<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    mut tokens: Peekable<T>,
    now: &DateTime<TZ>,
    tz: &Tz,
) -> Option<Schedule> {
    let mut since = now.clone();
    let mut until: Option<DateTime<TZ>> = None;
    let mut builder = ScheduleGridBuilder::new(*tz);
    builder.with_times(vec![since.time()]);

    while let Some(token) = tokens.peek().copied() {
        match token {
            "ogni" => {
                tokens.next();
                set_schedule(&mut builder, &mut tokens)
            }
            "al" | "all" | "allo" | "a" | "ad" => set_until(&mut until, &since, &mut tokens),
            "dal" | "da" | "dall" => set_since(&mut since, now, &mut tokens),
            "alle" => {
                tokens.next();
                set_time_schedule(&mut builder, &mut tokens)
            }
            _ => {
                tokens.next();
            }
        };
    }

    builder.with_year(since.year() as u32);

    Some(match until {
        None => Schedule::Recurrent {
            since: since.with_timezone(&Utc),
            schedule: builder.build(),
        },
        Some(until) => Schedule::RecurrentUntil {
            since: since.with_timezone(&Utc),
            until: until.with_timezone(&Utc),
            schedule: builder.build(),
        },
    })
}

fn set_time_schedule<'a, T: Iterator<Item = &'a str>>(
    builder: &mut ScheduleGridBuilder,
    tokens: &mut Peekable<T>,
) {
    builder.with_times(std::iter::from_fn(|| try_parse_time(tokens)).collect());
}

fn set_schedule<'a, T: Iterator<Item = &'a str>>(
    builder: &mut ScheduleGridBuilder,
    tokens: &mut Peekable<T>,
) {
    while let Some(token) = tokens.peek().copied() {
        match token {
            "fino" | "al" | "all" | "a" | "ad" | "dal" | "da" | "dall" | "alle" | "ogni" => return,
            x if (POSITIONS.contains_key(x) || WEEKDAYS.contains_key(x)) => {
                set_weekday_schedule(builder, tokens);
            }
            "di" => {
                tokens.next();
            }
            _ => {
                set_fittest_schedule(builder, tokens);
            }
        };
    }
}

fn set_weekday_schedule<'a, T: Iterator<Item = &'a str>>(
    builder: &mut ScheduleGridBuilder,
    tokens: &mut Peekable<T>,
) {
    let weekday_positions = std::iter::from_fn(|| {
        let position = try_parse_position(tokens);
        while let Some("e") = tokens.peek().copied() {
            tokens.next();
        }
        position
    })
    .collect::<Vec<_>>();

    if !weekday_positions.is_empty() {
        builder.with_weeks_of_month(weekday_positions);
    }

    let weekdays = std::iter::from_fn(|| {
        let weekday = try_parse_weekday(tokens).copied();
        while let Some("e") = tokens.peek().copied() {
            tokens.next();
        }
        weekday
    })
    .collect::<Vec<_>>();

    if !weekdays.is_empty() {
        builder.with_weekdays(weekdays);
    }
}

fn set_fittest_schedule<'a, T: Iterator<Item = &'a str>>(
    builder: &mut ScheduleGridBuilder,
    tokens: &mut Peekable<T>,
) {
    match tokens.peek().copied() {
        Some(x) if MONTHS.contains_key(x) => set_month_schedule(builder, tokens),
        Some(x) if x.contains(&['.', '/', '-'][..]) => set_date_schedule(builder, tokens),
        Some(x) if x.parse::<u32>().is_ok() => set_numeric_schedule(builder, tokens),
        _ => (),
    }
}
fn set_numeric_schedule<'a, T: Iterator<Item = &'a str>>(
    builder: &mut ScheduleGridBuilder,
    tokens: &mut Peekable<T>,
) {
    let numbers = std::iter::from_fn(|| {
        let num = tokens
            .peek()
            .copied()
            .and_then(|s| s.parse::<u8>().ok())
            .inspect(|_| {
                tokens.next();
            });
        while let Some("e") = tokens.peek().copied() {
            tokens.next();
        }
        num
    })
    .collect::<Vec<_>>();

    match tokens.peek().copied() {
        Some("anni") => {
            tokens.next();
            builder.with_year_cadence(numbers.last().copied().unwrap_or(1u8));
        }
        Some("del") => {
            tokens.next();
            if let Some("mese") = tokens.peek().copied() {
                tokens.next();
                builder.with_days_of_month(numbers);
            }
        }
        Some("di") => {
            tokens.next();
            builder.with_days_of_month(numbers);
            set_month_schedule(builder, tokens);
        }
        _ => (),
    }
}

fn set_date_schedule<'a, T: Iterator<Item = &'a str>>(
    builder: &mut ScheduleGridBuilder,
    tokens: &mut Peekable<T>,
) {
    tokens
        .peek()
        .copied()
        .map(|s| {
            s.split(&['.', '/', '-'][..])
                .map(|piece| piece.trim_start_matches('0'))
                .filter_map(|piece| piece.parse::<u8>().ok())
                .collect::<Vec<_>>()
        })
        .inspect(|_| {
            tokens.next();
        })
        .filter(|v| v.len() >= 2)
        .map(|v| (v[0], v[1]))
        .map(|(day, month)| {
            builder.with_days_of_month(vec![day]);
            Month::try_from(month)
                .ok()
                .map(|m| builder.with_months(vec![m]));
        });
}

fn set_month_schedule<'a, T: Iterator<Item = &'a str>>(
    builder: &mut ScheduleGridBuilder,
    tokens: &mut Peekable<T>,
) {
    let months = std::iter::from_fn(|| {
        let month = try_parse_month(tokens).copied();
        while let Some("e") = tokens.peek().copied() {
            tokens.next();
        }
        month
    })
    .collect::<Vec<_>>();

    builder.with_months(months);
}

fn set_until<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    until: &mut Option<DateTime<TZ>>,
    since: &DateTime<TZ>,
    tokens: &mut Peekable<T>,
) {
    let mut new_until = since.clone();
    while let Some(token) = tokens.peek().copied() {
        new_until = match token {
            "alle" => {
                tokens.next();
                at_time(since, new_until, tokens)
            }
            "al" | "allo" | "all" => {
                tokens.next();
                at_date_or_year(new_until, since, tokens)
            }
            "a" | "ad" => {
                tokens.next();
                at_month_or_weekday(new_until, tokens)
            }
            _ => break,
        };
    }
    *until = Some(new_until)
}

fn set_since<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    since: &mut DateTime<TZ>,
    now: &DateTime<TZ>,
    tokens: &mut Peekable<T>,
) {
    let mut new_since = since.clone();
    while let Some(token) = tokens.peek().copied() {
        new_since = match token {
            "dalle" => {
                tokens.next();
                at_time(now, new_since, tokens)
            }
            "dal" | "dallo" | "dall" => {
                tokens.next();
                at_date_or_year(new_since, now, tokens)
            }
            "da" => {
                tokens.next();
                at_month_or_weekday(new_since, tokens)
            }
            _ => break,
        };
    }

    *since = new_since;
}

fn at_date_or_year<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    date: DateTime<TZ>,
    lower_bound: &DateTime<TZ>,
    tokens: &mut Peekable<T>,
) -> DateTime<TZ> {
    tokens
        .peek()
        .copied()
        .and_then(|n| n.parse::<i32>().ok().filter(|year| *year > 1970))
        .inspect(|_| {
            tokens.next();
        })
        .map(|year| set_year(year + 1, date.clone()))
        .map(|d| truncated_by_day(&d))
        .unwrap_or_else(|| at_date(lower_bound, date, tokens))
}

fn at_month_or_weekday<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    date: DateTime<TZ>,
    tokens: &mut Peekable<T>,
) -> DateTime<TZ> {
    tokens
        .peek()
        .copied()
        .and_then(|d| WEEKDAYS.get(d).copied())
        .inspect(|_| {
            tokens.next();
        })
        .map(|weekday| next_weekday(weekday, date.clone()))
        .unwrap_or_else(|| at_month(date, tokens))
}

fn build_once<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    mut tokens: Peekable<T>,
    now: &DateTime<TZ>,
) -> Option<Schedule> {
    let mut when = now.clone();
    while let Some(token) = tokens.next() {
        when = match token {
            "tra" => advance_time(when, &mut tokens),
            "alle" => at_time(now, when, &mut tokens),
            "il" | "lo" | "l" => at_date(now, when, &mut tokens),
            "a" | "ad" => at_month(when, &mut tokens),
            "nel" => at_beginning_of_year(when, &mut tokens),
            x if WEEKDAYS.contains_key(x) => configure_weekday(when, &mut tokens),
            _ => when,
        };
    }
    let schedule = Schedule::Once {
        when: when.with_timezone(&Utc),
    };

    tracing::info!("Computed: {:#?}", schedule);

    Some(schedule)
}

fn advance_time<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    when: DateTime<TZ>,
    tokens: &mut Peekable<T>,
) -> DateTime<TZ> {
    tokens
        .peek()
        .and_then(|s| s.parse::<i32>().ok())
        .and_then(|quantity| {
            tokens.next();
            tokens
                .peek()
                .and_then(|unit| DURATION_UNITS.get(*unit))
                .copied()
                .map(|dur| {
                    tokens.next();
                    dur * quantity
                })
        })
        .map(|duration| when.clone() + duration)
        .map(|d| {
            if let Some("e") = tokens.peek().copied() {
                tokens.next();
            };
            advance_time(d, tokens)
        })
        .unwrap_or(when)
}

fn configure_weekday<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    when: DateTime<TZ>,
    tokens: &mut Peekable<T>,
) -> DateTime<TZ> {
    try_parse_weekday(tokens)
        .map(|weekday| next_weekday(*weekday, when.clone()))
        .unwrap_or(when)
}

fn at_date<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    lower_bound: &DateTime<TZ>,
    when: DateTime<TZ>,
    tokens: &mut Peekable<T>,
) -> DateTime<TZ> {
    if let Some(d) = parse_date(tokens).and_then(|date| {
        when.with_year(date.year_ce().1 as i32)
            .and_then(|d| d.with_month0(date.month0()))
            .and_then(|d| d.with_day0(date.day0()))
    }) {
        return d;
    }

    let day = match try_parse_day(tokens) {
        None => return when,
        day => day,
    };

    let month = match try_parse_month(tokens).map(|m| m.number_from_month()) {
        None => return try_set_day(day, when, lower_bound),
        month => month,
    };

    match try_parse_year(tokens) {
        None => try_set_month(month, when.clone(), lower_bound)
            .with_day(1)
            .map(|d| try_set_day(day, d, lower_bound))
            .unwrap_or(when),
        Some(year) => set_year(year, when.clone())
            .with_month(1)
            .map(|d| try_set_month(month, d, lower_bound))
            .and_then(|d| d.with_day(1))
            .map(|d| try_set_day(day, d, lower_bound))
            .unwrap_or(when.clone()),
    }
}

fn at_month<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    when: DateTime<TZ>,
    tokens: &mut Peekable<T>,
) -> DateTime<TZ> {
    try_parse_month(tokens)
        .and_then(|month| when.with_month(month.number_from_month()))
        .map(|date| at_year(date, tokens))
        .unwrap_or(when)
}

fn at_beginning_of_year<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    when: DateTime<TZ>,
    tokens: &mut Peekable<T>,
) -> DateTime<TZ> {
    try_parse_year(tokens)
        .and_then(|year| {
            when.with_month0(0)
                .and_then(|d| d.with_day0(0))
                .map(|d| set_year(year, d))
        })
        .unwrap_or(when)
}

fn at_time<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    lower_bound: &DateTime<TZ>,
    when: DateTime<TZ>,
    tokens: &mut Peekable<T>,
) -> DateTime<TZ> {
    try_set_time(try_parse_time(tokens).as_ref(), when, lower_bound)
}

fn at_year<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    when: DateTime<TZ>,
    tokens: &mut Peekable<T>,
) -> DateTime<TZ> {
    try_parse_year(tokens)
        .and_then(|year| when.with_year(year))
        .unwrap_or(when)
}

fn try_set_time<TZ: TimeZone>(
    time: Option<&NaiveTime>,
    when: DateTime<TZ>,
    lower_bound: &DateTime<TZ>,
) -> DateTime<TZ> {
    time.and_then(|time| {
        set_time(time, &when)
            .filter(|date| *date >= *lower_bound)
            .or_else(|| set_time(time, &(when.clone() + Duration::hours(24))))
    })
    .unwrap_or(when)
}

fn try_set_day<TZ: TimeZone>(
    day: Option<u32>,
    when: DateTime<TZ>,
    lower_bound: &DateTime<TZ>,
) -> DateTime<TZ> {
    day.and_then(|day| {
        when.with_day(day)
            .filter(|date| *date >= *lower_bound)
            .or_else(|| next_month(&when).with_day(day))
    })
    .unwrap_or(when)
}

fn try_set_month<TZ: TimeZone>(
    month: Option<u32>,
    when: DateTime<TZ>,
    lower_bound: &DateTime<TZ>,
) -> DateTime<TZ> {
    month
        .and_then(|month| {
            when.with_month(month)
                .filter(|date| *date >= *lower_bound)
                .or_else(|| next_year(&when).with_month(month))
        })
        .unwrap_or(when)
}

fn next_weekday<TZ: TimeZone>(weekday: Weekday, when: DateTime<TZ>) -> DateTime<TZ> {
    let current_weekday = when.weekday().num_days_from_monday();
    let next_weekday = weekday.num_days_from_monday();
    let skip_days = (next_weekday - current_weekday + 7) % 7;
    when + Duration::days(skip_days as i64)
}

fn next_month<TZ: TimeZone>(when: &DateTime<TZ>) -> DateTime<TZ> {
    let (y, m0) = (when.year(), when.month0());
    when.clone() + Duration::days(days_of_month0(y, m0))
}

fn next_year<TZ: TimeZone>(when: &DateTime<TZ>) -> DateTime<TZ> {
    let y = when.year();
    when.clone() + Duration::days(days_of_year(y))
}

fn set_year<TZ: TimeZone>(year: i32, when: DateTime<TZ>) -> DateTime<TZ> {
    when.with_year(year).unwrap_or(when)
}

fn try_parse_year<'a, T: Iterator<Item = &'a str>>(tokens: &mut Peekable<T>) -> Option<i32> {
    tokens
        .peek()
        .and_then(|s| s.parse::<i32>().ok())
        .inspect(|_| {
            tokens.next();
        })
}

fn try_parse_month<'a, T: Iterator<Item = &'a str>>(tokens: &mut Peekable<T>) -> Option<&Month> {
    tokens.peek().and_then(|s| MONTHS.get(*s)).inspect(|_| {
        tokens.next();
    })
}

fn try_parse_weekday<'a, T: Iterator<Item = &'a str>>(
    tokens: &mut Peekable<T>,
) -> Option<&Weekday> {
    tokens.peek().and_then(|s| WEEKDAYS.get(*s)).inspect(|_| {
        tokens.next();
    })
}

fn try_parse_position<'a, T: Iterator<Item = &'a str>>(tokens: &mut Peekable<T>) -> Option<u32> {
    tokens
        .peek()
        .and_then(|s| POSITIONS.get(*s))
        .inspect(|_| {
            tokens.next();
        })
        .copied()
}

fn try_parse_day<'a, T: Iterator<Item = &'a str>>(tokens: &mut Peekable<T>) -> Option<u32> {
    tokens
        .peek()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|day| *day < 32)
        .inspect(|_| {
            tokens.next();
        })
}

fn set_time<TZ: TimeZone>(time: &NaiveTime, when: &DateTime<TZ>) -> Option<DateTime<TZ>> {
    when.with_second(time.second())
        .and_then(|d| d.with_minute(time.minute()))
        .and_then(|d| d.with_hour(time.hour()))
}

fn try_parse_time<'a, T: Iterator<Item = &'a str>>(tokens: &mut Peekable<T>) -> Option<NaiveTime> {
    parse_time(tokens).or_else(|| custom_parse_time(tokens))
}

fn custom_parse_time<'a, T: Iterator<Item = &'a str>>(
    tokens: &mut Peekable<T>,
) -> Option<NaiveTime> {
    let hour = match tokens
        .peek()
        .and_then(|h| h.parse::<u32>().ok())
        .inspect(|_| {
            tokens.next();
        }) {
        None => return None,
        Some(h) => h,
    };

    tokens
        .peek()
        .copied()
        .filter(|e| *e == "e")
        .and_then(|_| {
            tokens.next();
            tokens.peek().copied()
        })
        .and_then(|m| m.parse::<u32>().ok())
        .inspect(|_| {
            tokens.next();
        })
        .and_then(|minute| NaiveTime::from_hms_opt(hour, minute, 0))
        .or_else(|| NaiveTime::from_hms_opt(hour, 0, 0))
}

fn parse_date<'a, T: Iterator<Item = &'a str>>(tokens: &mut Peekable<T>) -> Option<NaiveDate> {
    tokens
        .peek()
        .and_then(|date| {
            DATE_FORMATS
                .iter()
                .filter_map(|fmt| NaiveDate::parse_from_str(date, fmt).ok())
                .next()
        })
        .inspect(|_| {
            tokens.next();
        })
}

fn parse_time<'a, T: Iterator<Item = &'a str>>(tokens: &mut Peekable<T>) -> Option<NaiveTime> {
    tokens
        .peek()
        .and_then(|time| {
            TIME_FORMATS
                .iter()
                .filter_map(|fmt| NaiveTime::parse_from_str(time, fmt).ok())
                .next()
        })
        .inspect(|_| {
            tokens.next();
        })
}

fn days_of_month0(year: i32, month: u32) -> i64 {
    // Create a NaiveDate for the first day of the given month
    let first_day_of_month = NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap();

    let first_day_of_next_month =
        NaiveDate::from_ymd_opt(year + ((month as i32 + 2) / 12), (month + 1) % 12 + 1, 1).unwrap();

    first_day_of_next_month
        .signed_duration_since(first_day_of_month)
        .num_days()
}

fn days_of_year(year: i32) -> i64 {
    // Create a NaiveDate for the first day of the given month
    let first_day_of_year = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();

    let first_day_of_next_year = NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap();

    first_day_of_next_year
        .signed_duration_since(first_day_of_year)
        .num_days()
}

fn truncated_by_day<Tz: TimeZone>(date: &DateTime<Tz>) -> DateTime<Tz> {
    date.with_hour(0)
        .and_then(|d| d.with_minute(0))
        .and_then(|d| d.with_second(0))
        .and_then(|d| d.with_nanosecond(0))
        .unwrap_or(date.clone())
}

#[cfg(test)]
mod once_tests {
    use super::try_parse;
    use crate::interface::Schedule;
    use chrono::{DateTime, Utc};
    use ntest::timeout;

    #[test]
    #[timeout(50)]
    fn test_ricordami_il_18() {
        assert_schedule_once(
            "Ricordami il 18",
            "2024-08-17T20:58:00+02:00",
            "2024-08-18T20:58:00+02:00",
        );
    }

    #[test]
    #[timeout(50)]
    fn test_ricordami_tra_60_secondi_2_settimane_e_1_minuto() {
        assert_schedule_once(
            "Ricordami tra 60 secondi 2 settimane e 1 minuto",
            "2024-08-17T20:58:00+02:00",
            "2024-08-31T21:00:00+02:00",
        );
    }

    #[test]
    #[timeout(50)]
    fn test_ricordami_il_18_alle_00_01() {
        assert_schedule_once(
            "Ricordami il 18 alle 00:01",
            "2024-08-17T20:58:00+02:00",
            "2024-08-18T00:01:00+02:00",
        );
    }

    #[test]
    #[timeout(50)]
    fn test_ricordami_alle_2_e_59() {
        assert_schedule_once(
            "Ricordami alle 2 e 59",
            "2024-08-17T20:58:00+02:00",
            "2024-08-18T02:59:00+02:00",
        );
    }

    #[test]
    #[timeout(50)]
    fn test_ricordami_venerdì_alle_00() {
        assert_schedule_once(
            "Ricordami venerdì alle 00",
            "2024-08-17T20:58:00+02:00",
            "2024-08-18T00:00:00+02:00",
        );
    }

    #[test]
    #[timeout(50)]
    fn test_ricordami_nel_2025() {
        assert_schedule_once(
            "Ricordami nel 2025",
            "2024-08-17T20:58:00+02:00",
            "2025-01-01T20:58:00+01:00",
        );
    }

    #[test]
    #[timeout(50)]
    fn test_ricordami_il_12_maggio_2025() {
        assert_schedule_once(
            "Ricordami il 12 maggio 2025",
            "2024-08-17T20:58:00+02:00",
            "2025-05-12T20:58:00+02:00",
        );
    }

    #[test]
    #[timeout(50)]
    fn test_ricordami_l_1() {
        assert_schedule_once(
            "Ricordami l 1",
            "2024-08-17T20:58:00+02:00",
            "2024-09-01T20:58:00+02:00",
        );
    }

    #[test]
    #[timeout(50)]
    fn test_ricordami_ad_agosto_2025_alle_02_20() {
        assert_schedule_once(
            "Ricordami ad agosto 2025 alle 02:20",
            "2024-08-17T20:58:00+02:00",
            "2025-08-17T02:20:00+02:00",
        );
    }

    #[test]
    #[timeout(50)]
    fn test_ricordami_il_12_05_2025_alle_13_e_20() {
        assert_schedule_once(
            "Ricordami il 12/05/2025 alle 13 e 20",
            "2024-08-17T20:58:00+02:00",
            "2025-05-12T13:20:00+02:00",
        );
    }

    #[test]
    #[timeout(50)]
    fn test_ricordami_il_3_dicembre_alle_13_e_20() {
        assert_schedule_once(
            "Ricordami il 3 dicembre alle 13 e 20",
            "2024-08-17T20:58:00+02:00",
            "2024-12-03T13:20:00+01:00",
        );
    }

    #[test]
    #[timeout(50)]
    fn test_ricordami_il_3_dicembre_alle_13_20_ignores_minutes() {
        assert_schedule_once(
            "Ricordami il 3 dicembre alle 13 20",
            "2024-08-17T20:58:00+02:00",
            "2024-12-03T13:00:00+01:00",
        );
    }

    #[test]
    #[timeout(50)]
    fn test_ricordami_per_domani_does_not_work() {
        assert_schedule_once_none("Ricordami per domani", "2024-08-17T20:58:00+02:00");
    }

    fn assert_schedule_once(msg: &str, date_str: &str, expected_when_str: &str) {
        assert_eq_schedule(
            msg,
            try_parse(
                msg.split(' ').collect(),
                &date_str.parse::<DateTime<Utc>>().unwrap(),
            ),
            Some(Schedule::Once {
                when: expected_when_str.parse::<DateTime<Utc>>().unwrap(),
            }),
        )
    }

    fn assert_schedule_once_none(msg: &str, date_str: &str) {
        assert_eq_schedule(
            msg,
            try_parse(
                msg.split(' ').collect(),
                &date_str.parse::<DateTime<Utc>>().unwrap(),
            ),
            None,
        )
    }

    fn assert_eq_schedule(msg: &str, result: Option<Schedule>, expected: Option<Schedule>) {
        assert_eq!(expected, result, "When parsing \"{msg}\"");
    }
}

#[cfg(test)]
mod recurrent_tests {
    use super::try_parse;
    use crate::{interface::Schedule, schedule::ScheduleGrid};
    use chrono::{DateTime, Duration, Utc};
    use chrono_tz::Europe;
    use ntest::timeout;

    #[test]
    #[timeout(50)]
    fn ricordami_ogni_2_anni() {
        assert_schedule_recurrent(
            "Ricordami ogni 2 anni",
            "2024-08-17T20:58:00+02:00",
            "2024-08-17T20:58:00+02:00",
            |_, grid, _| {
                assert_eq!(2u8, grid.year_cadence.get());
            },
        );
    }

    #[test]
    #[timeout(50)]
    fn ricordami_ogni_2_anni_ogni_primo_sabato_di_gennaio_dal_1_1_2025() {
        assert_schedule_recurrent(
            "Ricordami ogni 2 anni ogni primo sabato di gennaio dal 1/1/2025",
            "2024-08-17T20:58:00+02:00",
            "2025-01-01T20:58:00+01:00",
            |schedule, grid, now| {
                let result = schedule
                    .next_tick(now)
                    .unwrap()
                    .with_timezone(&Europe::Rome)
                    .to_rfc3339();

                assert_eq!(2u8, grid.year_cadence.get());
                assert_eq!(2025u32, grid.year_start);
                assert_eq!(
                    "2025-01-04T20:58:00+01:00", result,
                    "Next schedule available"
                );
            },
        );
    }

    #[test]
    #[timeout(50)]
    fn ricordami_ogni_primo_e_terzo_giovedì() {
        assert_schedule_recurrent(
            "Ricordami ogni primo e terzo giovedì alle 00:00",
            "2024-08-17T20:58:00+02:00",
            "2024-08-17T20:58:00+02:00",
            |schedule, grid, now| {
                let result = schedule
                    .next_tick(now)
                    .unwrap()
                    .with_timezone(&Europe::Rome)
                    .to_rfc3339();

                assert!(grid.weeks_of_month.get(0usize), "Expecting first week set");
                assert!(grid.weeks_of_month.get(2usize), "Expecting third week set");
                assert!(
                    grid.days_of_week.get(3usize),
                    "Expecting fourth day of week set"
                );
                assert_eq!(
                    "2024-09-05T00:00:00+02:00", result,
                    "Next schedule available"
                );
            },
        );
    }

    #[test]
    #[timeout(50)]
    fn ricordami_ogni_primo_e_terzo_giovedì_e_venerdì() {
        assert_schedule_recurrent(
            "Ricordami ogni primo e terzo giovedì e venerdì alle 00:00",
            "2024-08-17T20:58:00+02:00",
            "2024-08-17T20:58:00+02:00",
            |schedule, grid, now| {
                let result = schedule
                    .next_tick(now)
                    .unwrap()
                    .with_timezone(&Europe::Rome)
                    .to_rfc3339();

                assert!(grid.weeks_of_month.get(0usize), "Expecting first week set");
                assert!(grid.weeks_of_month.get(2usize), "Expecting third week set");
                assert!(
                    grid.days_of_week.get(3usize),
                    "Expecting fourth day of week set"
                );
                assert!(
                    grid.days_of_week.get(4usize),
                    "Expecting fifth day of week set"
                );
                assert_eq!(
                    "2024-09-05T00:00:00+02:00", result,
                    "Next schedule available"
                );
            },
        );
    }

    #[test]
    #[timeout(50)]
    fn ricordami_ogni_12_05_alle_13() {
        assert_schedule_recurrent(
            "Ricordami ogni 12/05 alle 13",
            "2024-08-17T20:58:00+02:00",
            "2024-08-17T20:58:00+02:00",
            |schedule, grid, now| {
                let result = schedule
                    .next_tick(now)
                    .unwrap()
                    .with_timezone(&Europe::Rome)
                    .to_rfc3339();

                assert!(
                    grid.days_of_month.get(11usize),
                    "Expecting twelfth day of month set"
                );
                assert!(
                    grid.months_of_year.get(4usize),
                    "Expecting fifth month of year set"
                );
                assert_eq!(
                    "2025-05-12T13:00:00+02:00", result,
                    "Next schedule available"
                );
            },
        );
    }

    #[test]
    #[timeout(50)]
    fn ricordami_ogni_mercoledi_alle_20_da_ottobre() {
        assert_schedule_recurrent(
            "Ricordami ogni mercoledì alle 20 da ottobre",
            "2024-08-17T20:58:00+02:00",
            "2024-10-17T20:58:00+02:00",
            |schedule, _, now| {
                let result = schedule
                    .next_tick(now)
                    .unwrap()
                    .with_timezone(&Europe::Rome)
                    .to_rfc3339();
                assert_eq!(
                    "2024-10-23T20:00:00+02:00", result,
                    "Next schedule available"
                );
            },
        );
    }

    #[test]
    #[timeout(50)]
    fn ricordami_ogni_mercoledi_alle_20_da_ottobre_a_dicembre() {
        assert_schedule_recurrent_until(
            "Ricordami ogni mercoledì alle 20 da ottobre a dicembre",
            "2024-08-17T20:58:00+02:00",
            "2024-10-17T20:58:00+02:00",
            "2024-12-17T20:58:00+01:00",
            |schedule, _, now| {
                let result = schedule
                    .next_tick(now)
                    .unwrap()
                    .with_timezone(&Europe::Rome)
                    .to_rfc3339();
                assert_eq!(
                    "2024-10-23T20:00:00+02:00", result,
                    "Next schedule available"
                );
            },
        );
    }

    #[test]
    #[timeout(50)]
    fn ricordami_ogni_mercoledi_alle_20_da_ottobre_a_dicembre_after() {
        assert_schedule_recurrent_until(
            "Ricordami ogni mercoledì alle 20 da ottobre a dicembre",
            "2024-08-17T20:58:00+02:00",
            "2024-10-17T20:58:00+02:00",
            "2024-12-17T20:58:00+01:00",
            |schedule, _, now| {
                let one_year_later = *now + Duration::days(365);
                let result = schedule.next_tick(&one_year_later);
                assert_eq!(
                    None, result,
                    "No next schedule available at {}",
                    one_year_later
                );
            },
        );
    }

    #[test]
    #[timeout(100)]
    fn ricordami_ogni_venerdì_alle_18_dal_30_agosto_al_26_ottobre() {
        assert_schedule_recurrent_until_sequence(
            "Ricordami ogni venerdì alle 18 dal 29 agosto al 26 ottobre",
            "2024-08-17T20:58:00+02:00",
            &[
                "2024-08-30T18:00:00+02:00",
                "2024-09-06T18:00:00+02:00",
                "2024-09-13T18:00:00+02:00",
                "2024-09-20T18:00:00+02:00",
                "2024-09-27T18:00:00+02:00",
                "2024-10-04T18:00:00+02:00",
                "2024-10-11T18:00:00+02:00",
                "2024-10-18T18:00:00+02:00",
                "2024-10-25T18:00:00+02:00",
            ],
        );
    }

    fn assert_schedule_recurrent<F>(
        msg: &str,
        date_str: &str,
        expected_since_str: &str,
        assertions: F,
    ) where
        F: FnOnce(&Schedule, &ScheduleGrid, &DateTime<Utc>),
    {
        let now = &date_str.parse::<DateTime<Utc>>().unwrap();
        match try_parse(msg.split(' ').collect(), now).as_ref() {
            Some(
                schedule @ Schedule::Recurrent {
                    since,
                    schedule: grid,
                },
            ) => {
                assert_eq!(
                    *since,
                    expected_since_str.parse::<DateTime<Utc>>().unwrap(),
                    "Comparing since for {} @ {}",
                    msg,
                    date_str
                );
                assertions(schedule, grid, now);
            }
            _ => {
                assert!(false, "Expecting Recurrent for {} @ {}", msg, date_str);
            }
        }
    }

    fn assert_schedule_recurrent_until<F>(
        msg: &str,
        date_str: &str,
        expected_since_str: &str,
        expected_until_str: &str,
        assertions: F,
    ) where
        F: FnOnce(&Schedule, &ScheduleGrid, &DateTime<Utc>),
    {
        let now = &date_str.parse::<DateTime<Utc>>().unwrap();
        match try_parse(msg.split(' ').collect(), now).as_ref() {
            Some(
                schedule @ Schedule::RecurrentUntil {
                    since,
                    until,
                    schedule: grid,
                },
            ) => {
                assert_eq!(
                    expected_since_str.parse::<DateTime<Utc>>().unwrap(),
                    *since,
                    "Comparing since for {} @ {}",
                    msg,
                    date_str
                );
                assert_eq!(
                    expected_until_str.parse::<DateTime<Utc>>().unwrap(),
                    *until,
                    "Comparing until for {} @ {}",
                    msg,
                    date_str
                );
                assertions(schedule, grid, now);
            }
            _ => {
                assert!(false, "Expecting RecurrentUntil for {} @ {}", msg, date_str);
            }
        }
    }

    fn assert_schedule_recurrent_until_sequence(
        msg: &str,
        date_str: &str,
        expected_sequence: &[&str],
    ) {
        let now = &date_str.parse::<DateTime<Utc>>().unwrap();
        match try_parse(msg.split(' ').collect(), now) {
            Some(schedule @ Schedule::RecurrentUntil { .. }) => {
                let mut current = *now;
                let dates = std::iter::from_fn(|| {
                    schedule.next_tick(&current).inspect(|next| current = *next)
                })
                .map(|d| d.with_timezone(&Europe::Rome).to_rfc3339())
                .collect::<Vec<_>>();
                assert_eq!(
                    expected_sequence,
                    dates.as_slice(),
                    "Expecting sequence of dates for {} @ {}",
                    msg,
                    date_str
                );
            }
            _ => {
                assert!(false, "Expecting RecurrentUntil for {} @ {}", msg, date_str);
            }
        }
    }

    fn assert_schedule_none(msg: &str, date_str: &str) {
        assert_eq!(
            None,
            try_parse(
                msg.split(' ').collect(),
                &date_str.parse::<DateTime<Utc>>().unwrap(),
            ),
            "When parsing \"{msg}\""
        );
    }
}
