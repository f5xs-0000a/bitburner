import { Machine } from "machine_class.js";
import { get_network } from "scanner.js";
import { fractional_gcd } from "math.js";

////////////////////////////////////////////////////////////////////////////////

// the rate of RAM on the host machine that will be reserved for other uses
const RESERVATION_RATE = 0.1;

// hack_time multipliers to obtain their corresponding waiting times.
const GROW_TIME_MUL = 3.2;
const WEAKEN_TIME_MUL = 4;

////////////////////////////////////////////////////////////////////////////////

/// Disables logging of specific commands
function disable_logs(ns) {
    let noisy_methods = [
        "ALL",
        //"disableLog",
        //"grow",
        //"hack",
        //"weaken",
        //"scan"
    ];

    for (let method of noisy_methods) {
        ns.disableLog(method);
    }
}

async function wait_pid_with_time_hint(ns, pid, end_time, interval = 250) {
    ns.tprint("Waiting until " + end_time);
    await ns.sleep(Date.now() - end_time);
    while (ns.isRunning(pid)) {
        await ns.sleep(interval);
    }
}

////////////////////////////////////////////////////////////////////////////////

function tprint_help(ns) {
    ns.tprint("======== AUTOHACKER ============================================================");
    ns.tprint("Author: F5XS");
    ns.tprint("");
    ns.tprint("    Automatically and optimally hacks available networks and stripping them");
    ns.tprint("    out of their cash.");
    ns.tprint("");
    ns.tprint("Parameters:");
    ns.tprint("    --min-rate    Multiplier for hacking output to compare against the amount of");
    ns.tprint("                  money in the machine.");
    ns.tprint("                  If server money < min-rate * hack output, then the server will");
    ns.tprint("                  be grown.");
    ns.tprint("                  Must be less than max-rate.");
    ns.tprint("    --max-rate    Multiplier for hacking output to compare against the amount of");
    ns.tprint("                  money in the machine.");
    ns.tprint("                  If max-rate * hack output < server money, then the server will");
    ns.tprint("                  be hacked.");
    ns.tprint("                  Must be greater than min-rate.");
}

function print_header_bar(ns, text) {
    ns.print("\n======== " + text + " " + "=".repeat(Math.max(0, 70 - text.length)));
}

////////////////////////////////////////////////////////////////////////////////

class HackableMachine {
    constructor(
        machine,
    ) {
        this.machine = machine;
    }

    get_hostname() {
        return this.machine.get_hostname();
    }
    
    get_path() {
        return this.machine.get_path();
    }
    
    is_root() {
        return this.machine.is_root();
    }
    
    upget_root() {
        return this.machine.upget_root();
    }
    
    is_backdoored() {
        return this.machine.is_backdoored();
    }
    
    upget_backdoor() {
        return this.machine.upget_backdoor();
    }
    
    get_max_money() {
        return this.machine.get_max_money();
    }
    
    is_player_owned() {
        return this.machine.is_player_owned();
    }
    
    get_hacking_skill() {
        return this.machine.get_hacking_skill();
    }
    
    upget_hacking_skill() {
        return this.machine.upget_hacking_skill();
    }

    get_hack_chance(ns) {
        return ns.hackAnalyzeChance(this.get_hostname());
    }
    
    get_min_security() {
        return this.machine.get_min_security();
    }

    // NOTE: this stays here because hackAnalyze() is dependent on player level
    // TODO: upget function here. this method can change depending on player
    // level
    get_money_per_hack(ns) {
        return ns.hackAnalyze(this.get_hostname()) * this.get_max_money();
    }

    // Returns the number of threads required to reduce the security of a
    // machine to its minimum
    get_overflow_security_credits(ns) {
        const WEAKEN_SECURITY_REDUCTION = 0.05;
        ns.tprint("CUR SECURITY :" + ns.getServerSecurityLevel(this.get_hostname()));
        ns.tprint("MIN SECURITY :" + this.get_min_security());
        let offset_security = ns.getServerSecurityLevel(this.get_hostname())
            - this.get_min_security();
        ns.tprint("OFFSET :" + offset_security / 0.05);

        return Math.round(offset_security / WEAKEN_SECURITY_REDUCTION);
    }

    get_hack_time_ms(ns) {
        return ns.getHackTime(this.hostname);
    }

    get_weaken_time_ms(ns) {
        return this.get_hack_time(ns) * WEAKEN_TIME_MUL;
    }

    get_grow_time_ms(ns) {
        return this.get_hack_time(ns) * GROW_TIME_MUL;
    }

    get_hack_time(ns) {
        return this.get_hack_time_ms(ns) / 1000;
    }

