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

async function hack_machine(ns, machine) {
    ns.print(
        "\n" +
        "=".repeat(8) +
        " " + machine["hostname"] +
        " " +
        "=".repeat(70 - machine["hostname"].length)
    );
    ns.print("Hacking...");

    if (machine["accel"] < 0) {
        ns.print("Growing...");
        await ns.grow(machine["hostname"]);
    }

    // hacking statistics
    let hack_start = Date.now();
    let hacked_amount = await ns.hack(machine["hostname"]);
    let hack_duration = (Date.now() - hack_start) / 1_000;

    machine["attempts"] += 1;

    // if we failed to hack
    if (hacked_amount == 0) {
        ns.print("Failed to hack (took " + hack_duration + "s).");
        ns.print("Weakening...");

        let weaken_start = Date.now();
        await ns.weaken(machine["hostname"]);
        let weaken_duration = (Date.now() - weaken_start) / 1_000;

        ns.print("Weakening took " + weaken_duration + "s.");

        return;
    }

    let new_hack_rate = hacked_amount / hack_duration;
    let new_hack_accel = new_hack_rate - machine["rate"];

    machine["rate"] = new_hack_rate;
    machine["accel"] = new_hack_accel;

    machine["successes"] += 1;

    ns.print("Hack Duration:     " + hack_duration + "s");
    ns.print("Hacked Amount:     $" + hacked_amount);
    ns.print("Hack Rate:         " + new_hack_rate);
    ns.print("Hack Acceleration: " + new_hack_accel);
}

export async function main(ns) {
    disable_logs(ns);
    let networks = get_workable_networks(ns);

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
