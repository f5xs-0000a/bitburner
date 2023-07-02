use std::{
    collections::{
        HashMap,
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
    //Hack,
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
                    // unimplemented!(); we move to actual hacking
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
                    now, // TODO: are these correct?
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

                // spawn both
                let maybe_w_pid_meta = self.spawn_hgw(
                    ns,
                    HGW::Weaken,
                    govr.get_hackers_iter(),
                    now, // TODO: are these correct?
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

            /*
            Hack => {
                // spawn one weaken, one grow, another weaken, then one hack
                unimplemented!();

                // check if spawns worked. if it did not, do on_no_memory()
                unimplemented!();
            },
            */
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
                let deployed = WEAKEN_SCRIPT.deploy_to_machine(ns, h) &&
                    GROW_SCRIPT.deploy_to_machine(ns, h) &&
                    HACK_SCRIPT.deploy_to_machine(ns, h);

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

        crate::debug!(ns, "{:?}", self);

        match event.event_type {
            PollTarget(target) => {
                let target =
                    self.targets_by_name.get(&*target).unwrap().clone();
                let mut target_lock = target.lock().unwrap();
                target_lock.on_poll(ns, ctx, self);
            },

            MemoryFreed(_) => {
                unimplemented!()
            },

            GeneralPoll => {
                self.do_level_up_check(ns);

                // spawn another general poll request
                ctx.add_event(
                    AutoHackEventWrapped::new_general_poll(
                        Date::now() + MILLISECOND * 50.,
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
        unimplemented!();
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

/*
struct AutohackGovernor {
    grace_period: f64, // milliseconds

    // list of machines there are
    machines: VecDeque<Arc<Machine>>,

    // really good ones at the front, bad ones at the back
    hackers: VecDeque<Arc<Machine>>,

    current_level: usize,
}

impl AutohackGovernor {
    fn on_level_update(&mut self) {
        unimplemented!()
    }

    fn do_level_check(&mut self, ns: &NsWrapper<'_>) {
        let new_level = ns.get_player_hacking_level();

        if new_level == self.current_level {
            return;
        }

        self.current_level = new_level;
        self.on_level_update();
    }
}

impl EventLoopState for AutohackGovernor {
    type Event = HackStates;

    fn initial_run(
        &mut self,
        ns: &NsWrapper<'_>,
        ctx: &mut EventLoopContext<Self::Event>,
    ) {
        let now = Date::now();

        // spawn as many weakeners as we can
        for machine in self.machines.iter() {
            if machine.is_player_owned() {
                continue;
            }

            if !machine.is_root(ns) {
                continue;
            }

            if self.current_level < machine.get_min_hacking_skill() {
                continue;
            }

            if 0 < machine.get_max_money() {
                continue;
            }

            let allowed_spawn_time = unimplemented!();

            // spawn the weakener
            TotalWeakener::spawn(
                ns,
                machine.clone(),
                allowed_spawn_time,
                ctx,
                self
            );
        }

        // also spawn checks on levels, one second later
        ctx.add_event(HackStates::LevelChecker(now + SECOND));
    }

    fn on_event(
        &mut self,
        ns: &NsWrapper<'_>,
        event: Self::Event,
        ctx: &mut EventLoopContext<Self::Event>,
    ) {
        use HackStates::*;

        match event {
            LevelCheck => self.do_level_check(ns),
            TotalWeakener(t) => match t.on_event(ns, ctx, self) {
                Ok(()) => {},
                Err(e) => unimplemented!(),
            }
        }
    }

    fn on_event_fail(
        &mut self,
        ns: &NsWrapper<'_>,
        event: Self::Event,
        ctx: &mut EventLoopContext<Self::Event>,
    ) {
        use HackStates::*;

        match event {
            LevelCheck => self.do_level_check(ns),
            TotalWeakener(t) => t.on_event_fail(),
        }
    }
}

impl AutohackGovernor {
    pub fn new(ns: &NsWrapper<'_>) -> AutohackGovernor {
        // TODO: you might want to consider moving this to initial_run
        let hackers = get_machines(ns)
            .iter()
            // the hackers must be rooted
            .filter(|m| m.is_root(ns))
            // the hackers must have free RAM
            .filter(|m| 0 < m.get_max_gb_ram_hundredths(ns))
            // only allow hackers that can possess this file
            .filter_map(|h| {
                ns.scp("child_weaken.js", &h.get_hostname(), "home");
                ns.scp("child_grow.js", &h.get_hostname(), "home");
                ns.scp("child_hack.js", &h.get_hostname(), "home");

                if ns.file_exists("child_weaken.js", h.get_hostname()) {
                    Some(h)
                }

                else {
                    None
                }
            })
            .map(|m| Arc::new(m.clone()))
            .collect::<VecDeque<_>>();

        AutohackGovernor {
            grace_period: 50., // TODO: add a proper value for this
            hackers,
        }
    }

    pub async fn run(self, ns: &NsWrapper<'_>) {
        // TODO: you haven't selected which machines to hack on.

        let mut event_loop = EventLoop::new(self);
        event_loop.run(ns).await;
    }

    fn get_hackers_iter<'a>(&'a mut self) -> AHGHackerIterator<'a> {
        let rotations_left = self.hackers.len();

        AHGHackerIterator {
            governor: self,
            has_called_next: false,
            rotations_left,
        }
    }
}

enum HackStates {
    LevelChecker(f64),
    TotalWeakener(TotalWeakener),
    // more to go, like:
    // Grower,
    // BatchHacker,
}

impl Event for HackStates {
    fn trigger_time(&self) -> f64 {
        use HackStates::*;

        match self {
            LevelChecker(tt) => tt,
            TotalWeakener(tw) => unimplemented!(),
        }
    }

    fn grace_period(&self) -> f64 {
        use HackStates::*;

        match self {
            LevelChecker(_) => f64::MAX,
            TotalWeakener(tw) => unimplemented!(),
        }
    }
}

struct TotalWeakener {
    grace_period: f64,
    target: Arc<Machine>,
    weakens_left: usize,
}

impl TotalWeakener {
    fn spawn_inner(
        ns: &NsWrapper<'_>,
        machine: Arc<Machine>,
        spawn_time: f64,
        govr: &mut AutohackGovernor,
    ) -> TotalWeakener {
        let weakens_left = machine.get_weaken_threads_to_reduce(ns);

        TotalWeakener {
            grace_period: govr.grace_period,
            target: machine,
            weakens_left,
        }
    }

    pub fn spawn(
        ns: &NsWrapper<'_>,
        machine: Arc<Machine>,
        spawn_time: f64,
        ctx: &mut EventLoopContext<HackStates>,
        govr: &mut AutohackGovernor,
    ) {
        let weakener = Self::spawn_inner(ns, machine, spawn_time, govr);

        // add this job into the pool
        unimplemented!();
    }

    pub fn on_event(
        mut self,
        ns: &NsWrapper<'_>,
        ctx: &mut EventLoopContext<HackStates>,
        govr: &mut AutohackGovernor,
    ) -> Result<(), Self> {
        // this state has been chosen to be the next to invoke its event
        // check first for machine that we can use
        while 0 < self.weakens_left {
            if let Some((machine, max_instances)) = govr.get_hackers_iter().next_available_unit(ns, EXEC_MEMORY_USAGE_HUNDREDTHS) {
                let instances = max_instances.min(self.weakens_left);

                ns.exec(
                    WEAKEN_SCRIPT.filename,
                    machine.get_hostname(),
                    Some(instances),
                    &[self.target.get_hostname()]
                ).unwrap();

                self.weakens_left -= instances;

                // TODO: do we wait for this to finish then trigger another event?
                unimplemented!();
            }

            // if there are no machines that are usable, return this as an error
            else {
                return Err(self);
            }
        }

        // if we've reached this, then we've successfully spawned as many
        // threads as we can to minimize the security of this machine
        //
        // only then we can spawn, one grace period later, a job that will
        // maximally grow a server
        // it's unimplemented!() for now.

        Ok(())
    }
}
*/

/*
struct MachinePrepper {
    grace_period: f64,
    machine: Arc<Machine>,
    last_level: usize,

    weakens_left: usize,
    after_weaken_weakens_left: usize

    next_event: (),
}

impl MachinePrepper {
    fn on_event(
        self,
        ctx: &mut EventLoopContext<()>,
        govr: &mut AutohackGovernor
    ) -> Result<(), Self> {
        // what event are we in right now though?
        if 0 < after_weaken_weakens_left {
            self.spawn_weakener();
        }
    }

    fn on_failure(
    ) -> Result<(), Self> {
    }
}
*/

/*
pub struct HackLoopState {
    // target machiens should be indexable by name but also must have the most
    // performant be the first to be chosen
    machines: HashMap<Arc<str>, HackLoopStage>,
}

impl HackLoopState {
    fn do_level_up_check(&mut self, ctx: &mut EventLoopContext<HackLoopEvents>) {
        unimplemented!();
        ctx.add_event(LevelUpCheck);
    }
}

impl EventLoopState for HackLoopStage {
    type Event = HackLoopEvents;

    fn initial_run(ns: &NsWrapper<'_>, ctx: &mut EventLoopContext<HackLoopEvents>)
    {
        // for each of the targetted machines, push the event
        self.machines.values().on_start(ns, ctx);

        // check for level up too
        ctx.push(HackLoopEvents::LevelUpCheck);
    }

    fn on_event(ns: &NsWrapper<'_>, event: HackLoopEvents, ctx: &mut EventLoopContext<HackLoopEvents>) {
        match event {
            LevelUpCheck => self.do_level_up_check(ctx),
        }
    }

    fn on_event_fail(ns: &NsWrapper<'_>, event: HackLoopEvents, ctx: &mut EventLoopContext<HackLoopEvents>) {
        match event {
            // don't care, still check for level up
            LevelUpCheck => self.do_level_up_check(ctx),
        }
    }
}

enum HackLoopStage {
    WeakenGrow(WeakenGrow),
    HackAnalysis(HackAnalysis),
    HackLoop(HackLoop),
}

enum HackLoopEvents {
    LevelUpCheck,
}

// the stage of a single target machine on being weakened and grown
pub struct WeakenGrow {
    machine: Arc<Machine>,

    weakens_left: usize,

    // if None, then we have fully drained a machine
    grows_required: Option<usize>,
}

impl WeakenGrow {
    pub fn new(target: Arc<Machine>) -> WeakenGrow {
        let mut grows_requried = None;
        let available_money = machine.get_money_available(ns);

        if 0 < available_money {
            let multiplier = machine.get_max_money(ns) as f64 / available_money as f64;
            grows_requried = Some(ns.growthAnalyze(ns, multiplier).ceil() as usize);
        }

        let weakens_left = machine.get_weaken_threads_to_reduce(ns);

        WeakenGrow {
            machine: target,
            weakens_left,
            grows_required,
        }
    }

    pub fn initial_run(&mut self, ns: &NsWrapper<'_>, ctx: &mut EventLoopContext<HSEvent>) {
        // send a weaken event
        ctx.add_event(unimplemented!());

        // if we're growing this machine...
        match self.grows_required() {
            // dedicae all
        }

        // send another signal to do grow
        ctx.add_event(unimplemented!());
    }

    // initial run

    // on event
    //
    // TODO: not done here yet
}
*/

/*
struct MachineAnalysis {
    machine: Arc<Machine>,
    hack_time: f64,
    min_security_thousandths: f64,
}

enum HackStage {
}

enum HSEvent {
    // continues an HWGW thread
    ContinueHWGW(String, usize),
    EndHWGW(String, usize),
    SpawnHWGW(String),
    CheckLevel
}

enum HWGWState {
    // first spawned, second to end
    FirstWeaken,
    // second spawned, third to end
    SecondWeaken,
    // third spawned, second to end
    Grow,
    // last spawned, fourth to end
    Hack,
}

// weakens machines
struct HackState {
    pre_analysis: HashMap<Arc<Machine>>,
    hack: Vec<MachineAnalysis>,
    last_level_check: usize,
    hacks_per_grow: usize,

    last_hwgw_id: usize,
    running_hwgw_threads: HashMap<usize, HWGW>,
}

impl HackState {
    fn do_spawn_hwgw_event(&mut self, id: usize, ctx: &mut EventLoopContext<HSEvent>) {

    }

    fn on_continue_hwgw_event(&mut self, id: usize, ctx: &mut EventLoopContext<HSEvent>) {

    }
}

impl HackState {
    fn new() -> HackState {
    }

    // do things on level up.
    // you usually would reanalyze everything.
    fn on_level_up() {

    }
}
*/

/*
const REFRESH_GRACE_PERIOD_MILLIS: usize = 10;
const RESERVATION_RATE: f64 = 0.9;
const BATCH_GRACE_PERIOD_MILLIS: usize = 100;

async fn sleep_until(
    ns: &NsWrapper<'_>,
    time: f64,
) -> bool {
    let now = Date::now();

    if now < time {
        ns.sleep((time - now) as i32).await;
        true
    }
    else {
        false
    }
}

#[derive(Clone, Copy, Debug)]
pub enum JobType {
    Grow,
    Hack,
    Weaken,
}

#[derive(Clone, Debug)]
pub struct HGWJob {
    job_type: JobType,
    running_machine: String,
    pid: usize,
    end_time: f64,
}

impl PartialOrd for HGWJob {
    fn partial_cmp(
        &self,
        other: &Self,
    ) -> Option<Ordering> {
        self.end_time.partial_cmp(&other.end_time)
    }
}

impl Ord for HGWJob {
    fn cmp(
        &self,
        other: &Self,
    ) -> Ordering {
        self.partial_cmp(&other).unwrap()
    }
}

impl PartialEq for HGWJob {
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        self.end_time.eq(&other.end_time)
    }
}

impl Eq for HGWJob {
    // forbidden magic
    fn assert_receiver_is_total_eq(&self) {}
}

impl HGWJob {
    fn spawn_common(
        ns: &NsWrapper,
        script_name: &str,
        hostname: &str,
        threads: usize,
        target: &str,
    ) -> Option<usize> {
        assert!(0 < threads);
        ns.exec(script_name, hostname, Some(threads), &[target]).unwrap()
    }

    pub fn grow(
        ns: &NsWrapper,
        hacker: &Machine,
        target: &Machine,
        threads: usize,
    ) -> HGWJob {
        let pid = HGWJob::spawn_common(
            ns,
            "child_grow.js",
            hacker.get_hostname(),
            threads,
            target.get_hostname(),
        )
        .unwrap();

        HGWJob {
            job_type: JobType::Grow,
            running_machine: hacker.get_hostname().to_owned(),
            pid,
            end_time: Date::now() + target.get_grow_time(ns),
        }
    }

    pub fn weaken(
        ns: &NsWrapper,
        hacker: &Machine,
        target: &Machine,
        threads: usize,
    ) -> HGWJob {
        let pid = HGWJob::spawn_common(
            ns,
            "child_weaken.js",
            hacker.get_hostname(),
            threads,
            target.get_hostname(),
        )
        .unwrap();

        HGWJob {
            job_type: JobType::Weaken,
            running_machine: hacker.get_hostname().to_owned(),
            pid,
            end_time: Date::now() + target.get_weaken_time(ns),
        }
    }

    pub fn hack(
        ns: &NsWrapper,
        hacker: &Machine,
        target: &Machine,
        threads: usize,
    ) -> HGWJob {
        let pid = HGWJob::spawn_common(
            ns,
            "child_hack.js",
            hacker.get_hostname(),
            threads,
            target.get_hostname(),
        )
        .unwrap();

        HGWJob {
            job_type: JobType::Hack,
            running_machine: hacker.get_hostname().to_owned(),
            pid,
            end_time: Date::now() + target.get_hack_time(ns),
        }
    }

    pub fn get_job_type(&self) -> JobType {
        self.job_type
    }

    pub fn get_pid(&self) -> usize {
        self.pid
    }

    pub fn get_end_time(&self) -> f64 {
        self.end_time
    }

    pub async fn wait_until_end(
        &self,
        ns: &NsWrapper<'_>,
    ) {
        sleep_until(ns, self.end_time).await;

        while ns.is_running(self.get_pid()) {
            ns.sleep(REFRESH_GRACE_PERIOD_MILLIS as i32).await;
        }
    }
}

struct TotalWeakener {
    hackers: Vec<(Machine, BinaryHeap<HGWJob, MinComparator>)>,
    targets: Vec<(Machine, usize)>,

    current_hacker_index: usize,
    current_target_index: usize,
}

impl TotalWeakener {
    pub fn new(
        ns: &NsWrapper,
        machines: &[Machine],
    ) -> TotalWeakener {
        let current_hacking_level = ns.get_player_hacking_level();

        let hackers = machines
            .iter()
            .filter(|m| m.is_root(ns))
            .filter(|m| 0 < m.get_max_gb_ram_hundredths(ns))
            // only allow hackers that can possess this file
            .filter_map(|h| {
                ns.scp("child_weaken.js", &h.get_hostname(), "home");
                ns.scp("child_grow.js", &h.get_hostname(), "home");
                ns.scp("child_hack.js", &h.get_hostname(), "home");

                if ns.file_exists("child_weaken.js", h.get_hostname()) {
                    Some(h)
                }

                else {
                    None
                }
            })
            .cloned()
            .map(|m| (m, BinaryHeap::new_min()))
            .collect::<Vec<_>>();

        let mut targets = machines
            .iter()
            // hack only machines that are
            // not owned by the player
            .filter(|m| !m.is_player_owned())
            // rooted
            .filter(|m| m.is_root(ns))
            // has a required hacking level lower than what we have
            .filter(|m| m.get_min_hacking_skill() <= current_hacking_level)
            // can be filled with money
            .filter(|m| 0 < m.get_max_money())
            .map(|m| {
                let threads = m.get_weaken_threads_to_reduce(ns);
                (m, threads)
            })
            // and needs to be weakened
            .filter(|(_, count)| 0 < *count)
            .map(|(m, count)| (m.clone(), count))
            .collect::<Vec<_>>();
        targets.sort_unstable_by(|(m1, _), (m2, _)| {
            m1.get_weaken_time(ns)
                .partial_cmp(&m2.get_weaken_time(ns))
                .unwrap()
                .reverse()
        });

        TotalWeakener {
            hackers,
            targets,
            current_hacker_index: 0,
            current_target_index: 0,
        }
    }

    pub fn display_targets(
        &self,
        ns: &NsWrapper,
    ) {
        let mut output = "\nTargets to weaken:\n".to_owned();

        let (hostname_len, ..) =
            crate::scan::get_longest_stuff(self.targets.iter().map(|(m, _)| m));

        // print header
        output += "Hostname";
        for _ in "Hostname".len() .. hostname_len {
            output += " ";
        }

        output += "  Duration  Threads Left\n";

        for (machine, threads_left) in self.targets.iter() {
            output += &format!(
                "{: <hnl$}  {:>7.2}s  {:^12}\n",
                machine.get_hostname(),
                machine.get_weaken_time(ns) / 1000.,
                threads_left,
                hnl = hostname_len
            );
        }

        ns.tprint(&output);
    }

    pub async fn run(
        &mut self,
        ns: &NsWrapper<'_>,
    ) {
        if self.targets.is_empty() {
            return;
        }

        // the progression goes from left to right
        let mut last_successful_pair =
            Some((self.current_hacker_index, self.current_target_index));
        while 0 < self.targets.last().unwrap().1 {
            let cur_hacker_idx = self.current_hacker_index;
            let cur_target_threads_rem = self
                .targets
                .get_mut(self.current_target_index)
                .unwrap()
                .1
                .clone();
            let cur_target_idx = self.current_target_index;

            self.step(ns).await;

            let mut success = false;

            // if the target index incremented, thread spawning is a success
            if self.current_target_index != cur_target_idx {
                success = true;
            }

            // if the number of threads reduced, thread spawning is a success
            if self.targets.get(cur_target_idx).unwrap().1
                == cur_target_threads_rem
            {
                success = true
            }

            if success {
                last_successful_pair = Some((
                    self.current_hacker_index,
                    self.current_target_index,
                ));
            }
            // at this point, step() has incremented the current hacker all the
            // way around. we have to wait until the next hacker is available.
            else if last_successful_pair
                != Some((self.current_hacker_index, self.current_target_index))
            {
                self.wait_for_next_job_finish(ns).await;
            }
        }

        self.wait_for_end(ns).await;
    }

    /// Returns whether the hacking attempt was successful or not for this
    /// machine.
    async fn step(
        &mut self,
        ns: &NsWrapper<'_>,
    ) {
        let (current_hacker, ref mut current_jobs) =
            self.hackers.get_mut(self.current_hacker_index).unwrap();
        let (current_target, ref mut current_threads_remaining) =
            self.targets.get_mut(self.current_target_index).unwrap();

        // increment the pointer if there are no threads required for this
        // machine
        if *current_threads_remaining == 0 {
            self.current_target_index += 1;
            return;
        }

        // not enough RAM on this machine. get to the next one.
        let max_ram = {
            let mut mr = current_hacker.get_max_gb_ram_hundredths(ns);
            mr *= (RESERVATION_RATE * 1000.) as usize;
            mr /= 1000;
            mr
        };

        // fail because we don't have enough memory
        if current_hacker.get_free_ram_hundredths(ns) < max_ram {
            self.current_hacker_index =
                (self.current_hacker_index + 1) % self.hackers.len();
            return;
        }

        // make sure that the number of threads to use
        // - still follows the reservation rate, to an extent
        // - is not larger than weaken_threads_left
        // - is at least 1
        let mut threads_to_use = current_hacker.get_threads_left(ns);
        threads_to_use *= (RESERVATION_RATE * 1000.) as usize;
        threads_to_use /= 1000;
        threads_to_use = threads_to_use.min(*current_threads_remaining);
        threads_to_use = threads_to_use.max(1);

        // perform hacking
        let job =
            HGWJob::weaken(ns, current_hacker, current_target, threads_to_use);
        current_jobs.push(job);

        *current_threads_remaining -= threads_to_use;
    }

    async fn wait_until_next_or_end_finish(
        &mut self,
        ns: &NsWrapper<'_>,
        is_next: bool,
    ) {
        let mut best: Option<(usize, &mut BinaryHeap<HGWJob, MinComparator>)> =
            None;

        // determine which job finishes first
        for (idx, (_, jobs)) in self.hackers.iter_mut().enumerate() {
            let jobs = match jobs.peek() {
                // don't take it. just return the jobs.
                Some(_) => jobs,
                None => continue,
            };

            if let Some((ref mut best_idx, ref mut best_jobs)) = best.as_mut() {
                let this_job_end_time = jobs.peek().unwrap().get_end_time();
                let best_job_end_time =
                    best_jobs.peek().unwrap().get_end_time();

                // wait for next job
                if is_next && this_job_end_time < best_job_end_time {
                    *best_idx = idx;
                    *best_jobs = jobs;
                }
                // wait for end job
                else if !is_next && best_job_end_time < this_job_end_time {
                    *best_idx = idx;
                    *best_jobs = jobs;
                }
            }
            else {
                best = Some((idx, jobs));
            }
        }

        // if there's a job, wait for it to finish.
        if let Some((idx, jobs)) = best {
            jobs.pop().unwrap().wait_until_end(ns).await;
            self.current_hacker_index = idx;
        }
    }

    async fn wait_for_next_job_finish(
        &mut self,
        ns: &NsWrapper<'_>,
    ) {
        self.wait_until_next_or_end_finish(ns, true).await;
    }

    async fn wait_for_end(
        &mut self,
        ns: &NsWrapper<'_>,
    ) {
        self.wait_until_next_or_end_finish(ns, false).await;
    }
}

#[derive(Debug)]
enum EventType {
    FirstWeaken,
    Grow,
    SecondWeaken,
    Hack,
    Finish,
}

#[derive(Debug)]
struct TimedEvent {
    time: f64,
    batch_id: usize,
    event_type: EventType,
}

impl PartialEq for TimedEvent {
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        self.time == other.time
    }
}

impl Eq for TimedEvent {}

impl PartialOrd for TimedEvent {
    fn partial_cmp(
        &self,
        other: &Self,
    ) -> Option<Ordering> {
        self.time.partial_cmp(&other.time).map(|o| o.reverse())
    }
}

impl Ord for TimedEvent {
    fn cmp(
        &self,
        other: &Self,
    ) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

#[derive(Debug)]
struct HackerTargetPair {
    hacker: Arc<Machine>,
    target: Arc<Machine>,
}

#[derive(Debug)]
struct BatchHacker {
    events: BinaryHeap<TimedEvent>,
    batches: HashMap<usize, HackerTargetPair>,

    missed_spawns: usize,
}

impl BatchHacker {
    fn push_new_jobs_to_events(
        &mut self,
        ns: &NsWrapper,
        mut time: f64,
        batch_id: usize,
        spawn_now: bool,
    ) {
        use EventType::*;

        let batch = self.batches.get(&batch_id).unwrap();
        let (hack_time, grow_time, weaken_time) = batch.target.get_hgw_time(ns);

        let mut now = Date::now();

        // first weaken
        // first to spawn
        // second to finish
        if spawn_now || time < now {
            // immediately spawn if it's already time
            HGWJob::weaken(ns, &*batch.hacker, &*batch.target, 1);
            time = now;
        }

        else {
            self.events.push(TimedEvent {
                time,
                event_type: FirstWeaken,
                batch_id,
            });
        }

        // hack
        // fourth to spawn
        // first to finish
        self.events.push(TimedEvent {
            time: time + weaken_time
                - hack_time
                - BATCH_GRACE_PERIOD_MILLIS as f64,
            event_type: Hack,
            batch_id,
        });

        // grow
        // third to spawn
        // second to finish
        self.events.push(TimedEvent {
            time: time + weaken_time - grow_time
                + BATCH_GRACE_PERIOD_MILLIS as f64,
            event_type: Grow,
            batch_id,
        });

        // second weaken
        // second to spawn
        // last to finish
        self.events.push(TimedEvent {
            time: time + 2. * BATCH_GRACE_PERIOD_MILLIS as f64,
            event_type: SecondWeaken,
            batch_id,
        });

        // finish, spawn another
        self.events.push(TimedEvent {
            time: time + weaken_time + 2. * BATCH_GRACE_PERIOD_MILLIS as f64,
            event_type: Finish,
            batch_id,
        });
    }

    pub async fn run(
        &mut self,
        ns: &NsWrapper<'_>,
    ) {
        use EventType::*;

        loop {
            let event = self.events.pop().unwrap();

            let batch = self.batches.get(&event.batch_id).unwrap();

            sleep_until(ns, event.time).await;

            match event.event_type {
                FirstWeaken | SecondWeaken => {
                    HGWJob::weaken(ns, &*batch.hacker, &*batch.target, 1);
                },
                Grow => {
                    HGWJob::grow(ns, &*batch.hacker, &*batch.target, 1);
                },
                Hack => {
                    HGWJob::hack(ns, &*batch.hacker, &*batch.target, 1);
                },
                // on finish, spawn another
                Finish => {
                    self.push_new_jobs_to_events(
                        ns,
                        f64::NEG_INFINITY,
                        event.batch_id,
                        true,
                    );
                },
            }
        }
    }

    /// Use it immediately
    fn new(
        ns: &NsWrapper<'_>,
        machines: &[Machine],
    ) -> BatchHacker {
        // TODO: you still have to test each machines for how much you can grow
        // them compared to how much you can hack them

        const STEP_DURATION_MS: f64 = 500.;

        let current_hacking_level = ns.get_player_hacking_level();

        let mut hackers = machines
            .iter()
            .filter(|m| m.is_root(ns))
            .filter(|m| 0 < m.get_max_gb_ram_hundredths(ns))
            // only allow hackers that can possess this file
            .filter_map(|h| {
                ns.scp("child_weaken.js", &h.get_hostname(), "home");
                ns.scp("child_grow.js", &h.get_hostname(), "home");
                ns.scp("child_hack.js", &h.get_hostname(), "home");

                if ns.file_exists("child_weaken.js", h.get_hostname()) {
                    Some(h)
                }

                else {
                    None
                }
            })
            // four: two weakens, one grow, and one hack
            .map(|m| {
                let allowed_threads = m.get_threads_left(ns)
                    * (RESERVATION_RATE * 1000.) as usize / (4 * 1000);
                (m, allowed_threads)
            })
            .collect::<Vec<_>>();

        // divided by four: two weakens, one hack, and one grow
        let max_batches_allowed =
            hackers.iter().map(|(m, threads)| threads).sum::<usize>();

        let mut targets = machines
            .iter()
            // hack only machines that are
            // not owned by the player
            .filter(|m| !m.is_player_owned())
            // rooted
            .filter(|m| m.is_root(ns))
            // has a required hacking level lower than what we have
            .filter(|m| m.get_min_hacking_skill() <= current_hacking_level)
            // and can be filled with money
            .filter(|m| 0 < m.get_max_money())
            .map(|m| {
                let avg_yield = m.get_average_yield(ns);

                let duration = m.get_weaken_time(ns);
                let mut steps = (duration / STEP_DURATION_MS).ceil() as usize;
                steps = steps.min(max_batches_allowed);

                (m, avg_yield, steps, duration * steps as f64)
            })
            .collect::<Vec<_>>();

        // reorder the vector by the total batch yield, top to bottom
        targets.sort_unstable_by(|(_, _, _, tby1), (_, _, _, tby2)| {
            tby1.partial_cmp(tby2).unwrap().reverse()
        });

        // remove the machines we won't be needing because we have too few
        // threads
        let mut max_size = 0;
        let mut over_difference = 0;
        let mut remaining_threads = max_batches_allowed;
        for (idx, (_, _, steps, _)) in targets.iter_mut().enumerate() {
            if *steps < remaining_threads {
                remaining_threads -= *steps;
                max_size = idx + 1;
            }
            // at this point, we don't have enough memory to go by. remove those
            // that are much less profitable and only work with these.
            else {
                max_size = idx + 1;
                *steps = remaining_threads;
                break;
            }
        }
        targets.truncate(max_size);

        let mut output = "\nTargets to hack:\n".to_owned();

        let (hostname_len, ..) =
            crate::scan::get_longest_stuff(targets.iter().map(|(m, ..)| *m));

        // print header
        output += "Hostname";
        for _ in "Hostname".len() .. hostname_len {
            output += " ";
        }

        output += "   Solo Yield   Steps   Batch Yield\n";

        for (machine, chance_yield, steps, batch_yield) in targets.iter() {
            output += &format!(
                "{: <hnl$}   {:>10.2}   {:>5}   ${:>12.2}\n",
                machine.get_hostname(),
                chance_yield,
                steps,
                batch_yield,
                hnl = hostname_len
            );
        }

        ns.tprint(&output);

        let mut hacker_iter = hackers
            .into_iter()
            .map(|(h, t)| (Arc::new(h.clone()), t))
            .flat_map(|(h, t)| std::iter::repeat(h).take(t));

        // create the list of batches
        let mut batches = HashMap::new(); // TODO: preallocate
        let mut latest_id = 0;
        for (target, _, steps, _) in targets.into_iter() {
            let target = Arc::new(target.clone());

            for _ in 0 .. steps {
                // get the ID
                let id = latest_id;
                latest_id += 1;

                // get a hacker
                let hacker = hacker_iter.next().unwrap();

                let htp = HackerTargetPair {
                    hacker,
                    target: target.clone(),
                };

                batches.insert(id, htp);
            }
        }

        let mut batch_hacker = BatchHacker {
            events: BinaryHeap::new(),
            batches,
            missed_spawns: 0,
        };

        // spawn the events
        let now = Date::now();
        for id in 0 .. latest_id {
            batch_hacker.push_new_jobs_to_events(
                ns,
                // now, plus a second, plus twice the grace period multiplied by the offset
                now + 1000. + (2 * BATCH_GRACE_PERIOD_MILLIS * id) as f64,
                id,
                false,
            );
        }

        batch_hacker
    }
}
*/

fn get_potential_grow_amt(ns: &NsWrapper<'_>, machine: &Machine) -> usize {
    let money = machine.get_money_available(ns).min(1);
    let growth_factor = machine.get_max_money() as f64 / money as f64;

    ns.growth_analyze(machine.get_hostname(), growth_factor, None).ceil() as usize
}
