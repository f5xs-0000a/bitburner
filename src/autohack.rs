use std::{
    collections::{
        HashMap,
        BinaryHeap,
        VecDeque,
    },
    sync::{
        Arc,
        Mutex,
    },
};

use smallvec::SmallVec;

use crate::{
    event_pool::{
        Event,
        EventWrapper,
        EventLoop,
        EventLoopContext,
        EventLoopState,
    },
    machine::get_machines,
    netscript::Date,
    script_deploy::HGW,
    time_consts::{
        MILLISECOND,
        SECOND,
    },
    utils::{
        rational_mult_u64,
        rational_mult_usize,
    },
};
use crate::{
    machine::Machine,
    netscript::NsWrapper,
};

const RESERVATION_RATE: f64 = 0.9;

pub async fn auto_hack(ns: &NsWrapper<'_>) {
    let mut ahg = EventLoop::new(AutoHackGovernor::new(ns));
    ahg.run(ns).await;
    
    // might want to test this out.
    //let mut ahg = EventLoop::new(AutoHackGovernor::new(ns));

    //ns.tprint(&format!("{:#?}", &ahg));
}

#[derive(Debug)]
enum AutoHackEventType {
    PollTarget(Arc<str>),
    MemoryFreed(Arc<str>),
    GeneralPoll,
}

#[derive(Debug)]
struct AutoHackEventWrapped {
    trigger_time: f64,
    grace_period: f64,
    event_type: AutoHackEventType,
}

impl AutoHackEventWrapped {
    pub fn new_poll_target(
        trigger_time: f64,
        grace_period: f64,
        target: Arc<str>,
    ) -> AutoHackEventWrapped {
        AutoHackEventWrapped {
            trigger_time,
            grace_period,
            event_type: AutoHackEventType::PollTarget(target),
        }
    }

    pub fn new_memory_freed(
        trigger_time: f64,
        grace_period: f64,
        target: Arc<str>,
    ) -> AutoHackEventWrapped {
        AutoHackEventWrapped {
            trigger_time,
            grace_period,
            event_type: AutoHackEventType::MemoryFreed(target),
        }
    }

    pub fn new_general_poll(
        trigger_time: f64,
        grace_period: f64,
    ) -> AutoHackEventWrapped {
        AutoHackEventWrapped {
            trigger_time,
            grace_period,
            event_type: AutoHackEventType::GeneralPoll,
        }
    }
}

impl Event for AutoHackEventWrapped {
    fn trigger_time(&self) -> f64 {
        self.trigger_time
    }

    fn grace_period(&self) -> f64 {
        self.grace_period
    }
}

#[derive(Debug, Clone)]
enum TargetState {
    TotalWeaken(usize),
    MaxGrow,
    //Analysis,
    Hack,
}

#[derive(Debug)]
struct RunningProcessMetadata {
    pid: usize,
    threads: usize,
}

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
enum SplitType {
    /// Do not split the process at all
    NoSplit,
    /// Allow splitting the process and yield the full number of threads
    /// required
    FullSplit,
    /// Allow splitting the process and yield a partial number of threads
    /// compared to what's required
    PartialSplit,
}

fn find_available_hackers(
    ns: &NsWrapper<'_>,
    hackers: AHGHackerIterator,
    mut hgw_threads: usize,
    split: SplitType,
) -> Option<Vec<(Arc<Machine>, usize)>> {
    use SplitType::*;

    // TODO: make use of RESERVATION_RATE

    let mut available_hackers = vec![];

    // allow splitting the threads among machines
    if split == NoSplit {
        for hacker in hackers {
            let available_threads = hacker.get_threads_left(ns) as usize;

            // if we've found the machine that can support that many
            if hgw_threads <= available_threads {
                available_hackers.push((hacker.clone(), hgw_threads));
                break;
            }
        }

        // if we haven't found our hacker, fail
        if available_hackers.is_empty() {
            return None;
        }

        return Some(available_hackers);
    }

    for hacker in hackers {
        let available_threads = (hacker.get_threads_left(ns) as usize).min(hgw_threads);

        // if this machine does not have any available threads left,
        // skip this machine
        if available_threads == 0 {
            continue;
        }

        hgw_threads -= available_threads;

        available_hackers.push((hacker.clone(), available_threads));

        // if we've obtained all the machines that we could need to
        // weaken this, proceed
        if hgw_threads == 0 {
            break;
        }
    }

    // if we asked for a full split and we didn't get a full split
    if split == FullSplit && hgw_threads != 0 {
        return None;
    }

    Some(available_hackers)
}

