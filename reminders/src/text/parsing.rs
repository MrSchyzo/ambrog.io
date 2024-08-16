use std::{collections::HashMap, iter::Peekable};

use chrono::{DateTime, Days, Duration, Month, NaiveTime, TimeZone, Timelike, Utc, Weekday};
use chrono_tz::{Europe, Tz};
use lazy_static::lazy_static;

use crate::interface::Schedule;

lazy_static! {
    static ref WEEKDAYS: HashMap<String, Weekday> = {
        let mut lookup = HashMap::with_capacity(7);
        lookup.insert("lunedì".to_owned(), Weekday::Mon);
        lookup.insert("martedì".to_owned(), Weekday::Tue);
        lookup.insert("mercoledì".to_owned(), Weekday::Wed);
        lookup.insert("giovedì".to_owned(), Weekday::Thu);
        lookup.insert("venerdì".to_owned(), Weekday::Fri);
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
    let mut when = now.clone();
    while let Some(token) = tokens.next() {
        when = match token {
            "tra" => advance_time(when, &mut tokens),
            "alle" => configure_time(when, &mut tokens),
            "il" | "lo" | "l" => configure_date(when, &mut tokens),
            "a" | "ad" => configure_month(when, &mut tokens),
            "nel" => configure_year(when, &mut tokens),
            x if WEEKDAYS.contains_key(x) => configure_weekday(when, &mut tokens),
            _ => when,
        };
    }
    Some(Schedule::Once {
        when: when.with_timezone(&Utc),
    })
}

fn advance_time<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    when: DateTime<TZ>,
    tokens: &mut Peekable<T>,
) -> DateTime<TZ> {
    tracing::warn!("TODO: advance_time");
    when
}

fn configure_weekday<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    when: DateTime<TZ>,
    tokens: &mut Peekable<T>,
) -> DateTime<TZ> {
    tracing::warn!("TODO: configure_weekday");
    when
}

fn configure_date<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    when: DateTime<TZ>,
    tokens: &mut Peekable<T>,
) -> DateTime<TZ> {
    tracing::warn!("TODO: configure_date");
    when
}

fn configure_month<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    when: DateTime<TZ>,
    tokens: &mut Peekable<T>,
) -> DateTime<TZ> {
    tracing::warn!("TODO: configure_month");
    when
}

fn configure_year<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    when: DateTime<TZ>,
    tokens: &mut Peekable<T>,
) -> DateTime<TZ> {
    tracing::warn!("TODO: configure_year");
    when
}

fn configure_time<'a, TZ: TimeZone, T: Iterator<Item = &'a str>>(
    when: DateTime<TZ>,
    tokens: &mut Peekable<T>,
) -> DateTime<TZ> {
    try_set_time(try_parse_time(tokens), when)
}

fn try_set_time<TZ: TimeZone>(time: Option<NaiveTime>, when: DateTime<TZ>) -> DateTime<TZ> {
    time.and_then(|time| {
        when.with_second(time.second())
            .and_then(|d| d.with_minute(time.minute()))
            .and_then(|d| d.with_hour(time.hour()))
    })
    .and_then(|date| {
        if date < when {
            date.checked_add_days(Days::new(1))
        } else {
            Some(date)
        }
    })
    .unwrap_or(when)
}

fn try_parse_time<'a, T: Iterator<Item = &'a str>>(tokens: &mut Peekable<T>) -> Option<NaiveTime> {
    let h = match tokens.next() {
        None => return None,
        Some(h) => h,
    };

    if let Ok(time) = NaiveTime::parse_from_str(h, "%H:%M") {
        return Some(time);
    }

    if let Ok(time) = NaiveTime::parse_from_str(h, "%H:%M:%S") {
        return Some(time);
    }

    let hour = match h.parse::<u32>() {
        Err(_) => return None,
        Ok(hour) => hour,
    };

    match tokens.peek().copied() {
        Some("e") => {
            tokens.next();
            tokens.peek().copied()
        }
        x @ Some(_) => x,
        None => return NaiveTime::from_hms_opt(hour, 0, 0),
    }
    .and_then(|m| m.parse::<u32>().ok())
    .and_then(|min| {
        tokens.next();
        NaiveTime::from_hms_opt(hour, min, 0)
    })
}
