use std::num::NonZeroU8;

use chrono::{TimeZone, Utc};
use futures::{Stream, StreamExt};
use mongodb::{
    bson::{self, doc},
    Collection, Database,
};
use serde::{Deserialize, Serialize};

use crate::{
    interface::{ReminderDefinition, Schedule, ScheduleInspection},
    schedule::ScheduleGrid,
};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
struct ReminderMongoloidId {
    user: u64,
    id: i32,
}

#[derive(Serialize, Deserialize)]
struct MongoloidReminder {
    #[serde(rename = "_id")]
    id: ReminderMongoloidId,
    message: String,
    schedule: MongoloidSchedule,
}

#[derive(Serialize, Deserialize)]
struct MongoloidScheduleGrid {
    minutes: Vec<u8>,
    hours: Vec<u8>,
    weeks_of_month: Vec<u8>,
    days_of_month: Vec<u8>,
    days_of_week: Vec<u8>,
    months_of_year: Vec<u8>,
    year_cadence: u8,
    year_start: u32,
    timezone: String,
}

#[derive(Serialize, Deserialize)]
enum MongoloidSchedule {
    Once {
        when_micros: i64,
    },
    Recurrent {
        since_micros: i64,
        schedule: MongoloidScheduleGrid,
    },
    RecurrentUntil {
        since_micros: i64,
        until_micros: i64,
        schedule: MongoloidScheduleGrid,
    },
}

impl From<Schedule> for MongoloidSchedule {
    fn from(value: Schedule) -> Self {
        match value {
            Schedule::Once { when } => Self::Once {
                when_micros: when.timestamp_micros(),
            },
            Schedule::Recurrent { since, schedule } => Self::Recurrent {
                since_micros: since.timestamp_micros(),
                schedule: schedule.into(),
            },
            Schedule::RecurrentUntil {
                since,
                until,
                schedule,
            } => Self::RecurrentUntil {
                since_micros: since.timestamp_micros(),
                until_micros: until.timestamp_micros(),
                schedule: schedule.into(),
            },
        }
    }
}

impl From<MongoloidSchedule> for Schedule {
    fn from(value: MongoloidSchedule) -> Self {
        match value {
            MongoloidSchedule::Once { when_micros } => Self::Once {
                when: Utc.timestamp_micros(when_micros).unwrap(),
            },
            MongoloidSchedule::Recurrent {
                since_micros,
                schedule,
            } => Self::Recurrent {
                since: Utc.timestamp_micros(since_micros).unwrap(),
                schedule: schedule.into(),
            },
            MongoloidSchedule::RecurrentUntil {
                since_micros,
                until_micros,
                schedule,
            } => Self::RecurrentUntil {
                since: Utc.timestamp_micros(since_micros).unwrap(),
                until: Utc.timestamp_micros(until_micros).unwrap(),
                schedule: schedule.into(),
            },
        }
    }
}

impl From<ScheduleGrid> for MongoloidScheduleGrid {
    fn from(value: ScheduleGrid) -> Self {
        let (year_start, year_cadence) = value.inspect_year_and_cadence();
        Self {
            minutes: value.inspect(ScheduleInspection::Minute),
            hours: value.inspect(ScheduleInspection::Hour),
            weeks_of_month: value.inspect(ScheduleInspection::WeekOfMonth),
            days_of_month: value.inspect(ScheduleInspection::DayOfMonth),
            days_of_week: value.inspect(ScheduleInspection::DaysOfWeek),
            months_of_year: value.inspect(ScheduleInspection::MonthsOfYear),
            year_cadence: year_cadence.get(),
            year_start,
            timezone: value.inspect_timezone().name().to_owned(),
        }
    }
}

impl From<MongoloidScheduleGrid> for ScheduleGrid {
    fn from(value: MongoloidScheduleGrid) -> Self {
        Self::new(
            value.minutes.into(),
            value.hours.into(),
            value.weeks_of_month.into(),
            value.days_of_month.into(),
            value.days_of_week.into(),
            value.months_of_year.into(),
            NonZeroU8::new(value.year_cadence).unwrap(),
            value.year_start,
            value.timezone.parse().unwrap(),
        )
    }
}

impl MongoloidReminder {
    pub fn new(definition: &ReminderDefinition, id: i32) -> Self {
        Self {
            id: ReminderMongoloidId {
                user: definition.user_id(),
                id,
            },
            message: definition.message().to_string(),
            schedule: definition.schedule().clone().into(),
        }
    }
}

pub struct MongoloidStorage {
    collection: Collection<MongoloidReminder>,
}

impl MongoloidStorage {
    pub fn new(db: Database) -> Self {
        Self {
            collection: db.collection::<MongoloidReminder>("reminders"),
        }
    }

    pub async fn create(&self, definition: &ReminderDefinition, id: i32) -> bool {
        self.collection
            .insert_one(MongoloidReminder::new(definition, id))
            .await
            .is_ok()
    }
    pub async fn delete(&self, user_id: u64, id: i32) -> bool {
        let id = ReminderMongoloidId { user: user_id, id };
        self.collection
            .delete_one(doc! {"_id": bson::to_bson(&id).unwrap()})
            .await
            .is_ok()
    }
    pub async fn get_all(
        &self,
    ) -> impl Stream<Item = Result<(ReminderDefinition, i32), mongodb::error::Error>> {
        self.collection.find(doc! {}).await.unwrap().map(|rem| {
            rem.map(|reminder| {
                let def = ReminderDefinition::new(
                    reminder.schedule.into(),
                    reminder.id.user,
                    reminder.message,
                );
                (def, reminder.id.id)
            })
        })
    }
}
