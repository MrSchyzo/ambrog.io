use std::{
    num::{NonZeroU8, NonZeroUsize},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Europe;
use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    Mutex,
};

use crate::{bitmap::Bitmap, memory::transient::InMemoryStorage, schedule::ScheduleGrid};

/*
 * Needed:
 * 4. Write-through mechanism
 * 5. On bootup, load the whole state
 *
 * Ideas:
 * - SQLite as backing engine
 */

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

pub struct Engine {
    storage: Mutex<InMemoryStorage>,
    time_provider: Arc<dyn TimeProvider + Send + Sync>,
    receiver: Mutex<Receiver<EngineMessage>>,
    sender: Sender<EngineMessage>,
    callback: Arc<dyn ReminderCallback + Send + Sync>,
}

impl Engine {
    pub fn new(
        time_provider: Arc<dyn TimeProvider + Send + Sync>,
        callback: Arc<dyn ReminderCallback + Send + Sync>,
    ) -> Self {
        let (sender, receiver) = channel::<EngineMessage>(128);
        Self {
            storage: Mutex::new(InMemoryStorage::new()),
            time_provider,
            receiver: Mutex::new(receiver),
            sender,
            callback,
        }
    }

    pub fn stop(&self) -> bool {
        self.sender.try_send(EngineMessage::Stop).is_ok()
    }

    pub async fn add(&self, def: ReminderDefinition) -> Option<i32> {
        tracing::info!("Adding definition for user {}", def.user_id);
        let _ = self.sender.try_send(EngineMessage::WakeUp);
        let id = self
            .storage
            .lock()
            .await
            .insert(def, &self.time_provider.now());
        tracing::info!("Added definition id: {:?}", id);
        let _ = self.sender.try_send(EngineMessage::WakeUp);
        id
    }

    pub async fn defuse(&self, user: u64, id: i32) {
        tracing::info!("Defusing reminder ({}, {})", user, id);
        self.storage.lock().await.defuse(&user, &id);
        tracing::info!("Defused reminder ({}, {})", user, id);
    }

    pub async fn run(&self) {
        loop {
            tracing::info!("Storage fetching");
            let reminder = match { self.storage.lock().await.dequeue_next() } {
                None => match {
                    tracing::info!("Waiting for reception");
                    let x = self.receiver.lock().await.recv().await;
                    tracing::info!("Received something!");
                    x
                } {
                    None | Some(EngineMessage::Stop) => return,
                    Some(EngineMessage::WakeUp) => continue,
                },
                Some(reminder) => reminder,
            };

            if let Some(date) = reminder.current_tick().copied() {
                let now = self.time_provider.now();
                let time_to_wait = date.signed_duration_since(now).num_milliseconds().max(0) as u64;
                let mut channel = self.receiver.lock().await;
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
                    message = channel.recv() => {
                        if let None | Some(EngineMessage::Stop) = message {return};
                    }
                };
            }

            tracing::info!("Reinserting reminder {:#?}", reminder.reminder_id());
            self.storage
                .lock()
                .await
                .advance(reminder, &self.time_provider.now());
        }
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
}

pub enum Schedule {
    Once {
        when: DateTime<Utc>,
    },
    Recurrent {
        since: DateTime<Utc>,
        schedule: Box<dyn UtcDateScheduler + Send + Sync>,
    },
    RecurrentUntil {
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        schedule: Box<dyn UtcDateScheduler + Send + Sync>,
    },
}

impl Schedule {
    pub fn every_fucking_minute_of_your_damn_life() -> Self {
        let builder = ScheduleGridBuilder::new(Europe::Rome);
        Self::Recurrent {
            since: Utc.from_utc_datetime(&NaiveDateTime::from_timestamp_millis(0).unwrap()),
            schedule: Box::new(builder.build()),
        }
    }

    pub fn next_tick(&self, now: &DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            Self::Once { when } if now < when => Some(*when),
            Self::Recurrent { since, schedule } => {
                if now < since {
                    Some(*since)
                } else {
                    schedule.next_scheduled_at(now)
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
                    match schedule.next_scheduled_at(now) {
                        x @ Some(d) if d < *until => x,
                        _ => None,
                    }
                }
            }
            _ => None,
        }
    }
}

pub struct ScheduleGridBuilder<Tz: TimeZone> {
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

impl<Tz: TimeZone> ScheduleGridBuilder<Tz> {
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

    pub fn build(self) -> ScheduleGrid<Tz> {
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

pub trait UtcDateScheduler {
    fn next_scheduled_at(&self, now: &DateTime<Utc>) -> Option<DateTime<Utc>>;
}

impl<Tz: TimeZone> UtcDateScheduler for ScheduleGrid<Tz> {
    fn next_scheduled_at(&self, now: &DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.next_scheduled_after(now)
    }
}
