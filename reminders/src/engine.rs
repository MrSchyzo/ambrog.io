use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    Mutex,
};

use crate::memory::transient::{InMemoryStorage, ReminderDefinition};

/*
 * Needed:
 * 3. Fire-and-forget + recompute next reminder if next iteration exists
 * 4. Write-through mechanism
 * 5. On bootup, load the whole state
 *
 * Ideas:
 * - heap for next reminder + HashMap for access + deletion flag
 * - SQLite as backing engine
 * - expose callback in fire-and-forget mode
 */

pub trait TimeProvider {
    fn now(&self) -> DateTime<Utc>;
}

struct ChronoTimeProvider {}

impl TimeProvider for ChronoTimeProvider {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub enum EngineMessage {
    WakeUp,
    Stop,
}

#[async_trait]
pub trait ReminderCallback {
    async fn call(&self, user: u64, reminder_id: i32, message: Arc<String>);
}

pub struct Engine {
    storage: Mutex<InMemoryStorage>,
    time_provider: Arc<dyn TimeProvider>,
    receiver: Mutex<Receiver<EngineMessage>>,
    sender: Sender<EngineMessage>,
    callback: Arc<dyn ReminderCallback + Send + Sync>,
}

impl Engine {
    pub fn new(
        storage: InMemoryStorage,
        time_provider: Arc<dyn TimeProvider>,
        callback: Arc<dyn ReminderCallback + Send + Sync>,
    ) -> Self {
        let (sender, receiver) = channel::<EngineMessage>(128);
        Self {
            storage: Mutex::new(storage),
            time_provider,
            receiver: Mutex::new(receiver),
            sender,
            callback,
        }
    }

    pub async fn stop(&self) -> bool {
        self.sender.try_send(EngineMessage::Stop).is_ok()
    }

    pub async fn add(&self, def: ReminderDefinition) -> Option<i32> {
        let id = self
            .storage
            .lock()
            .await
            .insert(def, &self.time_provider.now());
        let _ = self.sender.try_send(EngineMessage::WakeUp);
        id
    }

    pub async fn defuse(&self, user: u64, id: i32) {
        self.storage.lock().await.defuse(&user, &id);
    }

    pub async fn run(&self) {
        loop {
            let reminder = match self.storage.lock().await.dequeue_next() {
                None => match self.receiver.lock().await.recv().await {
                    None | Some(EngineMessage::Stop) => return,
                    Some(EngineMessage::WakeUp) => continue,
                },
                Some(reminder) => reminder,
            };

            if let Some(date) = reminder.current_tick().copied() {
                let now = self.time_provider.now();
                let time_to_wait = now.signed_duration_since(date).num_milliseconds().max(0) as u64;
                let mut channel = self.receiver.lock().await;
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(time_to_wait)) => {
                        tokio::spawn({
                            let message = reminder.message().clone();
                            let (user, id) = reminder.reminder_id();
                            let callback = self.callback.clone();
                            async move {
                                callback.call(user, id, message).await;
                            }
                        });
                    }
                    message = channel.recv() => {
                        if let None | Some(EngineMessage::Stop) = message {return};
                    }
                };
            }
            self.storage
                .lock()
                .await
                .advance(reminder, &self.time_provider.now());
        }
    }
}