    get_weaken_time(ns) {
        return this.get_weaken_time_ms(ns) / 1000;
    }

    get_grow_time(ns) {
        return this.get_grow_time_ms(ns) * WEAKEN_TIME_MUL;
    }

    // Returns te yield of this machine if the pot is unlimited and security
    // never rises
    get_base_yield(ns) {
        return this.get_money_per_hack() / (this.get_hack_time(ns) / 1000);
    }

    // Returns the average yield
    get_chance_corrected_yield(ns) {
        return this.base_yield(ns) * this.get_hack_chance(ns);
    }

    // Returns the average yield including grows based on historical grow-hack
    // cycle
    total_corrected_yield() {
        // account for the duration of grows
        let grow_factor = (this.grows + 1) * GROW_TIME_MUL;
        let hack_factor = (this.hacks + 1);
        let factor = hack_factor / grow_factor;

        return this.chance_corrected_yield() * factor;
    }

    // Returns a list of callback functions as well as their number of threads
    // and time to finish weakening. Useful for a governor-kind of loop.
    produce_weaken_threads(ns) {
        let threads = this.get_overflow_security_credits(ns);
        ns.tprint("OVERFLOW: " + this.get_overflow_security_credits(ns));
        let time_to_weaken = this.get_weaken_time_ms(ns);

        let do_weaken = function (ns, host_name, thread_count, target_hostname) {
            ns.print(
                "Weakening " +
                target_hostname +
                " with " +
                thread_count +
                " threads (" +
                time_to_weaken +
                ")."
            );

            return ns.exec(
                "./child_weaken.js",
                host_name,
                thread_count,
                target_hostname,
            );
        };

        return {
            "threads": threads,
            "weaken_time": time_to_weaken,
            "function": do_weaken
        };
    }
}

class HackGovernor {
    constructor(
        ns
    ) {
        this._networks = [];
        for (let machine of get_network(ns)) {
            if (machine.get_max_money() == 0) {
                continue;
            }

            if (machine.is_player_owned()) {
                continue;
            }

            this._networks.push(new HackableMachine(machine));
        }

        let weaken_mem = ns.getScriptRam("child_weaken.js");
        let hack_mem = ns.getScriptRam("child_hack.js");
        let grow_mem = ns.getScriptRam("child_grow.js");

        // instead of counting in free RAM, count in discrete credits instead
        this._credit_denominator = fractional_gcd(
            fractional_gcd(weaken_mem, hack_mem, 3),
            grow_mem
        );
        this._weaken_credits = Math.round(weaken_mem / this._credit_denominator);
        this._hack_credits = Math.round(hack_mem / this._credit_denominator);
        this._grow_credits = Math.round(grow_mem / this._credit_denominator);

        this._hostname = ns.getHostname();
    }

    get_hostname() {
        return this._hostname;
    }

    get_max_ram(ns) {
        return ns.getServerMaxRam(this.get_hostname());
    }

    get_free_ram(ns) {
        return this.get_max_ram(ns) - ns.getServerUsedRam(this.get_hostname());
    }

    to_credits(memory) {
        return Math.floor(memory / this._credit_denominator);
    }

    get_free_ram_credits(ns) {
        let actual_free_ram = this.get_free_ram(ns);
        let allocated_ram = actual_free_ram * (1 - RESERVATION_RATE);
        return this.to_credits(allocated_ram);
    }

    weaken_threads_available(ns) {
        return Math.floor(this.get_free_ram_credits(ns) / this._weaken_credits);
    }

    grow_threads_available(ns) {
        return Math.floor(this.get_free_ram_credits(ns) / this._grow_credits);
    }

    hack_threads_available(ns) {
        return Math.floor(this.get_free_ram_credits(ns) / this._hack_credits);
    }

