use std::{
    cell::{Ref, RefCell, RefMut},
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
    ops::{Deref, DerefMut},
    rc::Rc,
};

use chrono::DateTime;
use chrono::Utc;
use rand::{rngs::SmallRng, RngCore, SeedableRng};

use crate::schedule::ScheduleGrid;

#[derive(PartialEq, Eq)]
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
    pub fn next_tick(&self, now: &DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.schedule.next_tick(now)
    }

    pub fn user_id(&self) -> u64 {
        self.user_id
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(PartialEq, Eq)]
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

impl PartialOrd for ReminderState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ReminderState {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self.current_tick, other.current_tick) {
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(this), Some(other)) if this > other => Ordering::Greater,
            (Some(this), Some(other)) if this < other => Ordering::Less,
            _ => Ordering::Equal,
        }
    }
}

#[derive(Clone)]
pub struct MinHeapWrapper<T: Ord> {
    underlying_rc: Rc<RefCell<T>>,
}

impl<T: Ord> Deref for MinHeapWrapper<T> {
    type Target = Rc<RefCell<T>>;

    fn deref(&self) -> &Self::Target {
        &self.underlying_rc
    }
}

impl<T: Ord> DerefMut for MinHeapWrapper<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.underlying_rc
    }
}

impl<T: Ord> MinHeapWrapper<T> {
    pub fn new(state: T) -> Self {
        Self {
            underlying_rc: Rc::new(RefCell::new(state)),
        }
    }

    pub fn from_rc_refcell(rc: Rc<RefCell<T>>) -> Self {
        Self { underlying_rc: rc }
    }

    pub fn as_ref(&self) -> Ref<T> {
        (*self.underlying_rc).borrow()
    }

    pub fn as_ref_mut(&self) -> RefMut<T> {
        (*self.underlying_rc).borrow_mut()
    }
}

impl<T: Ord> PartialEq for MinHeapWrapper<T> {
    fn eq(&self, other: &Self) -> bool {
        *self.as_ref() == *other.as_ref()
    }
}

impl<T: Ord> Eq for MinHeapWrapper<T> {}

impl<T: Ord> Ord for MinHeapWrapper<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        other.as_ref().cmp(&self.as_ref())
    }
}

impl<T: Ord> PartialOrd for MinHeapWrapper<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// TODO: idea, get rid of Rc<RefCell>
/*
   How:
   1. lookup will own the actual reminder
   2. BinaryHeap will contain only the time + lookupId
   3. Heap2map deref will be emulated by entering the map
   4. Heap will always get the entire reminder by asking the map

   This way:
   - no rc-refcell
   - no runtime error for mut-borrow a shared ref across threads
   - this might also simplify heap and external API contact surface
*/
pub struct InMemoryStorage {
    queue: BinaryHeap<MinHeapWrapper<ReminderState>>,
    user_reminder_lookup: HashMap<u64, HashMap<i32, Rc<RefCell<ReminderState>>>>,
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

    pub fn pop_state(&mut self) -> Option<Rc<RefCell<ReminderState>>> {
        self.queue.pop().map(|x| Rc::clone(&x))
    }

    pub fn reinsert_reminder(
        &mut self,
        reminder: Rc<RefCell<ReminderState>>,
        then: &DateTime<Utc>,
    ) {
        reminder.borrow_mut().fast_forward_after(then);
        match reminder.borrow().is_active() {
            false => {
                let (user_id, id) = (*reminder).borrow().reminder_id();
                self.user_reminder_lookup
                    .get_mut(&user_id)
                    .map(|reminders| reminders.remove(&id));
            }
            true => self.internal_insert(Rc::clone(&reminder)),
        };
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

    fn internal_pop(&mut self) -> Option<Rc<RefCell<ReminderState>>> {
        self.queue.pop().map(|rc| {
            let (user_id, id) = (*rc).borrow().reminder_id();
            self.user_reminder_lookup
                .get_mut(&user_id)
                .map(|reminders| reminders.remove(&id));
            Rc::clone(&rc)
        })
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

        let state = ReminderState {
            id,
            definition,
            current_tick: Some(now),
            defused: false,
        };

        self.internal_insert(Rc::new(RefCell::new(state)));

        id
    }

    fn internal_insert(&mut self, state_ref: Rc<RefCell<ReminderState>>) {
        let (user_id, id) = (*Rc::clone(&state_ref)).borrow().reminder_id();
        self.user_reminder_lookup
            .entry(user_id)
            .or_default()
            .insert(id, Rc::clone(&state_ref));
        self.queue.push(MinHeapWrapper::from_rc_refcell(state_ref));
    }
}
