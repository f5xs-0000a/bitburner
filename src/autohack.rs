use core::cmp::Ordering;

use binary_heap_plus::{
    BinaryHeap,
    MinComparator,
};
use decorum::R64;

use crate::{
    machine::Machine,
    netscript::{
        Date,
        NsWrapper,
    },
};

const GRACE_PERIOD_MILLIS: usize = 10;
const RESERVATION_RATE: f64 = 0.9;

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct NSTime(R64);

impl NSTime {
    pub fn time_until(&self) -> NSTime {
        NSTime(R64::from_inner(Date::now() - self.0.into_inner()))
    }

    pub async fn sleep_until(
        &self,
        ns: &NsWrapper<'_>,
    ) {
        ns.sleep(self.time_until().0.into_inner().round() as i32)
            .await;
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
    end_time: NSTime,
}

impl PartialOrd for HGWJob {
    fn partial_cmp(
        &self,
        other: &Self,
    ) -> Option<Ordering> {
        self.end_time.0.partial_cmp(&other.end_time.0)
    }
}

impl Ord for HGWJob {
    fn cmp(
        &self,
        other: &Self,
    ) -> Ordering {
        self.end_time.0.cmp(&other.end_time.0)
    }
}

impl PartialEq for HGWJob {
    fn eq(
        &self,
        other: &Self,
    ) -> bool {
        self.end_time.0.eq(&other.end_time.0)
    }
}

impl Eq for HGWJob {
    // forbidden magic
    fn assert_receiver_is_total_eq(&self) {}
}

impl HGWJob {
    fn spawn_common(
        ns: &NsWrapper,
        hostname: &str,
        threads: usize,
        target: &str,
        command: &str,
    ) -> Option<usize> {
        assert!(0 < threads);
        let script_name = ns.get_script_name();
        ns.exec(&*script_name, hostname, Some(threads), &[command, target])
    }

    pub fn grow(
        ns: &NsWrapper,
        hacker: &Machine,
        target: &Machine,
        threads: usize,
    ) -> HGWJob {
        let pid = HGWJob::spawn_common(
            ns,
            hacker.get_hostname(),
            threads,
            target.get_hostname(),
            "grow",
        )
        .unwrap();

        HGWJob {
            job_type: JobType::Grow,
            running_machine: hacker.get_hostname().to_owned(),
            pid,
            end_time: NSTime(R64::from_inner(target.get_grow_time(ns))),
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
            hacker.get_hostname(),
            threads,
            target.get_hostname(),
            "weaken",
        )
        .unwrap();

        HGWJob {
            job_type: JobType::Weaken,
            running_machine: hacker.get_hostname().to_owned(),
            pid,
            end_time: NSTime(R64::from_inner(target.get_weaken_time(ns))),
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
            hacker.get_hostname(),
            threads,
            target.get_hostname(),
            "hack",
        )
        .unwrap();

        HGWJob {
            job_type: JobType::Hack,
            running_machine: hacker.get_hostname().to_owned(),
            pid,
            end_time: NSTime(R64::from_inner(target.get_hack_time(ns))),
        }
    }

    pub fn get_job_type(&self) -> JobType {
        self.job_type
    }

    pub fn get_pid(&self) -> usize {
        self.pid
    }

    pub fn get_end_time(&self) -> NSTime {
        self.end_time
    }

    pub async fn wait_until_end(
        &self,
        ns: &NsWrapper<'_>,
    ) {
        self.end_time.sleep_until(ns).await;

        while ns.is_running(self.get_pid()) {
            ns.sleep(GRACE_PERIOD_MILLIS as i32).await;
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
    pub fn new(ns: &NsWrapper) -> TotalWeakener {
        let machines = crate::machine::get_machines(ns);

        let hackers = machines
            .iter()
            .filter(|m| m.is_root(ns))
            // we're going to be using just home for now
            // TODO: remove this
            .filter(|m| m.get_hostname() == "home")
            .cloned()
            .map(|m| (m, BinaryHeap::new_min()))
            .collect::<Vec<_>>();

        let mut targets = machines
            .into_iter()
            // hack only machines that are
            // not owned by the player
            .filter(|m| !m.is_player_owned())
            // rooted
            .filter(|m| m.is_root(ns))
            // can be filled with money
            .filter(|m| 0 < m.get_max_money())
            .map(|m| {
                let threads = m.get_weaken_threads_to_reduce(ns);
                (m, threads)
            })
            // and needs to be weakened
            .filter(|(_, count)| 0 < *count)
            .collect::<Vec<_>>();
        targets.sort_unstable_by(|(m1, _), (m2, _)| {
            m1.get_weaken_time(ns)
                .partial_cmp(&m2.get_weaken_time(ns))
                .unwrap()
        });

        TotalWeakener {
            hackers,
            targets,
            current_hacker_index: 0,
            current_target_index: 0,
        }
    }

    pub async fn run(
        &mut self,
        ns: &NsWrapper<'_>,
    ) {
        if self.targets.is_empty() {
            return;
        }

        // the progression goes from left to right
        let mut last_successful_hacker_idx = Some(self.current_hacker_index);
        while 0 < self.targets.last().unwrap().1 {
            let cur_hacker_idx = self.current_hacker_index;

            // make sure to record the hacker's index that spawned a weaken
            // process successfully
            if self.step(ns).await {
                last_successful_hacker_idx = Some(cur_hacker_idx);
            }
            // at this point, step() has incremented the current hacker index.
            // that means that this current hacker index is not the same when
            // we're comparing.
            else if last_successful_hacker_idx
                == Some(self.current_hacker_index)
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
    ) -> bool {
        let (current_hacker, ref mut current_jobs) =
            self.hackers.get_mut(self.current_hacker_index).unwrap();
        let (current_target, ref mut current_threads_remaining) =
            self.targets.get_mut(self.current_target_index).unwrap();

        // increment the pointer if there are no threads required for this
        // machine
        if *current_threads_remaining == 0 {
            self.current_target_index += 1;
            return false;
        }

        // not enough RAM on this machine. get to the next one.
        let max_ram = {
            let mut mr = current_hacker.get_max_gb_ram_hundredths(ns);
            mr *= (RESERVATION_RATE * 1000.) as usize;
            mr /= 1000;
            mr
        };

        if current_hacker.get_free_ram_hundredths(ns) < max_ram {
            self.current_hacker_index =
                (self.current_hacker_index + 1) % self.hackers.len();
            return false;
        }

        // make sure that the number of threads to use
        // - still follows the reservation rate, to an extent
        // - is not larger than weaken_threads_left
        // - is at least 1
        let mut threads_to_use = current_target.get_threads_left(ns);
        threads_to_use *= (RESERVATION_RATE * 1000.) as usize;
        threads_to_use /= 1000;
        threads_to_use = threads_to_use.min(*current_threads_remaining);
        threads_to_use = threads_to_use.max(1);

        // perform hacking
        let job =
            HGWJob::weaken(ns, current_hacker, current_target, threads_to_use);
        current_jobs.push(job);

        *current_threads_remaining -= threads_to_use;

        // spawning happened successfully
        true
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

            if let Some((best_idx, best_jobs)) = best.as_mut() {
                let this_job_end_time = jobs.peek().unwrap().get_end_time().0;
                let best_job_end_time =
                    best_jobs.peek().unwrap().get_end_time().0;

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

pub async fn auto_hack(ns: &NsWrapper<'_>) {
    let mut weakener = TotalWeakener::new(ns);
    ns.tprint("Weakening machines...");
    weakener.run(ns).await;
    ns.tprint("Machines weakened.");
}
