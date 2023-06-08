import { get_network } from "scanner.js";

/// Which machines to ignore
var IGNORE_MACHINES = [
    "home",
    "aleph-0"
]

/// Disables logging of specific commands
function disable_logs(ns) {
    let noisy_methods = [
        "disableLog",
        "grow",
        "hack",
        "weaken",
        "scan"
    ];

    for (let method of noisy_methods) {
        ns.disableLog(method);
    }
}

/// Returns the list of networks that can be used to hack
function get_workable_networks(ns) {
    let retval = [];
    for (let machine of get_network(ns, false)) {
        if (!machine["is_root"]) {
            continue;
        }

        if (IGNORE_MACHINES.includes(machine["hostname"])) {
            continue;
        }

        let translated_machine = {
            "hostname": machine["hostname"],
            "attempts": 0,
            "successes": 0,
            "rate": 0,
            "accel": 0,
        };

        retval.push(translated_machine);
    }

    return retval;
}

function get_free_ram(ns, machine) {
    return ns.getServerMaxRam(machine["hostname"])
        - ns.getServerUsedRam(machine["hostname"]);
}

async function hack_machine(
    ns,
    target,
    running_machine,
    min_rate,
    max_rate
) {
    //const GROW_MEM = 0.15;
    //const WEAKEN_MEM = 0.15;
    //const HACK_MEM = 0.1;
    const RESERVATION_RATE = 0.1;

    const GROW_TIME_MUL = 3.2;
    const WEAKEN_TIME_MUL = 4;

    const CHILD_WEAKEN_MEMORY = ns.getScriptRam("child_weaken.js");
    const CHILD_HACK_MEMORY = ns.getScriptRam("child_hack.js");
    const CHILD_GROW_MEMORY = ns.getScriptRam("child_grow.js");

    // TODO: don't bother with servers you can't hack yet

    // get the money output of the machine
    let current_pool = ns.getServerMoneyAvailable(target);
    let hack_rate = ns.hackAnalyze(target);
    let max_cash = ns.getServerMaxMoney(target);
    let hack_time = ns.getHackTime(target);

    // don't bother with servers like CSEC that has no money
    if (max_cash == 0) {
        return;
    }

    let yield_per_hack = current_pool * hack_rate;

    let cur_sec_lvl = ns.getServerSecurityLevel(target);
    let min_sec_lvl = ns.getServerMinSecurityLevel(target);

    let should_hack = current_pool < yield_per_hack * min_rate;

    // loop on weaken
    while (true) {
        hack_time = ns.getHackTime(target);
        
        let free_ram = get_free_ram(ns, running_machine) * (1 - RESERVATION_RATE);

        if (min_sec_lvl < cur_sec_lvl) {
            ns.tprint("Weakening...");
            let difference = (cur_sec_lvl - min_sec_lvl);
            let remaining_threads = difference / running_machine["weaken_effect"];
            
            let threads_per_round = Math.floor(free_ram / CHILD_WEAKEN_MEMORY);

            while (0 < remaining_threads) {
                //await ns.weaken(target, { threads: threads_per_round });
                ns.exec("child_weaken.js", running_machine["hostname"], threads_per_round, target);
                await ns.sleep(hack_time * WEAKEN_TIME_MUL * 1.125);
                remaining_threads -= threads_per_round;
            }
        }

        if (should_hack) {
            ns.tprint("Hacking...");
            // get the minimum amount of threads to hack maximally
            let threads_per_round = free_ram / CHILD_HACK_MEMORY;
            let maximum_hack_amount = Math.floor(current_pool / yield_per_hack);
            let threads = Math.ceil(Math.min(threads_per_round, maximum_hack_amount));

            //await ns.hack(target, { threas: threads });
            ns.exec("child_hack.js", running_machine["hostname"], threads, target);
            await ns.sleep(hack_time * 1.125);

            if (ns.getServerMoneyAvailable(target) < yield_per_hack * min_rate) {
                should_hack = false;
            }
        }

        else {
            ns.tprint("Growing...");
            while (ns.getServerMoneyAvailable(target) < yield_per_hack * max_rate) {
                let threads = Math.ceil(free_ram / CHILD_GROW_MEMORY);
                //await ns.grow(target, { threas: threads });
                ns.exec("child_grow.js", running_machine["hostname"], threads, target);
                await ns.sleep(hack_time * GROW_TIME_MUL * 1.125);
            }

            should_hack = true;
        }
    }
}

function print_help(ns) {
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

/// Returns the statistics of the machine this program is running on.
function get_machine_stats(ns) {
    let hostname = ns.getHostname();
    let server = ns.getServer(hostname);

    return {
        "hostname": hostname,
        "max_ram": server.maxRam,
        "weaken_effect": ns.weakenAnalyze(1, server.cpuCores),
    };
}

export async function main(ns) {
    let flags = ns.flags([
        ["help", false],
        ["min-rate", 9],
        ["max-rate", 10],
    ]);

    if (flags["help"]) {
        print_help(ns);
        return;
    }

    disable_logs(ns);
    let networks = get_workable_networks(ns);
    let machine_stats = get_machine_stats(ns);

    await hack_machine(
        ns,
        "n00dles",
        machine_stats,
        flags["min-rate"],
        flags["max-rate"]
    );

    return;

    // TODO: remove this restriction
    networks.filter(function(machine) { return machine["hostname"] == "n00dles"; });

    ns.print("Machines to hack:");
    for (let machine of networks) {
        ns.print("> " + machine["hostname"]);
    }

    while (true) {
        for (let machine_stats of networks) {
            await hack_machine(ns, machine_stats);
        }
    }
}
