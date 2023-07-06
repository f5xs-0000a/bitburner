use std::{
    collections::{
        BinaryHeap,
        HashMap,
        VecDeque,
    },
    fmt::Write as _,
    sync::{
        Arc,
        Mutex,
    },
};

use chrono::{
    format::StrftimeItems,
    NaiveDateTime,
};
use smallvec::SmallVec;

use crate::{
    event_pool::{
        Event,
        EventLoop,
        EventLoopContext,
        EventLoopState,
        EventWrapper,
    },
    machine::{
        get_machines,
        Machine,
    },
    netscript::{
        Date,
        NsWrapper,
    },
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

const RESERVATION_RATE: f64 = 0.9;

pub async fn auto_hack(ns: &NsWrapper<'_>) {
    // disable logging. at all.
    ns.disable_log("ALL");

    let mut ahg = EventLoop::new(AutoHackGovernor::new(ns));
    ahg.run(ns).await;
}

#[derive(Debug)]
enum AutoHackEventType {
    PollTarget(u64),
    MemoryFreed,
    GeneralPoll,
}

#[derive(Debug, Eq, PartialEq)]
enum MemoryFreeUsage {
    NoMemory,
    NotRequired,
    MemoryAllocated,
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
        target_hash: u64,
    ) -> AutoHackEventWrapped {
        AutoHackEventWrapped {
            trigger_time,
            grace_period,
            event_type: AutoHackEventType::PollTarget(target_hash),
        }
    }

    pub fn new_memory_freed(
        trigger_time: f64,
        grace_period: f64,
    ) -> AutoHackEventWrapped {
        AutoHackEventWrapped {
            trigger_time,
            grace_period,
            event_type: AutoHackEventType::MemoryFreed,
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

impl TargetState {
    fn is_total_weaken(&self) -> bool {
        matches!(self, TargetState::TotalWeaken(_))
    }

    fn is_max_grow(&self) -> bool {
        matches!(self, TargetState::MaxGrow)
    }

    fn is_hack(&self) -> bool {
        matches!(self, TargetState::Hack)
    }
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
        // if we've obtained all the machines that we could need to
        // weaken this, proceed
        if hgw_threads == 0 {
            break;
        }

        let available_threads =
            (hacker.get_threads_left(ns) as usize).min(hgw_threads);

        // if this machine does not have any available threads left,
        // skip this machine
        if available_threads == 0 {
            continue;
        }

        hgw_threads -= available_threads;

        available_hackers.push((hacker.clone(), available_threads));
    }

    // if we asked for a full split and we didn't get a full split
    if split == FullSplit && hgw_threads != 0 {
        return None;
    }

    Some(available_hackers)
}

#[derive(Debug)]
struct TargetStateBundle {
    machine: Machine,

    state: TargetState,
    is_waiting_for_memory: bool,

    // latest spawned at the front
    // earliest spawned at the back
    // first element is the spawn time, not the finish time
    // TODO: we need a way to clear this
    running_pids: VecDeque<(f64, SmallVec<[RunningProcessMetadata; 4]>)>,

    last_poll: f64,
}

impl TargetStateBundle {
    fn write_diagnostics<W>(
        &self,
        ns: &NsWrapper<'_>,
        writable: &mut W,
    ) -> Result<(), std::fmt::Error>
    where
        W: core::fmt::Write,
    {
        use crate::autohack::TargetState::*;

        let state_hint = match &self.state {
            TotalWeaken(_) => "TotalWeaken",
            MaxGrow => "MaxGrow",
            Hack => "Hack",
        };

        let waiting_hint = match self.is_waiting_for_memory {
            true => "W",
            false => " ",
        };

        let last_poll = self.last_poll as i64;
        let strftime =
            NaiveDateTime::from_timestamp_millis(last_poll).map(|ndt| {
                ndt.format_with_items(StrftimeItems::new("%H:%M:%S%.3f"))
            });

        let money_available = self.machine.get_money_available(ns);

        write!(
            writable,
            "| {:<20} | {:^11} | {} | ",
            self.machine.get_hostname(),
            state_hint,
            waiting_hint,
        )?;

        match strftime {
            Some(st) => write!(writable, "{}", st)?,
            None => write!(writable, "Never polled")?,
        };

        write!(
            writable,
            " | {: >6.4}% | +{: >6.3}% |",
            money_available as f64 / self.machine.get_max_money() as f64 * 100.,
            self.machine.get_security_level(ns)
                - self.machine.get_min_security(),
        )
    }

    fn get_hash(&self) -> u64 {
        get_machine_hash(&self.machine)
    }

    fn new(
        ns: &NsWrapper<'_>,
        machine: Machine,
    ) -> TargetStateBundle {
        let weakens_required = machine.get_weaken_threads_to_reduce(ns);

        TargetStateBundle {
            machine,
            state: TargetState::TotalWeaken(weakens_required),
            is_waiting_for_memory: false,
            running_pids: Default::default(),
            last_poll: f64::MIN,
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
                    &[self.machine.get_hostname(), &sleep_time_str],
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
        let hack_time = self.machine.get_hack_time(ns); // TODO: use HGW time

        self.last_poll = now;

        match self.state.clone() {
            TotalWeaken(weakens_left) => {
                if weakens_left == 0 {
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

                ctx.add_event(AutoHackEventWrapped::new_memory_freed(
                    now + hack_time * 4. + 5.,
                    50.,
                ));

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
                    self.get_hash(),
                ));

                // update the state
                self.state = TotalWeaken(new_weakens_left);
            },

            MaxGrow => {
                // calculate how many grow and weakens we need to do
                let mut grows_required =
                    get_potential_grow_amt(ns, &self.machine);

                if grows_required == 0 {
                    self.state = Hack;
                    self.on_poll(ns, ctx, govr);

                    return;
                }

                let mut new_pids = SmallVec::new();

                // attempt to spawn multiple grows
                // if it failed, keep halving the amount of grows done
                // if we got less than 12, fail.
                loop {
                    let weakens_required =
                        rational_mult_usize(grows_required, 12.5f64.recip())
                            .max(1);

                    macro_rules! on_failure {
                        () => {{
                            kill_all(ns, new_pids.drain(..));
                            grows_required /= 2;

                            if grows_required == 0 {
                                self.on_no_memory();
                                return;
                            }

                            continue;
                        }};
                    }

                    // spawn the grow half
                    match self.spawn_hgw(
                        ns,
                        HGW::Grow,
                        govr.get_hackers_iter(),
                        now,
                        now + (4. - 3.2) * hack_time - MILLISECOND * 50.,
                        grows_required,
                        NoSplit, // NEVER split grows.
                    ) {
                        Some(m) => new_pids.extend(m.into_iter()),
                        None => on_failure!(),
                    };

                    // spawn the weaken half
                    match self.spawn_hgw(
                        ns,
                        HGW::Weaken,
                        govr.get_hackers_iter(),
                        now,
                        now,
                        weakens_required,
                        PartialSplit,
                    ) {
                        Some(m) => new_pids.extend(m.into_iter()),
                        None => on_failure!(),
                    };

                    // if we've reached this point, we've successfully grown
                    // the machine
                    break;
                }

                ctx.add_event(AutoHackEventWrapped::new_memory_freed(
                    now + hack_time * 4. + 5.,
                    50.,
                ));

                self.running_pids.push_front((now, new_pids));

                ctx.add_event(AutoHackEventWrapped::new_poll_target(
                    // TODO: there should be a proper place where you get the
                    // grace period
                    now + hack_time * 4. + MILLISECOND * 50.,
                    MILLISECOND * 50.,
                    self.get_hash(),
                ));
            },

            Hack => {
                let mut new_pids = SmallVec::new();

                let hack_time = self.machine.get_hack_time(ns);

                // spawn the hack
                match self.spawn_hgw(
                    ns,
                    HGW::Hack,
                    govr.get_hackers_iter(),
                    now,
                    now + (4. - 1.) * hack_time - 50.,
                    1,
                    PartialSplit,
                ) {
                    Some(pids) => new_pids.extend(pids.into_iter()),
                    None => {
                        kill_all(ns, new_pids.into_iter());
                        self.on_no_memory();
                        return;
                    },
                };

                // spawn the first weaken
                match self.spawn_hgw(
                    ns,
                    HGW::Weaken,
                    govr.get_hackers_iter(),
                    now,
                    now,
                    1,
                    PartialSplit,
                ) {
                    Some(pids) => new_pids.extend(pids.into_iter()),
                    None => {
                        kill_all(ns, new_pids.into_iter());
                        self.on_no_memory();
                        return;
                    },
                };

                // spawn the grow
                match self.spawn_hgw(
                    ns,
                    HGW::Grow,
                    govr.get_hackers_iter(),
                    now,
                    now + (4. - 3.2) * hack_time + 50.,
                    1,
                    PartialSplit,
                ) {
                    Some(pids) => new_pids.extend(pids.into_iter()),
                    None => {
                        kill_all(ns, new_pids.into_iter());
                        self.on_no_memory();
                        return;
                    },
                };

                // spawn the second weaken
                match self.spawn_hgw(
                    ns,
                    HGW::Weaken,
                    govr.get_hackers_iter(),
                    now,
                    now + 50. * 2.,
                    1,
                    PartialSplit,
                ) {
                    Some(pids) => new_pids.extend(pids.into_iter()),
                    None => {
                        kill_all(ns, new_pids.into_iter());
                        self.on_no_memory();
                        return;
                    },
                };

                ctx.add_event(AutoHackEventWrapped::new_memory_freed(
                    now + hack_time * 4. + 5. + 50. * 2.,
                    50.,
                ));

                self.running_pids.push_front((now, new_pids));

                // spawn another that will hach this machine again
                ctx.add_event(AutoHackEventWrapped::new_poll_target(
                    now + 4. + MILLISECOND * 50.,
                    MILLISECOND * 50.,
                    self.get_hash(),
                ));
            },
        }
    }

    fn on_no_memory(&mut self) {
        self.is_waiting_for_memory = true;
    }

    fn on_memory_freed(
        &mut self,
        ns: &NsWrapper<'_>,
        ctx: &mut EventLoopContext<AutoHackEventWrapped>,
        govr: &mut AutoHackGovernor,
    ) -> MemoryFreeUsage {
        use MemoryFreeUsage::*;

        if self.is_waiting_for_memory {
            // set is_waiting_for_memory to be false. it will be set true if we
            // tried to spawn and yet nothing happened
            self.is_waiting_for_memory = false;

            self.on_poll(ns, ctx, govr);

            // is_waiting_for_memory will be true if there is still no
            // memory even if we did poll() so use that to check.
            if self.is_waiting_for_memory {
                NoMemory
            }
            else {
                MemoryAllocated
            }
        }
        else {
            NotRequired
        }
    }
}

#[derive(Debug)]
struct AutoHackGovernor {
    hackers: VecDeque<Arc<Machine>>,
    targets_by_name: HashMap<u64, TargetStateBundle>,
    targets_by_score: Vec<u64>,

    hacking_level: usize,
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

    /// Returns a list of hostname hashes currently used as a hacker and a
    /// target.
    fn get_used_hostname_hashes(
        &self,
        mut buffer: Vec<u64>,
    ) -> Vec<u64> {
        buffer.clear();

        let iter_1 = self.targets_by_name.keys().cloned();
        let iter_2 = self.hackers.iter().map(|h| get_machine_hash(h));

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
        buffer_1: Vec<u64>,
    ) -> Vec<(u64, Machine)> {
        let used_hostnames = self.get_used_hostname_hashes(buffer_1);

        let hacking_level = ns.get_player_hacking_level();

        get_machines(ns)
            .into_iter()
            .filter_map(|mut m| match crate::scan::nuke_machine(ns, &mut m) {
                crate::scan::NukeResult::NotNuked => None,
                _ => Some(m)
            })
            //.filter(|m| m.get_min_hacking_skill() <= hacking_level)
            .map(|m| (get_machine_hash(&m), m))
            // don't allow machines that already exist in hackers and targets
            // so we don't consume ns function runtime
            .filter(|(h, _)| !used_hostnames
                .iter()
                .any(|uhn| *uhn == *h)
            )
            // the machines must be rooted
            // TODO: just root it ourselves
            .filter(|(_, m)| m.is_root(ns))
            .collect::<Vec<_>>()
    }

    /// Obtains new hackers from a list of machines.
    fn get_new_hackers_from(
        &mut self,
        ns: &NsWrapper<'_>,
        machines: &[(u64, Machine)],
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
            .cloned()
            .map(|m| Arc::new(m));
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
        machines: &[(u64, Machine)],
        buffer: &mut Vec<(u64, TargetStateBundle)>,
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
            .map(|(hn, m)| (hn, TargetStateBundle::new(ns, m)));
        buffer.extend(iter);

        // if there is nothing inside the buffer, exit
        if buffer.is_empty() {
            return;
        }

        // add the keys into the score
        self.targets_by_score
            .extend(buffer.iter().map(|(k, _)| k).cloned());

        // then move everything back into the hash map
        self.targets_by_name.extend(buffer.drain(..));

        // then sort
        self.resort_targets_by_score(ns);
    }

    fn resort_targets_by_score(
        &mut self,
        ns: &NsWrapper<'_>,
    ) {
        self.targets_by_score.sort_by_cached_key(|key| {
            let avg_yld = self
                .targets_by_name
                .get(key)
                .unwrap()
                .machine
                .get_average_yield(ns);

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

    fn do_level_up_check(
        &mut self,
        ns: &NsWrapper<'_>,
    ) {
        let level = ns.get_player_hacking_level();

        if level == self.hacking_level {
            return;
        }

        // if we've levelled up, do many things

        // only regenerate hackers and targets upon level up
        self.hacking_level = level;
        self.regenerate_hackers_and_targets(ns);

        // set everything back to total weaken
        for key in self.targets_by_score.iter() {
            let target = self.targets_by_name.get_mut(key).unwrap();

            // don't interrupt an already weakening process with another one
            if target.state.is_total_weaken() {
                continue;
            }

            target.state = TargetState::TotalWeaken(
                target.machine.get_weaken_threads_to_reduce(ns),
            );
        }

        // resort targets by score
        self.resort_targets_by_score(ns);
    }

    fn do_diagnostics(
        &self,
        ns: &NsWrapper<'_>,
    ) {
        let mut printable = String::new();

        for key in self.targets_by_score.iter() {
            let target = self.targets_by_name.get(key).unwrap();

            target.write_diagnostics(ns, &mut printable).unwrap();
            printable += "\n";
        }

        let now = Date::now();
        let strftime = NaiveDateTime::from_timestamp_millis(now as i64)
            .map(|ndt| {
                ndt.format_with_items(StrftimeItems::new("%H:%M:%S%.3f"))
            })
            .unwrap();

        write!(&mut printable, "Current time: {}", strftime);

        ns.clear_log();
        ns.print(&printable);
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
            PollTarget(key) => {
                // take it out, do poll stuff on it, then put it back
                let mut target = self.targets_by_name.remove(&key).unwrap();
                target.on_poll(ns, ctx, self);
                self.targets_by_name.insert(key, target);
            },

            MemoryFreed => {
                // TODO: this is an expensive clone.
                for key in self.targets_by_score.clone().into_iter() {
                    let mut target = self.targets_by_name.remove(&key).unwrap();

                    let free_result = target.on_memory_freed(ns, ctx, self);
                    self.targets_by_name.insert(key, target);

                    // if we finally have no memory left, break away
                    if free_result == MemoryFreeUsage::NoMemory {
                        break;
                    }
                }
            },

            GeneralPoll => {
                self.do_level_up_check(ns);
                self.do_diagnostics(ns);

                // spawn another general poll request
                ctx.add_event(AutoHackEventWrapped::new_general_poll(
                    Date::now() + SECOND,
                    MILLISECOND * 50.,
                ));
            },
        }
    }

    fn on_event_fail<'a>(
        &mut self,
        ns: &NsWrapper<'a>,
        event: Self::Event,
        ctx: &mut EventLoopContext<Self::Event>,
    ) {
        // as of right now, we have no reasons to deny a poll. it's adaptive
        // enough on its own.
        self.on_event(ns, event, ctx);
    }

    fn post_loop_inspect<'a>(
        &self,
        ns: &NsWrapper<'a>,
        event_heap: &BinaryHeap<EventWrapper<Self::Event>>,
    ) {
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

fn get_potential_grow_amt(
    ns: &NsWrapper<'_>,
    machine: &Machine,
) -> usize {
    let money = machine.get_money_available(ns).max(1);
    let growth_factor = machine.get_max_money() as f64 / money as f64;

    ns.growth_analyze(machine.get_hostname(), growth_factor, None)
        .ceil() as usize
}

fn kill_all(
    ns: &NsWrapper<'_>,
    iter: impl Iterator<Item = RunningProcessMetadata>,
) {
    for process in iter {
        ns.kill(process.pid as i32);
    }
}

fn get_machine_hash(machine: &Machine) -> u64 {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::Hasher as _,
    };

    let mut hasher = DefaultHasher::new();

    hasher.write(machine.get_hostname().as_bytes());
    hasher.finish()
}
