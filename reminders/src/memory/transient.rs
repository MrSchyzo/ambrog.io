use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
    sync::{Mutex, MutexGuard},
};

use chrono::DateTime;
use chrono::Utc;
use rand::{rngs::SmallRng, RngCore, SeedableRng};

use crate::interface::{Reminder, ReminderDefinition};

struct ReminderState {
    id: i32,
    definition: ReminderDefinition,
    current_tick: Option<DateTime<Utc>>,
    defused: bool,
}

impl ReminderState {
    pub fn fast_forward_after(&mut self, then: &DateTime<Utc>) -> Option<&DateTime<Utc>> {
        let next = self
            .current_tick
            .and_then(|_| self.definition.next_tick(then));
        self.current_tick = next;
        self.current_tick()
    }

    pub fn defuse(&mut self) {
        self.defused = true;
    }

    pub fn current_tick(&self) -> Option<&DateTime<Utc>> {
        if self.defused {
            None
        } else {
            self.current_tick.as_ref()
        }
    }

    pub fn reminder_id(&self) -> (u64, i32) {
        (self.definition.user_id(), self.id)
    }
}

#[derive(PartialEq, Eq)]
struct ReminderHeapref {
    user_id: u64,
    id: i32,
    next_tick: DateTime<Utc>,
}

impl PartialOrd for ReminderHeapref {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ReminderHeapref {
    fn cmp(&self, other: &Self) -> Ordering {
        other.next_tick.cmp(&self.next_tick)
    }
}

pub struct InMemoryStorage {
    queue: BinaryHeap<ReminderHeapref>,
    user_reminder_lookup: HashMap<u64, HashMap<i32, Mutex<ReminderState>>>,
    rand: rand::rngs::SmallRng,
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
            user_reminder_lookup: HashMap::new(),
            rand: SmallRng::from_entropy(),
        }
    }

    pub fn insert(&mut self, definition: ReminderDefinition, now: &DateTime<Utc>) -> Option<i32> {
        tracing::info!("Insert for user {:#?}", definition.user_id());
        definition
            .next_tick(now)
            .map(|d| self.internal_insert_new(definition, d))
    }

    pub fn dequeue_next(&mut self) -> Option<Reminder> {
        tracing::info!("DEQUEUE");
        let x = self
            .queue
            .pop()
            .and_then(|reminder| self.get_reminder(&reminder.user_id, &reminder.id))
            .map(|rem| {
                let (user_id, id) = rem.reminder_id();
                tracing::info!("Acquired lock {:#?}", rem.reminder_id());
                Reminder::new(
                    user_id,
                    id,
                    rem.current_tick().cloned(),
                    rem.definition.message(),
                )
            });
        tracing::info!("DEQUEUED");
        x
    }

    pub fn advance(&mut self, reminder: Reminder, then: &DateTime<Utc>) {
        let (user_id, id) = reminder.reminder_id();
        self.get_reminder(&user_id, &id)
            .and_then(|mut state| state.fast_forward_after(then).cloned())
            .map(|next_tick| {
                self.queue.push(ReminderHeapref {
                    user_id,
                    id,
                    next_tick,
                });
            })
            .unwrap_or_else(|| {
                self.remove_reminder(&user_id, &id);
            });
    }

    pub fn defuse(&self, user_id: &u64, reminder_id: &i32) {
        if let Some(mut state) = self.get_reminder(user_id, reminder_id) {
            state.defuse()
        }
    }

    pub fn get(&self, user_id: &u64, reminder_id: &i32) -> Option<Reminder> {
        self.get_reminder(user_id, reminder_id)
            .map(Self::into_reminder)
    }

    pub fn get_all(&self, user_id: &u64) -> HashMap<i32, Reminder> {
        self.user_reminder_lookup
            .get(user_id)
            .map(|lookup| {
                lookup
                    .values()
                    .filter_map(|lock| lock.lock().ok())
                    .map(|reminder| (reminder.id, Self::into_reminder(reminder)))
                    .collect::<HashMap<i32, Reminder>>()
            })
            .unwrap_or_default()
    }

    fn internal_insert_new(&mut self, definition: ReminderDefinition, now: DateTime<Utc>) -> i32 {
        tracing::info!("Internal insertion");
        let map = self
            .user_reminder_lookup
            .entry(definition.user_id())
            .or_default();

        let mut id = self.rand.next_u32() as i32;
        while map.contains_key(&id) {
            id = self.rand.next_u32() as i32;
        }

        let heap_ref: ReminderHeapref = ReminderHeapref {
            user_id: definition.user_id(),
            id,
            next_tick: now,
        };
        let state = ReminderState {
            id,
            definition,
            current_tick: Some(now),
            defused: false,
        };

        map.insert(id, Mutex::new(state));
        self.queue.push(heap_ref);

        id
    }

    fn get_reminder(&self, user_id: &u64, id: &i32) -> Option<MutexGuard<ReminderState>> {
        tracing::info!("Get reminder ({}, {})", user_id, id);
        self.user_reminder_lookup
            .get(user_id)
            .and_then(|reminders| reminders.get(id))
            .and_then(|lock| lock.lock().ok())
    }

    fn remove_reminder(&mut self, user_id: &u64, id: &i32) -> Option<Mutex<ReminderState>> {
        tracing::info!("Remove reminder ({}, {})", user_id, id);
        self.user_reminder_lookup
            .get_mut(user_id)
            .and_then(|reminders| reminders.remove(id))
    }

    fn into_reminder(reminder: MutexGuard<ReminderState>) -> Reminder {
        Reminder::new(
            reminder.definition.user_id(),
            reminder.id,
            reminder.current_tick().cloned(),
            reminder.definition.message(),
        )
    }
}