    async weaken_machines(ns) {
        let calls = {};
        for (let machine of this._networks) {
            if (!machine.is_root()) {
                continue;
            }

            if (ns.getHackingLevel() < machine.get_hacking_skill()) {
                continue;
            }

            let weaken_threads = machine.produce_weaken_threads(ns);

            ns.tprint(weaken_threads);
            if (weaken_threads["threads"] == 0) {
                continue;
            }

            calls[machine.get_hostname()] = weaken_threads;
        }

        if (calls.length == 0) {
            // no machines to weaken.
            return;
        }
        
        // greedily process each threads. for a single machine, take as much of
        // the hardest to crack machines first.
        let ending_times = [];
        while (0 < Object.keys(calls).length) {
            // TODO: remove me
            ns.tprint("went here");
            await ns.sleep(1000);

            let available = this.weaken_threads_available(ns);

            // add a job
            if (0 < available) {
                // find the job that has the highest waiting time
                let highest_key = null;
                let longest_time = -Infinity;
                for (let hostname in calls) {
                    if (longest_time < calls[hostname]["weaken_time"]) {
                        highest_key = hostname;
                        longest_time = calls[hostname]["weaken_time"];
                    }
                }

                // run the job
                let threads = Math.min(
                    available,
                    calls[highest_key]["threads"]
                );

                ns.tprint("CHKT: " + calls[highest_key]["threads"]);
                ns.tprint("Usable threads: " + threads);

                let pid = calls[highest_key]["function"](
                    ns,
                    this.get_hostname(),
                    threads,
                    highest_key,
                );
                let end_time = calls[highest_key]["weaken_time"] + Date.now();

                if (pid == 0) {
                    throw new Error("Cannot run weakening thread.");
                }

                // subtract the remaining threads
                calls[highest_key]["threads"] -= threads;

                // remove from the pool if we've weakened it enough
                if (calls[highest_key]["threads"] == 0) {
                    delete calls[highest_key];
                }

                // record the PID, hostname (TODO), and the end time
                ending_times.push([pid, end_time]);
                ns.tprint("354 " + ending_times);
            }

            // wait until the next job ends
            else {
                // find when the latest script will end
                let next_end_index = 0;
                for (let i = 1; i < ending_times.length; i += 1) {
                    if (ending_times[i][1] < ending_times[next_end_index][1]) {
                        next_end_index = i;
                    }
                }

                let to_wait = ending_times.splice(next_end_index, 1);

                // wait until the next script ends
                ns.tprint(ending_times);
                ns.tprint(370);
                await wait_pid_with_time_hint(ns, to_wait[0], to_wait[1]);
            }
        }

        ns.tprint("ENDING TIMES: ")
        ns.tprint(ending_times);

        // wait until every job ends
        let last_end_index = 0;
        for (let i = 1; i < ending_times.length; i += 1) {
            if (ending_times[i][1] < ending_times[last_end_index][1]) {
                last_end_index = i;
            }
        }

        if (last_end_index != null) {
            let to_wait = ending_times.splice(last_end_index, 1)[0];
            ns.tprint(to_wait);
            ns.tprint(384);
            await wait_pid_with_time_hint(ns, to_wait[0], to_wait[1]);
        }
    }

