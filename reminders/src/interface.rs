use std::{
    num::{NonZeroU8, NonZeroUsize},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

pub use crate::text::parsing::*;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::{Europe, Tz};
use futures::{pin_mut, StreamExt};
use mongodb::Client;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::Mutex as AsyncMutex;

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
        let (sender, receiver) = channel::<EngineMessage>(128);
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
        let records = ret.permanent_storage.get_all().await;
        pin_mut!(records);
        let now = time_provider.now();
        while let Some(record) = records.next().await {
            record
                .ok()
                .map(|(definition, id)| ret.obtain_storage().insert(definition, &now, Some(id)));
        }
        tracing::info!(
            "State is initialised with {} record(s)",
            ret.obtain_storage().size()
        );

        ret
    }

    pub fn stop(&self) -> bool {
        self.sender.try_send(EngineMessage::Stop).is_ok()
    }

    pub async fn add(&self, def: ReminderDefinition) -> Option<i32> {
        tracing::info!("Adding definition for user {}", def.user_id);
        let id = self
            .obtain_storage()
            .insert(def.clone(), &self.time_provider.now(), None);
        tracing::info!("Added definition id: {:?}", id);
        let _ = self.sender.try_send(EngineMessage::WakeUp);
        tracing::info!("Inserting to mongo with id: {:?}", id);
        self.permanent_storage.create(&def, id?).await;
        tracing::info!("Inserted to mongo with id: {:?}", id);
        id
    }

    pub async fn defuse(&self, user: u64, id: i32) {
        self.permanent_storage.delete(user, id).await;
        tracing::info!("Defusing reminder ({}, {})", user, id);
        self.internal_defuse(&user, &id);
        tracing::info!("Defused reminder ({}, {})", user, id);
    }

    pub async fn run(&self) {
        loop {
            tracing::info!("Storage fetching");
            let reminder = match self.dequeue_next() {
                None => match self.listen().await {
                    None | Some(EngineMessage::Stop) => return,
                    Some(EngineMessage::WakeUp) => continue,
                },
                Some(reminder) => reminder,
            };

            if let Some(date) = reminder.current_tick().copied() {
                let now = self.time_provider.now();
                let time_to_wait = date.signed_duration_since(now).num_milliseconds().max(0) as u64;
                tracing::info!(
                    "Waiting for reminder execution {:#?}, sleeping {:?}",
                    reminder.reminder_id(),
                    time_to_wait
                );
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(time_to_wait)) => {
                        tracing::info!("Executing reminder {:#?}", reminder.reminder_id());
                        tokio::spawn({
                            let message = reminder.message().clone();
                            let (user, id) = reminder.reminder_id();
                            let callback = self.callback.clone();
                            async move {
                                tracing::info!("Executing reminder callback ({}, {})", user, id);
                                callback.call(user, id, message).await;
                                tracing::info!("Executed reminder callback ({}, {})", user, id);
                            }
                        });
                    }
                    message = self.listen() => {
                        if let None | Some(EngineMessage::Stop) = message {return};
                    }
                };
            }

            tracing::info!("Reinserting reminder {:#?}", reminder.reminder_id());
            self.advance(reminder);
        }
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

#[derive(Clone)]
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
    pub fn every_fucking_minute_of_your_damn_life(since: Option<DateTime<Utc>>) -> Self {
        let builder = ScheduleGridBuilder::new(Europe::Rome);
        Self::Recurrent {
            since: since.unwrap_or_else(|| {
                Utc.from_utc_datetime(&NaiveDateTime::from_timestamp_millis(0).unwrap())
            }),
            schedule: builder.build(),
        }
    }

    pub fn next_tick(&self, now: &DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            Self::Once { when } if now < when => Some(*when),
            Self::Recurrent { since, schedule } => {
                if now < since {
                    Some(*since)
                } else {
                    schedule.next_scheduled_after(now)
                }
            }
            Self::RecurrentUntil {
                since,
                until,
                schedule,
            } => {
                if now < since {
                    Some(*since)
                } else {
                    match schedule.next_scheduled_after(now) {
                        x @ Some(d) if d < *until => x,
                        _ => None,
                    }
                }
            }
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
}

pub enum ScheduleInspection {
    Minute,
    Hour,
    WeekOfMonth,
    DayOfMonth,
    DaysOfWeek,
    MonthsOfYear,
}
