use std::{
    collections::HashMap,
    num::{NonZeroU8, NonZeroUsize},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

pub use crate::text::parsing::*;
use async_trait::async_trait;
use chrono::{DateTime, Month, NaiveTime, Timelike, Utc, Weekday};
use chrono_tz::Tz;
use futures::{pin_mut, StreamExt};
use mongodb::Client;
use tokio::sync::Mutex as AsyncMutex;
use tokio::{
    sync::mpsc::{channel, Receiver, Sender},
    time::Instant,
};

use crate::{
    bitmap::Bitmap,
    memory::{persistent::MongoloidStorage, transient::InMemoryStorage},
    schedule::ScheduleGrid,
};

pub trait TimeProvider {
    fn now(&self) -> DateTime<Utc>;
}

pub struct ChronoTimeProvider {}

impl TimeProvider for ChronoTimeProvider {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

enum EngineMessage {
    WakeUp,
    Stop,
}

#[async_trait]
pub trait ReminderCallback {
    async fn call(&self, user: u64, reminder_id: i32, message: Arc<String>);
}

pub struct ReminderEngine {
    storage: Arc<Mutex<InMemoryStorage>>,
    time_provider: Arc<dyn TimeProvider + Send + Sync>,
    receiver: AsyncMutex<Receiver<EngineMessage>>,
    sender: Sender<EngineMessage>,
    callback: Arc<dyn ReminderCallback + Send + Sync>,
    permanent_storage: MongoloidStorage,
}

impl ReminderEngine {
    pub async fn new_and_init(
        time_provider: Arc<dyn TimeProvider + Send + Sync>,
        callback: Arc<dyn ReminderCallback + Send + Sync>,
        mongo_url: &str,
        mongo_db: &str,
    ) -> Self {
        let (sender, receiver) = channel::<EngineMessage>(512);
        let db = Client::with_uri_str(mongo_url)
            .await
            .unwrap()
            .database(mongo_db);
        let ret = Self {
            storage: Arc::new(Mutex::new(InMemoryStorage::new())),
            time_provider: time_provider.clone(),
            receiver: AsyncMutex::new(receiver),
            sender,
            callback,
            permanent_storage: MongoloidStorage::new(db),
        };

        tracing::info!("Initialising state");
        let start = Instant::now();
        let records = ret.permanent_storage.get_all().await;
        pin_mut!(records);
        let now = time_provider.now();
        while let Some(record) = records.next().await {
            record
                .ok()
                .map(|(definition, id)| ret.obtain_storage().insert(definition, &now, Some(id)));
        }
        let size = ret.obtain_storage().size();
        let elapsed = start.elapsed().as_micros();
        tracing::info!(
            elapsed_micros = elapsed,
            record_count = size,
            "State is initialised"
        );

        ret
    }

    pub fn stop(&self) -> bool {
        self.sender.try_send(EngineMessage::Stop).is_ok()
    }

    pub async fn add(&self, def: ReminderDefinition) -> Option<i32> {
        let id = self
            .obtain_storage()
            .insert(def.clone(), &self.time_provider.now(), None)?;
        let _ = self.sender.try_send(EngineMessage::WakeUp);
        if !self.permanent_storage.create(&def, id).await {
            self.internal_defuse(&def.user_id, &id);
            None
        } else {
            Some(id)
        }
    }

    pub async fn defuse(&self, user: u64, id: i32) -> bool {
        if self.permanent_storage.delete(user, id).await {
            self.internal_defuse(&user, &id);
            true
        } else {
            false
        }
    }

    pub async fn run(&self) {
        loop {
            let reminder = match self.dequeue_next() {
                None => match self.listen().await {
                    None | Some(EngineMessage::Stop) => return,
                    Some(EngineMessage::WakeUp) => continue,
                },
                Some(reminder) => reminder,
            };
            let (user_id, reminder_id) = reminder.reminder_id();

            if let Some(date) = reminder.current_tick().copied() {
                let now = self.time_provider.now();
                let time_to_wait = date.signed_duration_since(now).num_milliseconds().max(0) as u64;
                tracing::info!(
                    user_id = user_id,
                    reminder_id = reminder_id,
                    sleep_ms = time_to_wait,
                    target_date = date.to_rfc3339(),
                    "Sleeping until next reminder",
                );
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(time_to_wait)) => {
                        tokio::spawn({
                            let message = reminder.message().clone();
                            let callback = self.callback.clone();
                            async move {
                                let start = Instant::now();
                                callback.call(user_id, reminder_id, message).await;
                                tracing::info!(
                                    elapsed_micros = start.elapsed().as_micros(),
                                    user_id = user_id,
                                    reminder_id = reminder_id,
                                    "Executed reminder callback"
                                );
                            }
                        });
                    }
                    message = self.listen() => {
                        if let None | Some(EngineMessage::Stop) = message {return};
                    }
                };
            }

            tracing::info!(
                user_id = user_id,
                reminder_id = reminder_id,
                "Reinserting reminder"
            );
            self.advance(reminder);
        }
    }

    pub fn get(&self, user_id: &u64, reminder_id: &i32) -> Option<Reminder> {
        self.obtain_storage().get(user_id, reminder_id)
    }

    pub fn get_all(&self, user_id: &u64) -> HashMap<i32, Reminder> {
        self.obtain_storage().get_all(user_id)
    }

    fn dequeue_next(&self) -> Option<Reminder> {
        self.obtain_storage().dequeue_next()
    }

    fn internal_defuse(&self, user_id: &u64, id: &i32) {
        self.obtain_storage().defuse(user_id, id);
    }

    fn advance(&self, reminder: Reminder) {
        self.obtain_storage()
            .advance(reminder, &self.time_provider.now());
    }

    async fn listen(&self) -> Option<EngineMessage> {
        self.receiver.lock().await.recv().await
    }

    fn obtain_storage(&self) -> MutexGuard<InMemoryStorage> {
        self.storage.lock().unwrap()
    }
}

