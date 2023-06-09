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
    await ns.sleep(end_time);
    while (ns.isRunning(pid)) {
        await ns.sleep(250);
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
    get_money_per_hack() {
        return ns.hackAnalyze(this.get_hostname()) * this.get_max_money();
    }

    // Returns the number of threads required to reduce the security of a
    // machine to its minimum
    get_overflow_security_credits(ns) {
        let offset_security = ns.getServerSecurityLevel(this.hostname)
            - this.get_min_security();

        return Math.round(offset_security / 0.05);
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
        let time_to_weaken = this.get_weaken_time_ms(ns);
        let uuid = 

        let do_weaken = function (ns, host_name, thread_count) {
            ns.print(
                "Weakening " +
                this.hostname +
                " with " +
                thread_count +
                " threads (" +
                time_to_weaken / 1000 +
                ")."
            );

            return ns.exec(
                "./child_weaken.js",
                host_name
                threads,
                this.hostname,
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
        for (let network of get_network()) {
            this.networks.push(HackableMachine(network));
        }

        let weaken_mem = ns.getScriptRam("child_weaken.js");
        let hack_mem = ns.getScriptRam("child_hack.js");
        let grow_mem = ns.getScriptRam("child_grow.js");

        // instead of counting in free RAM, count in discrete credits instead
        this._credit_denominator = fractional_gcd(
            fractional_gcd(weaken_mem, hack_mem),
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
        return ns.getServerMaxRam(this.hostname);
    }

    get_free_ram(ns) {
        return ns.getServerUsedRam(this.hostname) - this.get_max_ram(ns);
    }

    to_credits(memory) {
        return Math.floor(memory / this.credit_denominator);
    }

    get_free_ram_credits(ns) {
        let actual_free_ram = this.get_free_ram(ns);
        let allocated_ram = actual_ram * (1 - RESERVATION_RATE);
        return this.to_credits(allocated_ram);
    }

    weaken_threads_available(ns) {
        get_free_ram_credits(ns) / this.weaken_credits;
    }

    grow_threads_available(ns) {
        get_free_ram_credits(ns) / this.grow_credits;
    }

    hack_threads_available(ns) {
        get_free_ram_credits(ns) / this.hack_credits;
    }

    weaken_machines(ns) {
        let calls = {};
        for (let machine in this._networks) {
            calls[machine.get_hostname()] = machine.produce_weaken_threads(ns);
        }

        if (calls.length == 0) {
            // no machines to weaken.
            return;
        }

        // greedily process each threads. for a single machine, take as much of
        // the hardest to crack machines first.
        let ending_times = [];
        while (0 < calls.keys()) {
            let available = weaken_threads_available();

            // add a job
            if (0 < available) {
                // find the job that has the highest waiting time
                let highest_key = null;
                let longest_time = -Infinity;
                for (let [hostname, deets] of calls) {
                    if (longest_time < deets["weaken_time"]) {
                        highest_key = hostname;
                        longest_time = deets["weaken_time"]);
                    }
                }

                // run the job
                let threads = Math.min(
                    available,
                    calls[highest_key]["threads"]
                );

                let pid = calls[highest_key]["function"](
                    ns,
                    this.get_hostname(),
                    threads,
                );
                let end_time = Date.now() + calls[highest_key]["weaken_time"] + Date.now();

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
                await wait_pid_with_time_hint(ns, to_wait[0], to_wait[1]);
            }
        }

        // wait until every job ends
        let last_end_index = 0;
        for (let i = 1; i < ending_times.length; i += 1) {
            if (ending_times[i][1] < ending_times[next_end_index][1]) {
                next_end_index = i;
            }
        }

        let to_wait = ending_times.splice(next_end_index, 1);
        await wait_pid_with_time_hint(ns, to_wait[0], to_wait[1]);
    }

    hgw_sequence(ns) {
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

            network_stats[machine.get_hostname()] = {
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
            if (0 < weaken_threads_available()) {
                // iterate through the machines to see which needs to run a
                // command
                for (let machine in this._networks) {
                    // try weakening first
                    // if the weaken procedure for this machine is done
                    if (network_stats[machine.get_hostname()]["weaken_end"] < Date.now()) {
                        // TODO: perform weaken
                        // in a single weaken() call, one can do four hack()s.
                        // that means we use a quarter of the `max_hg_threads`
                        // allocation
                        // there's probably an infinite sum pattern here that
                        // i'm not going to do, i'm sticking with 1/4.
                        let minimum_threads = Math.floor(
                            network_stats[machine.get_hostname()]["max_hg_threads"] / 4
                        );

                        // given the current security of the machine, calculate
                        // how many threads of weaken() we have to do
                        let security_level = = ns
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
                            network_stats[machine.get_hostname()]["max_hg_threads"],
                            this.weaken_threads_available(),
                        );

                        // spawn the weaken threads and assign the PID
                        network_stats[machine.get_hostname()]["weaken_pid"] = ns
                            .exec(
                                "./child_weaken.js",
                                this.get_hostname(),
                                threads,
                                machine.get_hostname(),
                            );
                        network_stats[machine.get_hostname()]["weaken_end"] =
                            machine.get_hack_time() * WEAKEN_TIME_MUL + Date.now();

                        break;
                    }

                    if (network_stats[network.get_hostname()]["hg_end"] < Date.now()) {
                        let available_cash = ns.getServerMoneyAvailable(machine.hostname);
                        let minimum_pool = ns.get_max_money() * (1 - HACKABLE_RATIO);
                        let maximum_pool = ns.get_max_money() * GROW_THRESHOLD;

                        let should_grow = available_cash < minimum_pool;
                        let should_hack = maximum_pool < available_cash;

                        let do_hack = network_stats[machine.get_hostname()]["was_hack"];

                        if (should_hack && !should_grow) {
                            do_hack = true;
                        }

                        else if (!should_hack && should_grow {
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
                                network_stats[machine.get_hostname()]["max_hg_threads"],
                                this.hack_threads_available(),
                            );

                            // run the program and assign PID
                            network_stats[machine.get_hostname()]["hg_pid"] = ns
                                .exec(
                                    "./child_hack.js",
                                    this.get_hostname(),
                                    threads,
                                    machine.get_hostname(),
                                );
                            network_stats[machine.get_hostname()]["hg_end"] =
                                machine.get_hack_time() + Date.now();

                            network_stats[machine.get_hostname()]["was_hack"] = true;
                        }

                        else {
                            // determine how much to grow
                            let threads = ns.growthAnalyze(
                                machine.get_hostname()
                                machine.get_max_money() / available_money,
                            );
                            threads = Math.floor(threads);
                            // don't exceed the allowed # of threads
                            threads = Math.min(
                                threads,
                                network_stats[machine.get_hostname()]["max_hg_threads"],
                                this.hack_threads_available(),
                            );

                            // run the program and assign PID
                            network_stats[machine.get_hostname()]["hg_pid"] = ns
                                .exec(
                                    "./child_grow.js",
                                    this.get_hostname(),
                                    threads,
                                    machine.get_hostname(),
                                );
                            network_stats[machine.get_hostname()]["hg_end"] =
                                machine.get_hack_time() * GROW_TIME_MUL + Date.now();

                            network_stats[machine.get_hostname()]["was_hack"] = true;
                        }

                        break;
                    }
                }
            }

            // wait for any one to finish
            else {
                let next_pid = 0;
                let next_end = 0;
                let cur_machine = "";

                for (let machine in this._networks) {
                    if (next_end < network_stats[machine.get_hostname()]["hg_end"]) {
                        next_end = network_stats[machine.get_hostname()]["hg_end"];
                        next_pid = network_stats[machine.get_hostname()]["hg_pid"];
                    }

                    if (next_end < network_stats[machine.get_hostname()]["weaken_end"]) {
                        next_end = network_stats[machine.get_hostname()]["weaken_end"];
                        next_pid = network_stats[machine.get_hostname()]["weaken_pid"];
                    }
                }

                await wait_pid_with_time_hint(ns, next_pid, next_end);
            }
        }
    }
}
        /*
        this.hostname = machine.hostname;
        this.parent_host = machine.parent_host;
        this.degree = machine.degree;
        this.is_root = machine.is_root;
        this.is_backdoored = machine.is_backdoored;
        this.max_money = machine.max_money;
        this.player_owned = machine.player_owned;
        this.hacking_skill = machine.hacking_skill;
        this.min_security = machine.min_security;

        machine.hack_level_recorded = player_hacking_level;
        this.update_stats(ns);

        this.security_level = ns.getServerSecurityLevel(this.hostname);

        this.hacks = 0;
        this.grows = 0;
        */

    /*
    // Updates the statistics of the target machine.
    //
    // Recommended to be called upon hacking level up.
    update_stats(ns) {
        let hack_rate = ns.hackAnalyze(this.hostname);
        let current_pool = ns.getServerMoneyAvailable(this.hostname);

        this.hack_yield = current_pool * hack_rate;
        this.hack_time = ns.getHackTime(this.hostname);
        this.hack_chance = ns.hackAnalyzeChance(this.hostname);
    }
    
    is_overhacked(ns) {
        const e = 0.01;
        
        let current_pool = ns.getServerMoneyAvailable(this.hostname);

        // if the yield is around the maximum cash there is, we'll overhack it
        // regardless
        if (this.max_money <= this.hack_yield) {
            return false;
        }

        // the hack yield should be greater than the current pool
        return current_pool * (1 + e) <= this.hack_yield;
    };

    // Updates the statistics when it's felt like the machine is overhacked
    //
    // Returns: machine
    async fix_overhack(
        ns,
        host
    ) {
        // keep growing while overhacked
        let free_ram = get_free_ram(ns, host) * (1 - RESERVATION_RATE);
        let has_posted = false;
        while (this.is_overhacked(ns)) {
            if (!has_posted) {
                print_header_bar(ns, this.hostname);
                ns.print("Machine is overhacked.");
                has_posted = true;
            }
            ns.print("Growing... ( ~" + this.hack_time * GROW_TIME_MUL / 1000 + "s )");

            let threads = Math.floor(free_ram / host.grow_mem);

            await exec_mt_wait(
                ns,
                "child_grow.js",
                host.hostname,
                threads,
                [this.hostname],
                this.hack_time * GROW_TIME_MUL,
            );

            this.security += threads * 0.004;
        }

        this.update_stats(ns);
    }

    /// Returns te yield of this machine if the pot is unlimited and security
    /// never rises
    base_yield() {
        return this.hack_yield / (this.hack_time / 1000);
    }

    /// Returns the average yield
    chance_corrected_yield() {
        return this.base_yield() * this.hack_chance;
    }

    /// Returns the average yield including grows based on historical grow-hack
    /// cycle
    total_corrected_yield() {
        // account for the duration of grows
        let grow_factor = (this.grows + 1) * GROW_TIME_MUL;
        let hack_factor = (this.hacks + 1);
        let factor = hack_factor / grow_factor;

        return this.chance_corrected_yield() * factor;
    }

    async procedural_hack(ns, host, min_rate, max_rate) {
        // derivable from the memory usage of the three scripts
        const GROW_CREDITS = 34;
        const WEAKEN_CREDITS = 34;
        const HACK_CREDITS = 35;
        const CREDIT_DENOMINATOR = 0.05;

        print_header_bar(ns, this.hostname);

        let ram_credits = Math.floor(get_free_ram(ns, host) / CREDIT_DENOMINATOR);

        let cs = { // current stats of processes
            gh: {
                threads: 0,
                pid: 0,
                end: 0,
                credits: 0,
                is_hack: false,
            },
            weaken: {
                threads: 0,
                pid: 0,
                end: 0,
                credits: 0,
            },
        }

        while (true) {
            let now = Date.now();
            //let is_gh_done = false;
            let is_weaken_done = false;

            await ns.sleep(
                Math.max(
                    Math.min(
                        cs.gh.end - now,
                        cs.weaken.end - now
                    ),
                    0
                )
            );

            // check which is done
            //is_gh_done = cs.gh.end < now;
            is_weaken_done = cs.weaken.end < now;

            // weaken procedure
            if (is_weaken_done) {
                while (ns.isRunning(cs.weaken.pid)) {
                    await ns.sleep(100);
                }

                cs.weaken.credits = 0;
                this.security_level = ns.getServerSecurityLevel(this.hostname);

                // we can tell how much to 
                let thread_by_security = (this.security_level - this.min_security) / 0.05;
                let thread_by_minimum = ((ram_credits / GROW_CREDITS) + (ram_credits / HACK_CREDITS)) * 0.004 / 0.05;
                let absolute_maximum = (ram_credits - cs.weaken.credits - cs.gh.credits) / WEAKEN_CREDITS;

                let threads = Math.floor(Math.min(absolute_maximum, Math.max(thread_by_security, thread_by_minimum)));
                let duration = this.hack_time * WEAKEN_TIME_MUL;
                ns.print("Weakening with " + threads + " threads (" + duration / 1000+ "s).");
                
                cs.weaken.pid = ns.exec(
                    "./child_weaken.js",
                    host.hostname,
                    threads,
                    this.hostname,
                );

                cs.weaken.end = Date.now() + duration;
                cs.weaken.threads = threads;
                cs.weaken.credits = WEAKEN_CREDITS * threads;
            }

            else { // if (is_gh_done)
                while (ns.isRunning(cs.gh.pid)) {
                    await ns.sleep(250);
                }

                cs.gh.credits = 0;
                this.security_level = ns.getServerSecurityLevel(this.hostname);

                let hack_threads = ram_credits / HACK_CREDITS;

                // determine if we're going to hack or grow
                let available_cash = ns.getServerMoneyAvailable(this.hostname);
                let should_grow = available_cash < this.hack_yield * min_rate * hack_threads;
                let should_hack = this.hack_yield * max_rate * hack_threads < available_cash;

                ns.print("AVAILABLE CASH: " + available_cash);
                ns.print("MIN THRESH:     " + this.hack_yield * min_rate * hack_threads);
                ns.print("MAX THRESH:     " + this.hack_yield * max_rate * hack_threads);

                let do_hack = cs.gh.is_hack;
                if (should_hack && !should_grow) {
                    do_hack = true;
                }

                else if (!should_hack && should_grow) {
                    do_hack = false;
                }

                if (do_hack) {
                    let threads = Math.floor((ram_credits - cs.gh.credits - cs.weaken.credits) / HACK_CREDITS);
                    let duration = this.hack_time;
                    ns.print("Hacking with " + threads + " threads (" + duration / 1000 + "s).");
                    
                    cs.gh.pid = ns.exec(
                        "./child_hack.js",
                        host.hostname,
                        threads,
                        this.hostname,
                    );

                    cs.gh.end = Date.now() + duration;
                    cs.gh.threads = threads;
                    cs.gh.credits = HACK_CREDITS * threads;
                }

                else {
                    let threads = Math.ceil((ram_credits - cs.gh.credits - cs.weaken.credits) / GROW_CREDITS);
                    let duration = this.hack_time * GROW_TIME_MUL;
                    ns.print("Growing with " + threads + " threads (" + duration / 1000+ "s).");

                    cs.gh.pid = ns.exec(
                        "./child_grow.js",
                        host.hostname,
                        threads,
                        this.hostname,
                    );

                    cs.gh.end = Date.now() + duration;
                    cs.gh.threads = threads;
                    cs.gh.credits = GROW_CREDITS * threads;
                }
            }
        }
    }
}

////////////////////////////////////////////////////////////////////////////////

/// Returns the list of networks that can be used to hack.
//
// Ignores:
// - unrooted machines
// - machines with no money
// - player-owned machines
// - machines beyond player's hacking level
function get_workable_network(ns) {
    let player_hacking_level = ns.getHackingLevel();

    let retval = [];
    for (let machine of get_network(ns, false)) {
        if (!machine.is_root) {
            continue;
        }

        if (machine.max_money == 0) {
            continue;
        }

        if (machine.player_owned) {
            continue;
        }

        if (player_hacking_level < machine.hacking_skill) {
            continue;
        }

        let new_machine = new HackableMachine(ns, machine, player_hacking_level);
        new_machine.update_stats(ns);

        retval.push(new_machine);
    }

    return retval;
}

function get_free_ram(ns, machine) {
    return (machine.max_ram - ns.getServerUsedRam(machine.hostname)) * (1 - RESERVATION_RATE);
}

function get_machine_stats(ns, host = "home") {
    let machine = new Machine(ns, host, "", 0);
    machine.max_ram = ns.getServerMaxRam(machine.hostname);
    //machine.weaken_mem = ns.getScriptRam("child_weaken.js");
    //machine.hack_mem = ns.getScriptRam("child_hack.js");
    //machine.grow_mem = ns.getScriptRam("child_grow.js");

    ns.tprint("WMEM: " + machine.weaken_mem);
    ns.tprint("GMEM: " + machine.grow_mem);
    ns.tprint("HMEM: " + machine.hack_mem);

    return machine;
}

async function exec_mt_wait(
    ns,
    command,
    machine,
    threads = 1,
    args = [],
    wait_hint = 0,
) {
    let pid = ns.exec(command, machine, threads, ...args);
    await ns.sleep(wait_hint);

    while (ns.isRunning(pid)) {
        await ns.sleep(250);
    }
}

////////////////////////////////////////////////////////////////////////////////

export async function main(ns) {
    let flags = ns.flags([
        ["help", false],
        ["min-rate", 9],
        ["max-rate", 10],
    ]);

    if (flags.help) {
        tprint_help(ns);
        return;
    }

    disable_logs(ns);
    let network = get_workable_network(ns);
    let host_stats = get_machine_stats(ns);

    /*
    await hack_machine(
        ns,
        networks[0],
        machine_stats,
        flags.min-rate,
        flags.max-rate
    );
    */
    
    ns.tprint("Machines to hack:");
    for (let machine of network) {
        ns.tprint("> " + machine.hostname);
    }

    // fix overhack first
    for (let machine of network) {
        await machine.minimize_security(ns, host_stats);
        await machine.fix_overhack(ns, host_stats);
    }

    network.sort(function(a, b) { return a.total_corrected_yield() - b.total_corrected_yield(); });

    await network[0].procedural_hack(ns, host_stats, flags["min-rate"], flags["max-rate"]);
    /*

    while (true) {
        for (let machine_stats of networks) {
            await hack_machine(ns, machine_stats);
        }
    }
    */
}
*/
// TODO: you still need to check if wait in weaken function really works