#[derive(Debug)]
struct TargetStateBundle {
    machine_name: Arc<str>,
    machine: Arc<Machine>,

    state: TargetState,
    is_waiting_for_memory: bool,

    // latest spawned at the front
    // earliest spawned at the back
    // first element is the spawn time, not the finish time
    // TODO: we need a way to clear this
    running_pids: VecDeque<(f64, SmallVec<[RunningProcessMetadata; 4]>)>,
}

impl TargetStateBundle {
    fn new(
        ns: &NsWrapper<'_>,
        machine_name: Arc<str>,
        machine: Arc<Machine>,
    ) -> TargetStateBundle {
        let weakens_required = machine.get_weaken_threads_to_reduce(ns);

        TargetStateBundle {
            machine_name,
            machine,
            state: TargetState::TotalWeaken(weakens_required),
            is_waiting_for_memory: false,
            running_pids: Default::default(),
        }
    }

    fn get_earliest_allowable_weaken_spawn(
        &self,
        grace_period: f64,
    ) -> f64 {
        let latest = self
            .running_pids
            .iter()
            .map(|(time, _)| time)
            .max_by(|a, b| a.partial_cmp(b).unwrap());

        // requires get_latest_weaken_release
        match latest {
            None => Date::now(),
            Some(time) => time + grace_period,
        }
    }

    fn spawn_hgw(
        &mut self,
        ns: &NsWrapper<'_>,
        hgw: HGW,
        hackers: AHGHackerIterator,
        current_time: f64,
        run_time: f64,
        threads: usize,
        split_type: SplitType,
    ) -> Option<SmallVec<[RunningProcessMetadata; 4]>> {
        dbg!(threads);

        // if there are no available hackers to run our job, don't do it
        let hackers =
            match find_available_hackers(ns, hackers, threads, split_type) {
                Some(h) => h,
                None => return None,
            };

        if hackers.is_empty() {
            return None;
        }

        ns.tprint(&format!("hackers: {:#?}", hackers));

        // the run time is different from spawn time
        // the spawn time is immediate.
        // the run time is spawn time + sleep time
        // we have to determine how much time to sleep

        let sleep_time = run_time - current_time;
        let sleep_time_str = format!("{}", sleep_time);
        let mut metadatas = SmallVec::<[RunningProcessMetadata; 4]>::new();

        for (hacker, threads) in hackers {
            let maybe_pid = ns
                .exec(
                    hgw.script().filename,
                    hacker.get_hostname(),
                    Some(threads),
                    &[&*self.machine_name, &sleep_time_str],
                )
                .unwrap();

            let pid = match maybe_pid {
                Some(p) => p,

                // if the process was not spawned, abort
                None => {
                    for metadata in metadatas.into_iter() {
                        ns.kill(metadata.pid as i32);
                    }

                    return None;
                },
            };

            // create the metadata
            let metadata = RunningProcessMetadata {
                pid,
                threads,
            };

            metadatas.push(metadata);
        }

        ns.tprint(&format!("metas: {:#?}", metadatas));

        Some(metadatas)
    }