pub struct Reminder {
    user_id: u64,
    id: i32,
    current_tick: Option<DateTime<Utc>>,
    message: Arc<String>,
}

impl Reminder {
    pub fn new(
        user_id: u64,
        id: i32,
        current_tick: Option<DateTime<Utc>>,
        message: Arc<String>,
    ) -> Self {
        Self {
            user_id,
            id,
            current_tick,
            message,
        }
    }

    pub fn reminder_id(&self) -> (u64, i32) {
        (self.user_id, self.id)
    }

    pub fn current_tick(&self) -> Option<&DateTime<Utc>> {
        self.current_tick.as_ref()
    }

    pub fn message(&self) -> Arc<String> {
        self.message.clone()
    }
}

#[derive(Clone)]
pub struct ReminderDefinition {
    schedule: Schedule,
    user_id: u64,
    message: Arc<String>,
}

impl ReminderDefinition {
    pub fn new(schedule: Schedule, user_id: u64, message: String) -> Self {
        Self {
            schedule,
            user_id,
            message: Arc::new(message),
        }
    }

    pub fn next_tick(&self, now: &DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.schedule.next_tick(now)
    }

    pub fn user_id(&self) -> u64 {
        self.user_id
    }

    pub fn message(&self) -> Arc<String> {
        self.message.clone()
    }

    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }
}

#[cfg_attr(test, derive(Eq, PartialEq))]
#[derive(Clone, Debug)]
pub enum Schedule {
    Once {
        when: DateTime<Utc>,
    },
    Recurrent {
        since: DateTime<Utc>,
        schedule: ScheduleGrid,
    },
    RecurrentUntil {
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        schedule: ScheduleGrid,
    },
}

impl Schedule {
    pub fn next_tick(&self, now: &DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            Self::Once { when } if now < when => Some(*when),
            Self::Recurrent { since, schedule } => schedule.next_scheduled_after(now.max(since)),
            Self::RecurrentUntil {
                since,
                until,
                schedule,
            } => schedule
                .next_scheduled_after(now.max(since))
                .filter(|then| *then < *until),
            _ => None,
        }
    }
}

