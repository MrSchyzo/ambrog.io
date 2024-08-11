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

use std::{
    cell::{Ref, RefCell},
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
    rc::Rc,
};

use chrono::DateTime;
use chrono_tz::Tz;
use rand::RngCore;

use crate::schedule::ScheduleGrid;

#[derive(PartialEq, Eq)]
pub enum Schedule {
    Once {
        when: DateTime<Tz>,
    },
    Recurrent {
        since: DateTime<Tz>,
        schedule: ScheduleGrid,
    },
    RecurrentUntil {
        since: DateTime<Tz>,
        until: DateTime<Tz>,
        schedule: ScheduleGrid,
    },
}

impl Schedule {
    pub fn next_tick(&self, now: &DateTime<Tz>) -> Option<DateTime<Tz>> {
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

#[derive(PartialEq, Eq)]
pub struct ReminderDefinition {
    schedule: Schedule,
    user_id: u64,
    message: String,
}

impl ReminderDefinition {
    pub fn next_tick(&self, now: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        self.schedule.next_tick(now)
    }
}

#[derive(PartialEq, Eq)]
pub struct ReminderState {
    definition: ReminderDefinition,
    next_tick: Option<DateTime<Tz>>,
}

impl ReminderState {
    pub fn go_next(&mut self) -> Option<&DateTime<Tz>> {
        let next = self.next_tick.and_then(|d| self.definition.next_tick(&d));
        self.next_tick = next;
        self.next_tick.as_ref()
    }

    pub fn fast_forward_after<'a>(&'a mut self, then: &DateTime<Tz>) -> Option<&'a DateTime<Tz>> {
        let next = self.next_tick.and_then(|_| self.definition.next_tick(then));
        self.next_tick = next;
        self.next_tick.as_ref()
    }

    pub fn defuse(&mut self) {
        self.next_tick = None
    }

    pub fn definition(&self) -> &ReminderDefinition {
        &self.definition
    }
}

impl PartialOrd for ReminderState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ReminderState {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self.next_tick, other.next_tick) {
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(this), Some(other)) if this > other => Ordering::Greater,
            (Some(this), Some(other)) if this < other => Ordering::Less,
            _ => Ordering::Equal,
        }
    }
}

pub struct InMemoryStorage {
    queue: BinaryHeap<Rc<RefCell<ReminderState>>>,
    user_reminder_lookup: HashMap<u64, HashMap<i32, Rc<RefCell<ReminderState>>>>,
    rand: rand::rngs::SmallRng,
}

impl InMemoryStorage {
    pub fn add(&mut self, definition: ReminderDefinition, now: &DateTime<Tz>) -> Option<i32> {
        definition
            .next_tick(now)
            .map(|d| ReminderState {
                definition,
                next_tick: Some(d),
            })
            .map(|state| self.internal_add(state))
    }

    pub fn defuse(&mut self, user_id: &u64, reminder_id: &i32) {
        self.user_reminder_lookup
            .get(user_id)
            .and_then(|lookup| (*lookup).get(reminder_id).cloned())
            .inspect(|state| {
                (*state.clone()).borrow_mut().defuse();
            });
    }

    pub fn get(&self, user_id: &u64, reminder_id: &i32) -> Option<Ref<ReminderDefinition>> {
        self.user_reminder_lookup.get(user_id).and_then(|lookup| {
            lookup
                .get(reminder_id)
                .map(|rc| Ref::map((**rc).borrow(), |state| state.definition()))
        })
    }

    pub fn get_all(&self, user_id: &u64) -> HashMap<i32, Ref<ReminderDefinition>> {
        self.user_reminder_lookup
            .get(user_id)
            .map(|lookup| {
                lookup
                    .iter()
                    .map(|(id, rc)| (*id, Ref::map((**rc).borrow(), |state| state.definition())))
                    .collect::<HashMap<i32, Ref<ReminderDefinition>>>()
            })
            .unwrap_or_default()
    }

    fn internal_add(&mut self, state: ReminderState) -> i32 {
        let map = self
            .user_reminder_lookup
            .entry(state.definition.user_id)
            .or_default();

        let mut id = self.rand.next_u32() as i32;
        while map.contains_key(&id) {
            id = self.rand.next_u32() as i32;
        }

        let state_ref = Rc::new(RefCell::new(state));
        map.insert(id, Rc::clone(&state_ref));
        self.queue.push(state_ref);

        id
    }
}

struct Engine {}