    fn on_poll(
        &mut self,
        ns: &NsWrapper<'_>,
        ctx: &mut EventLoopContext<AutoHackEventWrapped>,
        govr: &mut AutoHackGovernor,
    ) {
        use SplitType::*;
        use TargetState::*;

        let now = Date::now();

        match self.state.clone() {
            TotalWeaken(weakens_left) => {
                if weakens_left == 0 {
                    // correct implementation is below
                    // NOTE: assume that max_grow performs creating another poll
                    self.state = MaxGrow;
                    self.on_poll(ns, ctx, govr);

                    return;
                }

                // spawn weaken
                let maybe_pid_meta = self.spawn_hgw(
                    ns,
                    HGW::Weaken,
                    govr.get_hackers_iter(),
                    now,
                    now,
                    weakens_left,
                    PartialSplit,
                );

                let pid_meta = match maybe_pid_meta {
                    None => {
                        self.on_no_memory();
                        return;
                    },
                    Some(pidm) => pidm,
                };

                let new_weakens_left = weakens_left
                    - pid_meta.iter().map(|meta| meta.threads).sum::<usize>();

                // TODO: make sure that running_pids does not grow too large
                self.running_pids.push_front((now, pid_meta));

                // spawn another one grace period later. this will happen
                // regardless if it's finished or not
                ctx.add_event(AutoHackEventWrapped::new_poll_target(
                    // TODO: there should be a proper place where you get the
                    // grace period
                    now + MILLISECOND * 50. * 2.,
                    MILLISECOND * 50.,
                    self.machine_name.clone(),
                ));

                // update the state
                self.state = TotalWeaken(new_weakens_left);
            },

            MaxGrow => {
                // calculate how many grow and weakens we need to do
                let grows_required = get_potential_grow_amt(ns, &*self.machine);

                if grows_required == 0 {
                    self.state = Hack;
                    return;
                }

                let weakens_required = rational_mult_usize(
                    grows_required,
                    12.5f64.recip(),
                ).min(1);

                let hack_time = self.machine.get_hack_time(ns);

                // spawn the grow half
                let maybe_g_pid_meta = self.spawn_hgw(
                    ns,
                    HGW::Grow,
                    govr.get_hackers_iter(),
                    now,
                    now + (4. - 3.2) * hack_time - MILLISECOND * 50.,
                    grows_required,
                    NoSplit, // NEVER split grows.
                );

                let mut g_pid_meta = match maybe_g_pid_meta {
                    Some(m) => m,
                    None => {
                        self.on_no_memory();
                        return;
                    },
                };

                // spawn the weaken half
                let maybe_w_pid_meta = self.spawn_hgw(
                    ns,
                    HGW::Weaken,
                    govr.get_hackers_iter(),
                    now,
                    now,
                    weakens_required,
                    PartialSplit,
                );

                let w_pid_meta = match maybe_w_pid_meta {
                    Some(m) => m,
                    None => {
                        // kill the grows since we can't accompany it with
                        // weakens
                        for gpid in g_pid_meta.into_iter() {
                            ns.kill(gpid.pid as i32);
                        }

                        self.on_no_memory();
                        return;
                    },
                };

                // extend the PIDs
                g_pid_meta.extend(w_pid_meta.into_iter());

                self.running_pids.push_front((now, g_pid_meta));

                // TODO: spawn another poll one grace period + end time later
                ctx.add_event(AutoHackEventWrapped::new_poll_target(
                    // TODO: there should be a proper place where you get the
                    // grace period
                    now + hack_time * 4. + MILLISECOND * 50.,
                    MILLISECOND * 50.,
                    self.machine_name.clone(),
                ));
            },

            Hack => {
                // spawn one weaken, one grow, another weaken, then one hack
                unimplemented!();

                // check if spawns worked. if it did not, do on_no_memory()
                unimplemented!();
            },
        }
    }

    fn on_no_memory(&mut self) {
        self.is_waiting_for_memory = true;
    }

    // returns true when:
    //     memory is freed and we managed to continue on this machine
    //
    // return false if:
    //     the targeted machine is already being processed
    //     there still isn't enough memory to continue on this machine
    fn on_memory_freed(
        &mut self,
        ns: &NsWrapper<'_>,
        ctx: &mut EventLoopContext<AutoHackEventWrapped>,
        govr: &mut AutoHackGovernor,
    ) -> bool {
        if self.is_waiting_for_memory {
            // set is_waiting_for_memory to be false. it will be set true if we
            // tried to spawn and yet nothing happened
            self.is_waiting_for_memory = false;

            // TODO: check if this is still consistent
            self.on_poll(ns, ctx, govr);

            // is_waiting_for_memory will be true if there is still no
            // memory even if we did poll() so use that to check.
            return !self.is_waiting_for_memory;
        }
        else {
            false
        }
    }
}