pub struct ScheduleGridBuilder {
    minutes: Bitmap,
    hours: Bitmap,
    weeks_of_month: Bitmap,
    days_of_month: Bitmap,
    days_of_week: Bitmap,
    months_of_year: Bitmap,
    year_cadence: NonZeroU8,
    year_start: u32,
    timezone: Tz,
}

impl ScheduleGridBuilder {
    pub fn new(timezone: Tz) -> Self {
        Self {
            minutes: Bitmap::all_set(NonZeroUsize::new(60).unwrap()),
            hours: Bitmap::all_set(NonZeroUsize::new(60).unwrap()),
            weeks_of_month: Bitmap::all_set(NonZeroUsize::new(5).unwrap()),
            days_of_month: Bitmap::all_set(NonZeroUsize::new(31).unwrap()),
            days_of_week: Bitmap::all_set(NonZeroUsize::new(7).unwrap()),
            months_of_year: Bitmap::all_set(NonZeroUsize::new(12).unwrap()),
            year_cadence: NonZeroU8::new(1u8).unwrap(),
            year_start: 1970u32,
            timezone,
        }
    }

    pub fn build(self) -> ScheduleGrid {
        ScheduleGrid::new(
            self.minutes,
            self.hours,
            self.weeks_of_month,
            self.days_of_month,
            self.days_of_week,
            self.months_of_year,
            self.year_cadence,
            self.year_start,
            self.timezone,
        )
    }

    pub fn with_times(&mut self, times: Vec<NaiveTime>) -> &mut Self {
        let (hours, minutes) = times
            .into_iter()
            .map(|time| (time.hour(), time.minute()))
            .unzip();
        self.with_hours(hours).with_minutes(minutes)
    }

    pub fn with_hours(&mut self, hours: Vec<u32>) -> &mut Self {
        self.hours.clear();
        for hour in hours {
            self.hours.set(hour as usize);
        }
        self
    }

    pub fn all_hours(&mut self) -> &mut Self {
        for i in 0..23 {
            self.hours.set(i as usize);
        }
        self
    }

    pub fn with_minutes(&mut self, minutes: Vec<u32>) -> &mut Self {
        self.minutes.clear();
        for minute in minutes {
            self.minutes.set(minute as usize);
        }
        self
    }

    pub fn all_minutes(&mut self) -> &mut Self {
        for i in 0..59 {
            self.minutes.set(i as usize);
        }
        self
    }

    pub fn with_weeks_of_month(&mut self, weeks: Vec<u32>) -> &mut Self {
        self.weeks_of_month.clear();
        for week in weeks {
            self.weeks_of_month.set(week as usize);
        }
        self
    }

    pub fn all_weeks(&mut self) -> &mut Self {
        for i in 0..4 {
            self.weeks_of_month.set(i as usize);
        }
        self
    }

    pub fn with_weekdays(&mut self, weekdays: Vec<Weekday>) -> &mut Self {
        self.days_of_week.clear();
        for day in weekdays {
            self.days_of_week.set(day.num_days_from_monday() as usize);
        }
        self
    }

    pub fn with_year(&mut self, year: u32) -> &mut Self {
        self.year_start = year;
        self
    }

    pub fn with_months(&mut self, months: Vec<Month>) -> &mut Self {
        self.months_of_year.clear();
        for month in months {
            self.months_of_year
                .set((month.number_from_month() - 1) as usize);
        }
        self
    }

    pub fn all_months(&mut self) -> &mut Self {
        for i in 0..11 {
            self.months_of_year.set(i as usize);
        }
        self
    }

    pub fn with_days_of_month(&mut self, days: Vec<u8>) -> &mut Self {
        self.days_of_month.clear();
        for day in days {
            self.days_of_month.set((day - 1) as usize);
        }
        self
    }

    pub fn all_days_of_month(&mut self) -> &mut Self {
        for i in 0..30 {
            self.days_of_month.set(i as usize);
        }
        self
    }

    pub fn with_year_cadence(&mut self, year: u8) -> &mut Self {
        self.year_cadence = NonZeroU8::new(year.max(1u8)).unwrap();
        self
    }
}

pub enum ScheduleInspection {
    Minute,
    Hour,
    WeekOfMonth,
    DayOfMonth,
    DaysOfWeek,
    MonthsOfYear,
}
