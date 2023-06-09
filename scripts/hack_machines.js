import { Machine } from "machine_class.js";
import { get_network } from "scanner.js";

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
        ns,
        machine,
        player_hacking_level,
    ) {
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
    }

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

    // Minimizes security of a machine to its minimum.
    //
    // This is recommended only when starting the total hacking phase.
    async minimize_security(
        ns,
        host,
    ) {
        let free_ram = get_free_ram(ns, host) * (1 - RESERVATION_RATE);
        let has_posted = false;

        // repeatedly call `child_weaken.js` until sufficiently weakened
        while (this.min_security < ns.getServerSecurityLevel(this.hostname))
        {
            if (!has_posted) {
                print_header_bar(ns, this.hostname);
                ns.print("Machine is secure.");
                has_posted = true;
            }

            ns.print(
                "Weakening... ( ~" +
                this.hack_time * WEAKEN_TIME_MUL / 1000 +
                "s )"
            );

            await exec_mt_wait(
                ns,
                "child_weaken.js",
                host.hostname,
                // we don't care. just run as much threads as we can.
                Math.floor(free_ram / host.grow_mem),
                [this.hostname],
                this.hack_time * WEAKEN_TIME_MUL,
            );
        }

        this.security_level = this.min_security;
        this.update_stats(ns);
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

            else { // if (is_gh_done) {
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

// TODO: find the sweet spot of growth
