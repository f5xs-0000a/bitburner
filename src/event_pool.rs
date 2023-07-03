use core::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::netscript::{
    Date,
    NsWrapper,
};

pub trait Event {
    fn trigger_time(&self) -> f64;
}

pub struct EventLoopContext<E>
where
    E: Event,
{
    next_events: Vec<E>,
    grace_period: f64,
}

impl<E> EventLoopContext<E>
where
    E: Event,
{
    pub fn add_event(
        &mut self,
        event: E,
    ) {
        self.next_events.push(event);
    }

    pub fn get_grace_period(&self) -> f64 {
        self.grace_period
    }

    fn drain_to_event_pool(
        &mut self,
        extensible: &mut impl Extend<EventWrapper<E>>,
    ) {
        let drain_src = self.next_events.drain(..).map(|e| EventWrapper(e));
        extensible.extend(drain_src)
    }
}

pub trait EventLoopState {
    type Event: Event;

    fn initial_run<'a>(
        &mut self,
        ns: &NsWrapper<'a>,
        ctx: &mut EventLoopContext<Self::Event>,
    );

    fn on_event<'a>(
        &mut self,
        ns: &NsWrapper<'a>,
        event: Self::Event,
        ctx: &mut EventLoopContext<Self::Event>,
    );

    fn on_event_fail<'a>(
        &mut self,
        ns: &NsWrapper<'a>,
        event: Self::Event,
        ctx: &mut EventLoopContext<Self::Event>,
    );

    fn post_loop_inspect<'a>(&self, ns: &NsWrapper<'a>, event_heap: &BinaryHeap<EventWrapper<Self::Event>>);
}

#[derive(Debug)]
pub struct EventWrapper<E>(pub E)
where
    E: Event;

impl<E> PartialOrd for EventWrapper<E>
where
    E: Event,
{
    fn partial_cmp(
        &self,
        other: &Self,
    ) -> Option<Ordering> {
        // since the std BinaryHeap is a max-heap which will remove the highest
        // element first, we're reversing the comparison operator for this so
        // the lowest element gets popped first
        self.0
            .trigger_time()
            .partial_cmp(&other.0.trigger_time())
            .map(|o| o.reverse())
    }
}

impl<E> Ord for EventWrapper<E>
where
    E: Event,
{
    fn cmp(
        &self,
        other: &Self,
    ) -> Ordering {
        self.partial_cmp(&other).unwrap()
    }
}

impl<E> PartialEq for EventWrapper<E>
where
    E: Event,
{
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        self.0.trigger_time() == other.0.trigger_time()
    }
}

impl<E> Eq for EventWrapper<E> where E: Event {}

impl<E> From<E> for EventWrapper<E>
where
    E: Event,
{
    fn from(e: E) -> EventWrapper<E> {
        EventWrapper(e)
    }
}

#[derive(Debug)]
pub struct EventLoop<E>
where
    E: EventLoopState,
{
    event_pool: BinaryHeap<EventWrapper<E::Event>>,
    state: E,
    grace_period: f64,
}

impl<E> EventLoop<E>
where
    E: EventLoopState,
{
    pub fn new(state: E) -> EventLoop<E> {
        EventLoop {
            event_pool: BinaryHeap::new(),
            state,
            grace_period: 50., // milliseconds
        }
    }

    pub async fn run(
        &mut self,
        ns: &NsWrapper<'_>,
    ) {
        let mut context = EventLoopContext {
            next_events: Vec::with_capacity(16),
            grace_period: self.grace_period,
        };

        // populate the pool first
        self.state.initial_run(ns, &mut context);
        context.drain_to_event_pool(&mut self.event_pool);

        let mut last_slept_until = Date::now();
        while let Some(event) = self.event_pool.pop() {
            let event = event.0;

            // TODO: rework this.

            // if the trigger time is greater than now, sleep then execute
            if last_slept_until <= event.trigger_time() {
                ns.sleep((event.trigger_time() - Date::now()).round() as i32)
                    .await;
                last_slept_until = event.trigger_time();

                self.state.on_event(ns, event, &mut context);
            }
            // if the trigger time is between now and grace period, execute
            else if last_slept_until - self.grace_period
                <= event.trigger_time()
            {
                self.state.on_event(ns, event, &mut context);
            }
            // if the trigger time is beyond grace period, it's too late. fail.
            else {
                self.state.on_event_fail(ns, event, &mut context);
            }

            context.drain_to_event_pool(&mut self.event_pool);

            self.state.post_loop_inspect(ns, &self.event_pool);
        }
    }
}
