import { get_network } from "scanner.js";

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
    ns.tprint("Parameters:")
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
    let len = text.length;

    ns.print("\n======== " + text + " " + "=".repeat(Math.max(0, 70 - text.length)));
}

////////////////////////////////////////////////////////////////////////////////

// Updates the statistics of the target machine.
//
// Recommended to be called upon hacking level up.
//
// Returns: machine
function update_machine_stats(
    ns,
    machine
) {
    let hack_rate = ns.hackAnalyze(machine["hostname"]);
    let current_pool = ns.getServerMoneyAvailable(machine["hostname"]);

    machine["hack_yield"] = current_pool * hack_rate;
    machine["hack_time"] = ns.getHackTime(machine["hostname"]);
    machine["hack_chance"] = ns.hackAnalyzeChance(machine["hostname"]);

    return machine;
}

// Minimizes security of a machine to its minimum.
//
// This is recommended only when starting the total hacking phase.
async function minimize_security(
    ns,
    target,
    running_machine
) {
    let free_ram = get_free_ram(ns, running_machine) * (1 - RESERVATION_RATE);
    let has_posted = false;
    while (target["min_sec_lvl"] < ns.getServerSecurityLevel(target["hostname"])) {
        if (!has_posted) {
            print_header_bar(ns, target["hostname"]);
            ns.print("Machine is secure.");
            has_posted = true;
        }
        ns.print("Weakening... ( ~" + target["hack_time"] * WEAKEN_TIME_MUL / 1000 + "s )");

        await exec_mt_wait(
            ns,
            "child_grow.js",
            running_machine["hostname"],
            // we don't care. just run as much threads as we can.
            Math.floor(free_ram / running_machine["grow_mem"]),
            [target["hostname"]],
            target["hack_time"] * WEAKEN_TIME_MUL,
        );
    }

    return update_machine_stats(ns, target);
}

// Updates the target statistics when it's felt like the target machine is
// overhacked
//
// Returns: machine
async function fix_overhack(
    ns,
    target,
    running_machine
) {
    const e = 0.01;

    let is_overhacked = function() {
        let current_pool = ns.getServerMoneyAvailable(target["hostname"]);

        // if the yield is around the maximum cash there is, we'll overhack it
        // regardless
        if (target["max_money"] <= target["hack_yield"]) {
            return false;
        }

        // the hack yield should be greater than the current pool
        return current_pool * (1 + e) <= target["hack_yield"];
    };

    // keep growing while overhacked
    let free_ram = get_free_ram(ns, running_machine) * (1 - RESERVATION_RATE);
    let has_posted = false;
    while (is_overhacked()) {
        if (!has_posted) {
            print_header_bar(ns, target["hostname"]);
            ns.print("Machine is overhacked.");
            has_posted = true;
        }
        ns.print("Growing... ( ~" + target["hack_time"] * GROW_TIME_MUL / 1000 + "s )");

        await exec_mt_wait(
            ns,
            "child_grow.js",
            running_machine["hostname"],
            Math.floor(free_ram / running_machine["grow_mem"]),
            [target["hostname"]],
            target["hack_time"] * GROW_TIME_MUL,
        );
    }

    return update_machine_stats(ns, target);
}

/// Returns the list of networks that can be used to hack.
//
// Ignores:
// - unrooted machines
// - machines with no money
// - player-owned machines
//
// Returns a dictionary containing the keys:
// - is_root
// - backdoored
// - max_money
// - player_owned
// - hacking_skill
// - hack_level_recorded
// - hack_yield
// - hack_time
function get_workable_networks(ns) {
    let player_hacking_level = ns.getHackingLevel();

    let retval = [];
    for (let machine of get_network(ns, false)) {
        if (!machine["is_root"]) {
            continue;
        }

        if (machine["max_money"] == 0) {
            continue;
        }

        if (machine["player_owned"]) {
            continue;
        }

        if (machine["hostname"] == "home") {
            continue;
        }

        machine["hack_level_recorded"] = player_hacking_level;
        machine = update_machine_stats(ns, machine);

        retval.push(machine);
    }

    return retval;
}

////////////////////////////////////////////////////////////////////////////////



function get_free_ram(ns, machine) {
    return machine["max_ram"] - ns.getServerUsedRam(machine["hostname"]);
}