    async hgw_sequence(ns) {
        // sort the networks by yield
        this._networks.sort(
            (a, b) => b.total_corrected_yield(ns) - a.total_corrected_yield(ns)
        );

        // percent of money that we're allowed to pillage
        // we're not allowed to remove the (1 - x)% of it. let the company have
        // its shares
        const HACKABLE_RATIO = 0.75;

        // if a grow cannot reach 100%, let this rate threshold be an acceptable
        // range to grow in to.
        const GROW_THRESHOLD = 0.90;

        let machine_stats = {};
        for (let machine of this._networks) {
            // get the maximum hack threads allowed per machine
            let hackable_pool = machine.get_max_money() * HACKABLE_RATIO;
            let threads = hackable_pool / machine.get_base_yield(ns);
            threads = Math.floor(threads);

            machine_stats[machine.get_hostname()] = {
                "max_hg_threads": threads,
                "hg_pid": 0,
                "hg_end": 0,
                "weaken_pid": 0,
                "weaken_end": 0,
                "was_hack": true,
            };
        }

        while (true) {
            // if there is enough memory to perform a single weaken(), we have
            // enough memory to do anything.
            if (0 < this.weaken_threads_available(ns)) {
                // iterate through the machines to see which needs to run a
                // command
                for (let machine in this._networks) {
                    // ignore certain machines
                    if (machine.is_root()) {
                        continue;
                    }

                    if (ns.getHackingLevel() < machine.get_hacking_skill()) {
                        continue;
                    }

                    // try weakening first
                    // if the weaken procedure for this machine is done
                    if (machine_stats[machine.get_hostname()]["weaken_end"] < Date.now()) {
                        // TODO: perform weaken
                        // in a single weaken() call, one can do four hack()s.
                        // that means we use a quarter of the `max_hg_threads`
                        // allocation
                        // there's probably an infinite sum pattern here that
                        // i'm not going to do, i'm sticking with 1/4.
                        let minimum_threads = Math.floor(
                            machine_stats[machine.get_hostname()]["max_hg_threads"] / 4
                        );

                        // given the current security of the machine, calculate
                        // how many threads of weaken() we have to do
                        let security_level = ns
                            .getServerSecurityLevel(machine.get_hostname());
                        let offset_security = security_level
                            - ns.get_min_security();
                        let offset_threads = Math.ceil(offset_security / 0.05);

                        // whichever's higher gets to be the # of threads
                        let threads = Math.max(
                            minimum_threads,
                            offset_threads,
                        );
                        // don't exceed the allowed # of threads
                        threads = Math.min(
                            threads,
                            machine_stats[machine.get_hostname()]["max_hg_threads"],
                            this.weaken_threads_available(),
                        );

                        // spawn the weaken threads and assign the PID
                        machine_stats[machine.get_hostname()]["weaken_pid"] = ns
                            .exec(
                                "./child_weaken.js",
                                this.get_hostname(),
                                threads,
                                machine.get_hostname(),
                            );
                        machine_stats[machine.get_hostname()]["weaken_end"] =
                            machine.get_hack_time() * WEAKEN_TIME_MUL + Date.now();

                        break;
                    }

                    if (machine_stats[machine.get_hostname()]["hg_end"] < Date.now()) {
                        let available_cash = ns.getServerMoneyAvailable(machine.hostname);
                        let minimum_pool = machine.get_max_money() * (1 - HACKABLE_RATIO);
                        let maximum_pool = machine.get_max_money() * GROW_THRESHOLD;

                        let should_grow = available_cash < minimum_pool;
                        let should_hack = maximum_pool < available_cash;

                        let do_hack = machine_stats[machine.get_hostname()]["was_hack"];

                        if (should_hack && !should_grow) {
                            do_hack = true;
                        }

                        else if (!should_hack && should_grow) {
                            do_hack = false;
                        }

                        // Jesse, we're gonna hack
                        if (do_hack) {
                            let hackable_amount = (available_cash - minimum_pool);
                            let threads = Math.ceil(
                                hackable_amount / machine.get_money_per_hack(),
                            );
                            // don't exceed the allowed # of threads
                            threads = Math.min(
                                threads,
                                machine_stats[machine.get_hostname()]["max_hg_threads"],
                                this.hack_threads_available(),
                            );

                            // run the program and assign PID
                            machine_stats[machine.get_hostname()]["hg_pid"] = ns
                                .exec(
                                    "./child_hack.js",
                                    this.get_hostname(),
                                    threads,
                                    machine.get_hostname(),
                                );
                            machine_stats[machine.get_hostname()]["hg_end"] =
                                machine.get_hack_time() + Date.now();

                            machine_stats[machine.get_hostname()]["was_hack"] = true;
                        }

                        else {
                            // determine how much to grow
                            let threads = ns.growthAnalyze(
                                machine.get_hostname(),
                                machine.get_max_money() / available_money,
                            );
                            if (isNaN(threads)) {
                                threads = Infinity;
                            }
                            threads = Math.floor(threads);
                            // don't exceed the allowed # of threads
                            threads = Math.min(
                                threads,
                                machine_stats[machine.get_hostname()]["max_hg_threads"],
                                this.hack_threads_available(),
                            );

                            // run the program and assign PID
                            machine_stats[machine.get_hostname()]["hg_pid"] = ns
                                .exec(
                                    "./child_grow.js",
                                    this.get_hostname(),
                                    threads,
                                    machine.get_hostname(),
                                );
                            machine_stats[machine.get_hostname()]["hg_end"] =
                                machine.get_hack_time() * GROW_TIME_MUL + Date.now();

                            machine_stats[machine.get_hostname()]["was_hack"] = true;
                        }

                        break;
                    }
                }
            }

            // wait for any one to finish
            else {
                // TODO: loop over this and remove any lingering processes
                let next_pid = 0;
                let next_end = 0;
                let cur_machine = "";

                for (let machine in this._networks) {
                    if (next_end < machine_stats[machine.get_hostname()]["hg_end"]) {
                        next_end = machine_stats[machine.get_hostname()]["hg_end"];
                        next_pid = machine_stats[machine.get_hostname()]["hg_pid"];
                    }

                    if (next_end < machine_stats[machine.get_hostname()]["weaken_end"]) {
                        next_end = machine_stats[machine.get_hostname()]["weaken_end"];
                        next_pid = machine_stats[machine.get_hostname()]["weaken_pid"];
                    }
                }

                await wait_pid_with_time_hint(ns, next_pid, next_end);
            }
        }
    }
}

////////////////////////////////////////////////////////////////////////////////

export async function main(ns) {
    let flags = ns.flags([
        ["help", false],
    ]);

    if (flags.help) {
        tprint_help(ns);
        return;
    }

    disable_logs(ns);
    let governor = new HackGovernor(ns);
    
    governor._networks = governor._networks.filter((machine) => machine.get_hacking_skill() < 10);
    //ns.tprint(governor._networks);
    //return;
    
    ns.tprint("Weakening machines...");
    await ns.sleep(1000);
    await governor.weaken_machines(ns);
}