#[derive(Debug)]
struct AutoHackGovernor {
    hackers: VecDeque<Arc<Machine>>,
    targets_by_name: HashMap<Arc<str>, Arc<Mutex<TargetStateBundle>>>,
    targets_by_score: Vec<(Arc<str>, Arc<Mutex<TargetStateBundle>>)>,

    hacking_level: usize,
    // TODO after everything works: create a buffer so you don't get to
    // allocate buffers every time, which is bad for a bump-based allocator like
    // bumpalo
}

impl AutoHackGovernor {
    pub fn new(ns: &NsWrapper<'_>) -> AutoHackGovernor {
        let mut ahg = AutoHackGovernor {
            hackers: VecDeque::new(),
            targets_by_name: HashMap::new(),
            targets_by_score: vec![],
            hacking_level: ns.get_player_hacking_level(),
        };

        ahg.regenerate_hackers_and_targets(ns);
        ahg
    }

    /// Returns a list of hostnames currently used as a hacker and a target.
    fn get_used_hostnames(
        &self,
        mut buffer: Vec<Arc<str>>,
    ) -> Vec<Arc<str>> {
        buffer.clear();

        let iter_1 = self.targets_by_name.keys().cloned();
        let iter_2 = self
            .hackers
            .iter()
            .map(|h| h.get_hostname())
            .map(|hn| Arc::from(hn));

        for val in iter_1.chain(iter_2) {
            buffer.push(val);
        }

        // sort then dedup
        buffer.sort_unstable();
        buffer.dedup();

        buffer
    }

    /// Returns a list of machines that are neither in current list of hackers
    /// and targets.
    fn get_new_machines(
        &self,
        ns: &NsWrapper<'_>,
        buffer_1: Vec<Arc<str>>,
    ) -> Vec<(Arc<str>, Arc<Machine>)> {
        let used_hostnames = self.get_used_hostnames(buffer_1);

        let hacking_level = ns.get_player_hacking_level();

        get_machines(ns)
            .into_iter()
            // don't allow machines that already exist in hackers and targets
            // so we don't consume ns function runtime
            .filter(|m| !used_hostnames
                .iter()
                .any(|uhn| &**uhn == m.get_hostname())
            )
            // only allow machines, on both hackers and targets, to be within
            // our hacking level
            .filter(|m| m.get_min_hacking_skill() <= hacking_level)
            // the machines must be rooted
            // TODO: just root it ourselves
            .filter(|m| m.is_root(ns))
            .map(|m| (Arc::from(m.get_hostname()), Arc::new(m)))
            .collect::<Vec<_>>()
    }

    /// Obtains new hackers from a list of machines.
    fn get_new_hackers_from(
        &mut self,
        ns: &NsWrapper<'_>,
        machines: &[(Arc<str>, Arc<Machine>)],
        buffer: &mut Vec<Arc<Machine>>,
    ) {
        use crate::script_deploy::{
            GROW_SCRIPT,
            HACK_SCRIPT,
            WEAKEN_SCRIPT,
        };

        buffer.clear();

        // add the new entries into the buffer first
        let iter = machines
            .iter()
            .map(|(_, m)| m)
            .filter(|m| 0 < m.get_max_gb_ram_hundredths(ns))
            // only allow hackers that can possess this file
            .filter_map(|h| {
                let deployed = WEAKEN_SCRIPT.deploy_to_machine(ns, h, true) &&
                    GROW_SCRIPT.deploy_to_machine(ns, h, true) &&
                    HACK_SCRIPT.deploy_to_machine(ns, h, true);

                deployed.then(|| h)
            })
            .cloned();
        buffer.extend(iter);

        // if there is nothing inside the buffer, exit
        if buffer.is_empty() {
            return;
        }

        // then move everything inside the vecdeque into this vec
        buffer.extend(self.hackers.drain(..));

        // sort everything reversed
        buffer.sort_by_cached_key(|m| m.get_max_gb_ram_hundredths(ns));
        buffer.reverse();

        // then move everything back into the vecdeque
        self.hackers.extend(buffer.drain(..));
    }