/// Returns the statistics of the machine this program is running on.
function get_machine_stats(ns) {
    let hostname = ns.getHostname();
    let server = ns.getServer(hostname);

    return {
        "hostname": hostname,
        "max_ram": server.maxRam,
        "weaken_effect": ns.weakenAnalyze(1, server.cpuCores),

        // amount of memory required per thread to execute a given script
        "weaken_mem": ns.getScriptRam("child_weaken.js"),
        "hack_mem": ns.getScriptRam("child_hack.js"),
        "grow_mem": ns.getScriptRam("child_grow.js"),
    };
}

////////////////////////////////////////////////////////////////////////////////

// the rate of RAM on the `running_machine` that will be reserved for other uses
const RESERVATION_RATE = 0.1;

// hack_time multipliers to obtain their corresponding waiting times.
const GROW_TIME_MUL = 3.2;
const WEAKEN_TIME_MUL = 4;

/// A convenience function for calling exec and waiting for it to finish.
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

/// Hacks a single machine
async function hack_machine(
    ns,              // netscript accessors
    target,          // String, target machine
    running_machine, // Object, running machine statistics
    min_rate,        // number,
    max_rate         // number,
) {
    let yield_per_hack = current_pool * hack_rate;

    let cur_sec_lvl = ns.getServerSecurityLevel(target["hostname"]);
    let min_sec_lvl = ns.getServerMinSecurityLevel(target["hostname"]);

    let should_hack = current_pool < yield_per_hack * min_rate;

    // loop on weaken
    while (true) {
        hack_time = ns.getHackTime(target["hostname"]);
        
        let free_ram = get_free_ram(ns, running_machine) * (1 - RESERVATION_RATE);

        if (min_sec_lvl < cur_sec_lvl) {
            ns.print("Weakening...");
            let difference = (cur_sec_lvl - min_sec_lvl);
            let remaining_threads = difference / running_machine["weaken_effect"];
            
            let threads_per_round = Math.floor(free_ram / running_machine["weaken_mem"]);

            while (0 < remaining_threads) {
                ns.exec("child_weaken.js", running_machine["hostname"], threads_per_round, target["hostname"]);
                await ns.sleep(hack_time * WEAKEN_TIME_MUL * 1.125);
                remaining_threads -= threads_per_round;
            }
        }

        if (should_hack) {
            ns.print("Hacking...");
            // get the minimum amount of threads to hack maximally
            let threads_per_round = free_ram / running_machine["hack_mem"];
            let maximum_hack_amount = Math.floor(current_pool / yield_per_hack);
            let threads = Math.ceil(Math.min(threads_per_round, maximum_hack_amount));

            ns.exec("child_hack.js", running_machine["hostname"], threads, target["hostname"]);
            await ns.sleep(hack_time * 1.125);

            if (ns.getServerMoneyAvailable(target["hostname"]) < yield_per_hack * min_rate) {
                should_hack = false;
            }
        }

        else {
            ns.print("Growing...");
            while (ns.getServerMoneyAvailable(target["hostname"]) < yield_per_hack * max_rate) {
                let threads = Math.ceil(free_ram / running_machine["grow_mem"]);
                ns.exec("child_grow.js", running_machine["hostname"], threads, target["hostname"]);
                await ns.sleep(hack_time * GROW_TIME_MUL * 1.125);
            }

            should_hack = true;
        }
    }
}
export async function main(ns) {
    let flags = ns.flags([
        ["help", false],
        ["min-rate", 9],
        ["max-rate", 10],
    ]);

    if (flags["help"]) {
        tprint_help(ns);
        return;
    }

    disable_logs(ns);
    let networks = get_workable_networks(ns);
    let machine_stats = get_machine_stats(ns);

    /*
    await hack_machine(
        ns,
        networks[0],
        machine_stats,
        flags["min-rate"],
        flags["max-rate"]
    );
    */
    
    ns.tprint("Machines to hack:");
    for (let machine of networks) {
        ns.tprint("> " + machine["hostname"]);
    }

    // fix overhack first
    for (let machine of networks) {
        await minimize_security(ns, machine, machine_stats);
        await fix_overhack(ns, machine, machine_stats);
        await minimize_security(ns, machine, machine_stats);
    }

    return;

    /*

    while (true) {
        for (let machine_stats of networks) {
            await hack_machine(ns, machine_stats);
        }
    }
    */
}

