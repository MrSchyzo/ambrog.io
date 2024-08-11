use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    RwLock,
};

use crate::memory::transient::{InMemoryStorage, ReminderDefinition};

/*
 * Needed:
 * 1. In-memory storage for reminders
 *  - retrieval of next reminder by time
 *  - insertion of reminders
 *  - deletion of reminders
 *  - direct access of reminders
 *  - get reminders by user
 * 2. Interruptible sleeping
 *  - interruption events: new reminder on top, reminder has to be run
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
    Add(ReminderDefinition),
    Stop,
}

struct Engine {
    storage: RwLock<InMemoryStorage>,
    time_provider: Arc<dyn TimeProvider>,
    receiver: Receiver<EngineMessage>,
    sender: Sender<EngineMessage>,
}

impl Engine {
    pub fn new(storage: InMemoryStorage, time_provider: Arc<dyn TimeProvider>) -> Self {
        let (sender, receiver) = channel::<EngineMessage>(16_384);
        Self {
            storage: RwLock::new(storage),
            time_provider,
            receiver,
            sender,
        }
    }

    pub async fn run(&mut self) {
        loop {
            let sleep = self
                .storage
                .read()
                .await
                .peek()
                .and_then(|reminder| reminder.current_tick().cloned());
        }
    }
}