    /// Obtains new targets from a list of machines.
    fn get_new_targets_from(
        &mut self,
        ns: &NsWrapper<'_>,
        machines: &[(Arc<str>, Arc<Machine>)],
        buffer: &mut Vec<(Arc<str>, Arc<Mutex<TargetStateBundle>>)>,
    ) {
        buffer.clear();

        // add the new entries into the buffer first
        let iter = machines
            .iter()
            // can be filled with money
            .filter(|(_, m)| 0 < m.get_max_money())
            // is not in our list of targets
            .filter(|(hn, _)| !self.targets_by_name.contains_key(hn))
            .cloned()
            // convert it into a TargetStateBundle
            .map(|(hn, m)| (hn.clone(), Arc::new(Mutex::new(TargetStateBundle::new(ns, hn, m)))));
        buffer.extend(iter);

        // if there is nothing inside the buffer, exit
        if buffer.is_empty() {
            return;
        }

        // then move everything back into the hash map
        self.targets_by_name.extend(buffer.iter().cloned());

        // also add it into the sorted by score
        self.targets_by_score.extend(buffer.drain(..));

        // then sort
        self.resort_targets_by_score(ns);
    }

    fn resort_targets_by_score(&mut self, ns: &NsWrapper<'_>) {
        self.targets_by_score.sort_by_cached_key(|(_, m)| {
            let mlock = m.lock().unwrap();
            let avg_yld = mlock.machine.get_average_yield(ns);
            decorum::N64::from_inner(avg_yld)
        });
    }

    /// Regenerates a list of hackers and targets.
    fn regenerate_hackers_and_targets(
        &mut self,
        ns: &NsWrapper<'_>,
    ) {
        // TODO: make sure that these buffers come from the current object
        // itself
        let buffer_1 = vec![];

        let new_machines = self.get_new_machines(ns, buffer_1);

        let mut buffer_3 = vec![];
        let mut buffer_4 = vec![];

        self.get_new_hackers_from(ns, &new_machines, &mut buffer_3);
        self.get_new_targets_from(ns, &new_machines, &mut buffer_4);
    }

    /// Obtains an iterator over hackers.
    ///
    /// This iterator is especially created to always return the same value
    /// if next() is called once then dropped.
    fn get_hackers_iter<'a>(&'a mut self) -> AHGHackerIterator<'a> {
        let rotations_left = self.hackers.len();

        AHGHackerIterator {
            governor: self,
            has_called_next: false,
            rotations_left,
        }
    }

    fn do_level_up_check(&mut self, ns: &NsWrapper<'_>) {
        let level = ns.get_player_hacking_level();

        if level == self.hacking_level {
            return;
        }

        // if we've levelled up, do many things

        // only regenerate hackers and targets upon level up
        self.hacking_level = level;
        self.regenerate_hackers_and_targets(ns);

        // set everything back to total weaken
        for (_, target) in self.targets_by_score.iter() {
            let mut target_lock = target.lock().unwrap();

            target_lock.state = TargetState::TotalWeaken(target_lock.machine.get_weaken_threads_to_reduce(ns));
        }

        // resort targets by score
        self.resort_targets_by_score(ns);
    }
}

impl EventLoopState for AutoHackGovernor {
    type Event = AutoHackEventWrapped;

