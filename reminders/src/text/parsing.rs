use std::{collections::HashMap, iter::Peekable};

use chrono::{
    DateTime, Datelike, Duration, Month, NaiveDate, NaiveTime, TimeZone, Timelike, Utc, Weekday,
};
use chrono_tz::{Europe, Tz};
use lazy_static::lazy_static;

use crate::interface::Schedule;

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
}

/**
 * REM := "Ricordati " SCHEDULE "\n" MSG
 * SCHEDULE := ONCE | RECURRENT
 *
 * ONCE := AT TIME | DATE | DATE AT TIME | AT TIME DATE
 * AT := "alle"
 * TIME := HH | HH SEP_TIME MM
 * SEP_TIME :=  ":" | "e" | "."
 * HH := 0 | 1 | ... | 23
 * MM := 0 | 1 | ... | 59
 *
 * DATE := WEEK_DAY | ON DAY | AT_MONTH MONTH | AT_MONTH MONTH YEAR | "nel" YEAR
 * AT_MONTH := "a" | "ad"
 * ON := "il" | "l'"
 * WEEK_DAY := "lunedì" | ... | "domenica"
 * MONTH := "gennaio" | ... | "dicembre"
 * YEAR := 1970 | ... | 3000 | ...
 * DAY := DAY_NUM | DAY_NUM SEP_DATE MONTH_NUM | DAY_NUM SEP_DATE MONTH_NUM SEP_DATE YEAR | DAY_NUM MONTH YEAR | DAY_NUM MONTH
 * DAY_NUM := 1 | .. | 31 | 01 | .. | 09
 * MONTH_NUM := 01 | .. | 09 | 1 | .. | 12
 * SEP_DATE := "/" | "-" | "."
 *
 * RECURRENT := TODO
 *
 * Examples:
 * ONCE
 * - Ricordati alle 14
 * - Ricordati alle 14 e 20
 * - Ricordati alle 8:20
 * - Ricordati alle 11 e 37
 * - Ricordati il 13 alle 11 e 37
 * - Ricordati a gennaio alle 11 e 37
 * - Ricordati nel 2024 alle 11 e 37
 * - Ricordati il 13-09-2024 alle 11 e 37
 * - Ricordati tra 4 giorni
 * - Ricordati sabato
 * Generic
 * - Ricordati [data] [alle]
 *
 * EVER RECURRENT
 * - Ricordati ogni sabato
 * - Ricordati ogni primo,terzo sabato
 * - Ricordati ogni primo lunedì,mercoledì
 * - Ricordati ogni 12/05
 * - Ricordati ogni 12
 * - Ricordati ogni settembre
 * - Ricordati ogni giorno
 * - Ricordati ogni mese
 * - Ricordati ogni ora
 * - Ricordati ogni minuto
 * - Ricordati dall'11 gennaio ogni ...
 * Generic
 * - Ricordati dal ... [ogni ...] [alle ...]
 * - Ricordati [ogni ...] [alle ...] dal ...
 *
 * RECURRENT UNTIL
 * Generic
 * - Ricordati dal ... al ... [ogni ...] [alle ...]
 * - Ricordati fino al ... [ogni ...] [alle ...]
 * - Ricordati [ogni ...] [alle ...] dal ... al ...
 * - Ricordati [ogni ...] [alle ...] fino al ...
 *
 * Hints:
 * 1. ONCE iff !ogni
 * 2. EVER RECURRENT iff !(al || fino al)
 * 3. "alle" always TIME
 * 4. number < 32 => day
 * 5. XX/YY => XX = day, YY = month
 * 6. number > 1969 => year
 * 7. there is "primo","secondo","terzo","quarto","quinto"
 * 8. ogni default is "ogni giorno"
 * 9. alle default is "12:00 Europe/Rome"
 * 10. default MM is "00"
 * 11. (bonus) always find nearest next match:
 *   - alle 14, but now is 15 => tomorrow at 14
 *   - a gennaio, but now is marzo => next year at gennaio
 */
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
        Some("alle") | Some("a") | Some("il") | Some("lo") | Some("l") | Some("nel")
        | Some("ad") | Some("tra") => build_once(tokens, &context),
        Some(x) if WEEKDAYS.contains_key(x) => build_once(tokens, &context),
        Some("ogni") | Some("fino") | Some("dal") | Some("dall") => {
            build_recurrent(tokens, &context, tz)
        }
        _ => None,
    }
}

