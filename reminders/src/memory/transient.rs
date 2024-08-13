use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
    ops::{Deref, DerefMut},
    sync::{Arc, Mutex, MutexGuard},
};

use chrono::Utc;
use chrono::DateTime;
use rand::{rngs::SmallRng, RngCore, SeedableRng};

use crate::schedule::UtcDateScheduler;

pub enum Schedule {
    Once {
        when: DateTime<Utc>,
    },
    Recurrent {
        since: DateTime<Utc>,
        schedule: Box<dyn UtcDateScheduler>,
    },
    RecurrentUntil {
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        schedule: Box<dyn UtcDateScheduler>,
    },
}

impl Schedule {
    pub fn next_tick(&self, now: &DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            Self::Once { when } if now < when => Some(when.with_timezone(&Utc)),
            Self::Recurrent { since, schedule } => {
                if now < since {
                    Some(since.with_timezone(&Utc))
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
                    Some(since.with_timezone(&Utc))
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

pub struct ReminderDefinition {
    schedule: Schedule,
    user_id: u64,
    message: Arc<String>,
}

impl ReminderDefinition {
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

pub struct ReminderState {
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

    pub fn definition(&self) -> &ReminderDefinition {
        &self.definition
    }

    pub fn next_tick(&self, after: Option<&DateTime<Utc>>) -> Option<DateTime<Utc>> {
        after
            .or(self.current_tick())
            .and_then(|d| self.definition.next_tick(d))
    }

    pub fn current_tick(&self) -> Option<&DateTime<Utc>> {
        if self.defused {
            None
        } else {
            self.current_tick.as_ref()
        }
    }

    pub fn reminder_id(&self) -> (u64, i32) {
        (self.definition.user_id, self.id)
    }

    pub fn is_active(&self) -> bool {
        self.current_tick.is_some() && self.defused
    }
}

#[derive(Clone)]
pub struct MinHeapWrapper<T: Ord> {
    contained: T,
}

impl<T: Ord> Deref for MinHeapWrapper<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.contained
    }
}

impl<T: Ord> DerefMut for MinHeapWrapper<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.contained
    }
}

impl<T: Ord> PartialEq for MinHeapWrapper<T> {
    fn eq(&self, other: &Self) -> bool {
        self.deref().eq(other.deref())
    }
}

impl<T: Ord> Eq for MinHeapWrapper<T> {}

impl<T: Ord> Ord for MinHeapWrapper<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        other.deref().cmp(self.deref())
    }
}

impl<T: Ord> PartialOrd for MinHeapWrapper<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub struct Reminder {
    user_id: u64,
    id: i32,
    current_tick: Option<DateTime<Utc>>,
    message: Arc<String>,
}

impl Reminder {
    pub fn reminder_id(&self) -> (u64, i32) {
        (self.user_id, self.id)
    }

    pub fn next_tick(&self) -> Option<&DateTime<Utc>> {
        self.current_tick.as_ref()
    }

    pub fn message(&self) -> Arc<String> {
        self.message.clone()
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

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
            user_reminder_lookup: HashMap::new(),
            rand: SmallRng::from_entropy(),
        }
    }

    pub fn insert(&mut self, definition: ReminderDefinition, now: &DateTime<Utc>) -> Option<i32> {
        definition
            .next_tick(now)
            .map(|d| self.internal_insert_new(definition, d))
    }

    pub fn dequeue_next(&mut self) -> Option<Reminder> {
        self.queue
            .pop()
            .and_then(|reminder| self.get_reminder(&reminder.user_id, &reminder.id))
            .map(|rem| {
                let (user_id, id) = rem.reminder_id();
                Reminder {
                    user_id,
                    id,
                    current_tick: rem.current_tick().cloned(),
                    message: rem.definition.message(),
                }
            })
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
        self.get_reminder(user_id, reminder_id)
            .map(|mut state| state.defuse());
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
        let map = self
            .user_reminder_lookup
            .entry(definition.user_id)
            .or_default();

        let mut id = self.rand.next_u32() as i32;
        while map.contains_key(&id) {
            id = self.rand.next_u32() as i32;
        }

        let heap_ref = ReminderHeapref {
            user_id: definition.user_id,
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
        self.user_reminder_lookup
            .get(user_id)
            .and_then(|reminders| reminders.get(id))
            .and_then(|lock| lock.lock().ok())
    }

    fn remove_reminder(&mut self, user_id: &u64, id: &i32) -> Option<Mutex<ReminderState>> {
        self.user_reminder_lookup
            .get_mut(user_id)
            .and_then(|reminders| reminders.remove(id))
    }

    fn into_reminder(reminder: MutexGuard<ReminderState>) -> Reminder {
        Reminder{
            id: reminder.id,
            user_id: reminder.definition.user_id(),
            message: reminder.definition().message(),
            current_tick: reminder.current_tick().cloned(),
        }
    }
}