    fn initial_run<'a>(
        &mut self,
        _ns: &NsWrapper<'a>,
        ctx: &mut EventLoopContext<Self::Event>,
    ) {
        // poll each target near immediately
        let next_second = Date::now() + SECOND;

        // TODO: you might want to put this into a function so you can call it
        // on_level_up()
        for (name, _) in self.targets_by_name.iter() {
            let event = AutoHackEventWrapped::new_poll_target(
                next_second,
                MILLISECOND * 50.,
                name.clone(),
            );

            ctx.add_event(event);
        }

        // create a poll to update the level
        let event = AutoHackEventWrapped::new_general_poll(
            next_second,
            MILLISECOND * 50.,
        );

        ctx.add_event(event);
    }

    fn on_event<'a>(
        &mut self,
        ns: &NsWrapper<'a>,
        event: Self::Event,
        ctx: &mut EventLoopContext<Self::Event>,
    ) {
        use AutoHackEventType::*;

        match event.event_type {
            PollTarget(target) => {
                let target =
                    self.targets_by_name.get(&*target).unwrap().clone();
                let mut target_lock = target.lock().unwrap();
                target_lock.on_poll(ns, ctx, self);
            },

            MemoryFreed(_) => {
            },

            GeneralPoll => {
                self.do_level_up_check(ns);

                // spawn another general poll request
                ctx.add_event(
                    AutoHackEventWrapped::new_general_poll(
                        Date::now() + SECOND,
                        MILLISECOND * 50.
                    )
                );
            },
        }
    }

    fn on_event_fail<'a>(
        &mut self,
        ns: &NsWrapper<'a>,
        event: Self::Event,
        ctx: &mut EventLoopContext<Self::Event>,
    ) {

    }

    fn post_loop_inspect<'a>(&self, ns: &NsWrapper<'a>, event_heap: &BinaryHeap<EventWrapper<Self::Event>>) {
    }
}

/// Iterator over the list of hackers.
///
/// This list will prioritize utilizing machines that have a lot of RAM first.
/// By design, if this iterator is called using next() and then dropped,
/// instantiating another of this iterator then calling next() will return the
/// same Machine.
///
/// This is heavily used for hacking machines.
struct AHGHackerIterator<'a> {
    governor: &'a mut AutoHackGovernor,
    has_called_next: bool,
    rotations_left: usize,
}

impl<'a> AHGHackerIterator<'a> {
    /// Returns the next machine that has at least a given memory requirement.
    fn next_available_unit(
        &mut self,
        ns: &NsWrapper<'_>,
        memory_requirement_hundredths: u64,
    ) -> Option<(Arc<Machine>, u64)> {
        for machine in self.by_ref() {
            let max_ram = machine.get_max_gb_ram_hundredths(ns);
            let free_ram = machine.get_free_ram_hundredths(ns);

            let max_usable_ram = rational_mult_u64(max_ram, RESERVATION_RATE);
            let used_ram = max_ram - free_ram;

            if max_usable_ram <= used_ram {
                // calculate the number of instances that we can produce using
                // given memory requirement
                let instances =
                    (max_usable_ram - used_ram) / memory_requirement_hundredths;

                // if there is at least one instance, we can use the machine
                if 0 < instances {
                    return Some((machine, instances));
                }
            }
        }

        None
    }
}

impl<'a> Iterator for AHGHackerIterator<'a> {
    type Item = Arc<Machine>;

    fn next(&mut self) -> Option<Self::Item> {
        // if we've fully rotated the iterator, don't return anything else
        if self.rotations_left == 0 {
            return None;
        }

        // adjust the amount of rotations to do
        self.rotations_left -= 1;

        if self.has_called_next {
            // if we've called next already, rotate by popping the front element
            // and pushing it to the back
            let front = self.governor.hackers.pop_front().unwrap();
            self.governor.hackers.push_back(front);
        }

        self.has_called_next = true;
        self.governor.hackers.front().cloned()
    }
}

fn get_potential_grow_amt(ns: &NsWrapper<'_>, machine: &Machine) -> usize {
    let money = machine.get_money_available(ns).max(1);
    let growth_factor = machine.get_max_money() as f64 / money as f64;

    ns.growth_analyze(machine.get_hostname(), growth_factor, None).ceil() as usize
}