fn build_recurrent<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    tokens: Peekable<T>,
    now: &DateTime<TZ>,
    tz: &Tz,
) -> Option<Schedule> {
    None
}

fn build_once<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    mut tokens: Peekable<T>,
    now: &DateTime<TZ>,
) -> Option<Schedule> {
    tracing::info!("BUILD ONCE");
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
        .filter(|e| *e != "e")
        .or_else(|| {
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

#[test]
fn test_ricordami_il_18() {
    assert_schedule_once(
        "Ricordami il 18",
        "2024-08-17T20:58:00+02:00",
        "2024-08-18T20:58:00+02:00",
    );
}

#[test]
fn test_ricordami_tra_60_secondi_2_settimane_e_1_minuto() {
    assert_schedule_once(
        "Ricordami tra 60 secondi 2 settimane e 1 minuto",
        "2024-08-17T20:58:00+02:00",
        "2024-08-31T21:00:00+02:00",
    );
}

#[test]
fn test_ricordami_il_18_alle_00_01() {
    assert_schedule_once(
        "Ricordami il 18 alle 00:01",
        "2024-08-17T20:58:00+02:00",
        "2024-08-18T00:01:00+02:00",
    );
}

#[test]
fn test_ricordami_alle_2_e_59() {
    assert_schedule_once(
        "Ricordami alle 2 e 59",
        "2024-08-17T20:58:00+02:00",
        "2024-08-18T02:59:00+02:00",
    );
}

#[test]
fn test_ricordami_venerdì_alle_00() {
    assert_schedule_once(
        "Ricordami venerdì alle 00",
        "2024-08-17T20:58:00+02:00",
        "2024-08-18T00:00:00+02:00",
    );
}

#[test]
fn test_ricordami_nel_2025() {
    assert_schedule_once(
        "Ricordami nel 2025",
        "2024-08-17T20:58:00+02:00",
        "2025-01-01T20:58:00+01:00",
    );
}

#[test]
fn test_ricordami_il_12_maggio_2025() {
    assert_schedule_once(
        "Ricordami il 12 maggio 2025",
        "2024-08-17T20:58:00+02:00",
        "2025-05-12T20:58:00+02:00",
    );
}

#[test]
fn test_ricordami_l_1() {
    assert_schedule_once(
        "Ricordami l 1",
        "2024-08-17T20:58:00+02:00",
        "2024-09-01T20:58:00+02:00",
    );
}

#[test]
fn test_ricordami_ad_agosto_2025_alle_02_20() {
    assert_schedule_once(
        "Ricordami ad agosto 2025 alle 02:20",
        "2024-08-17T20:58:00+02:00",
        "2025-08-17T02:20:00+02:00",
    );
}

#[test]
fn test_ricordami_il_12_05_2025_alle_13_e_20() {
    assert_schedule_once(
        "Ricordami il 12/05/2025 alle 13 e 20",
        "2024-08-17T20:58:00+02:00",
        "2025-05-12T13:20:00+02:00",
    );
}

#[test]
fn test_ricordami_il_3_dicembre_alle_13_e_20() {
    assert_schedule_once(
        "Ricordami il 3 dicembre alle 13 e 20",
        "2024-08-17T20:58:00+02:00",
        "2024-12-03T13:20:00+01:00",
    );
}

#[test]
fn test_ricordami_per_domani_does_not_work() {
    assert_schedule_once_none("Ricordami per domani", "2024-08-17T20:58:00+02:00");
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
fn assert_eq_schedule(msg: &str, result: Option<Schedule>, expected: Option<Schedule>) {
    assert_eq!(expected, result, "When parsing \"{msg}\"");
}
