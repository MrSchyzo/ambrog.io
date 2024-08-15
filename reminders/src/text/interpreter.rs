use chrono::{DateTime, Utc};
use chrono_tz::Europe;

use crate::interface::{Schedule, ScheduleGridBuilder};

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
pub fn try_interpret_definition(
    tokens: Vec<&str>,
    message: &str,
    now: &DateTime<Utc>,
) -> Option<Schedule> {
    tracing::warn!("TODO: try_interpret_definition has to be implemented yet!");
    let schedule_grid = ScheduleGridBuilder::new(Europe::Rome);
    let context = now.with_timezone(&Europe::Rome);

    let tokens = tokens.iter().skip(1);

    for token in tokens {
        return Some(Schedule::every_fucking_minute_of_your_damn_life(
            Some(now).cloned(),
        ));
    }

    None
}
