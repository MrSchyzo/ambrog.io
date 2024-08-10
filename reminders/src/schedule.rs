use super::bitmap::Bitmap;
use std::num::{NonZeroU8, NonZeroUsize};

use chrono::{DateTime, Datelike, Timelike};
use chrono_tz::Tz;

pub struct ScheduleGrid {
    minutes: Bitmap,
    hours: Bitmap,
    days_of_month: Bitmap,
    days_of_week: Bitmap,
    months_of_year: Bitmap,
    year_cadence: NonZeroU8,
    year_start: u32,
}

impl ScheduleGrid {
    pub fn verbose_new(
        minutes: Vec<usize>,
        hours: Vec<usize>,
        days_of_month: Vec<usize>,
        days_of_week: Vec<usize>,
        months_of_year: Vec<usize>,
        year_cadence: NonZeroU8,
        year_start: u32,
    ) -> Self {
        Self {
            minutes: Bitmap::new_truncated(NonZeroUsize::new(60).unwrap(), minutes),
            hours: Bitmap::new_truncated(NonZeroUsize::new(60).unwrap(), hours),
            days_of_month: Bitmap::new_truncated(NonZeroUsize::new(31).unwrap(), days_of_month),
            days_of_week: Bitmap::new_truncated(NonZeroUsize::new(7).unwrap(), days_of_week),
            months_of_year: Bitmap::new_truncated(NonZeroUsize::new(12).unwrap(), months_of_year),
            year_cadence,
            year_start,
        }
    }

    pub fn next_scheduled_at(&self, now: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        let now = Self::truncate_to_minute(now);
        let current_year = now.year();
        let year_start = self.year_start as i32;
        let cadence = self.year_cadence.get() as i32;
        let year_skip = cadence + ((current_year - (year_start)) % cadence);

        let mut current_date = match year_skip {
            0 => now,
            _ => Self::set_year(&now, (current_year + year_skip).max(year_start)),
        };

        for _ in 0..50 {
            current_date = match self.find_month(&current_date) {
                d @ Some(_) => return d,
                None => Self::set_year(&current_date, current_date.year() + cadence),
            }
        }

        None
    }

    fn find_month(&self, now: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        let current_month = now.month0() as usize;
        if self.months_of_year.get(current_month) {
            if let d @ Some(_) = self.find_day(now) {
                return d;
            }
        }

        for month in self.months_of_year.iter(current_month) {
            if let d @ Some(_) = Self::set_month0(now, month as u32).and_then(|d| self.find_day(&d))
            {
                return d;
            }
        }

        None
    }

    fn find_day(&self, now: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        let current_day = now.day0() as usize;
        let current_weekday = (now.weekday().number_from_monday() - 1) as usize;
        if self.days_of_month.get(current_day) && self.days_of_week.get(current_weekday) {
            if let d @ Some(_) = self.find_hour(now) {
                return d;
            }
        }

        for day in self.days_of_month.iter(current_day) {
            let current = if let Some(d) = Self::set_day0(now, day as u32) {
                d
            } else {
                continue;
            };
            if !self
                .days_of_week
                .get((now.weekday().number_from_monday() - 1) as usize)
            {
                continue;
            }

            if let d @ Some(_) = self.find_hour(&current) {
                return d;
            }
        }

        None
    }

    fn find_hour(&self, now: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        let current_hour = now.hour() as usize;
        if self.hours.get(current_hour) {
            if let d @ Some(_) = self.find_minute(now) {
                return d;
            }
        }

        for hour in self.hours.iter(current_hour) {
            if let d @ Some(_) = Self::set_hour(now, hour as u32).and_then(|d| self.find_minute(&d))
            {
                return d;
            }
        }

        None
    }

    fn find_minute(&self, now: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        let current_minute = now.minute() as usize;
        if self.minutes.get(current_minute) {
            return Some(*now);
        }

        for minute in self.minutes.iter(current_minute) {
            if let d @ Some(_) = Self::set_minute(now, minute as u32) {
                return d;
            }
        }

        None
    }

    fn set_year(now: &DateTime<Tz>, years: i32) -> DateTime<Tz> {
        Self::set_month0(now, 0)
            .and_then(|dt| dt.with_year(years))
            .unwrap()
    }

    fn set_month0(now: &DateTime<Tz>, month: u32) -> Option<DateTime<Tz>> {
        Self::set_day0(now, 0).and_then(|dt| dt.with_month0(month))
    }

    fn set_day0(now: &DateTime<Tz>, day: u32) -> Option<DateTime<Tz>> {
        Self::set_hour(now, 0).and_then(|dt| dt.with_day0(day))
    }

    fn set_hour(now: &DateTime<Tz>, hour: u32) -> Option<DateTime<Tz>> {
        Self::set_minute(now, 0).and_then(|dt| dt.with_hour(hour))
    }

    fn set_minute(now: &DateTime<Tz>, minute: u32) -> Option<DateTime<Tz>> {
        now.with_minute(minute)
    }

    fn truncate_to_minute(now: &DateTime<Tz>) -> DateTime<Tz> {
        now.with_nanosecond(0)
            .and_then(|dt| dt.with_second(0))
            .unwrap()
    }
}
