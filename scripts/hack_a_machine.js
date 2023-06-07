/// Disables many logs from functions.
function disable_logs(ns) {
    let noisy_methods = [
        "grow",
        "hack",
        "weaken"
    ];

    for (let method of noisy_methods) {
        ns.disableLog(method);
    }
}

/*
class HackStatistics {
    constructor() {
        this.successes = 0;
        this.attempts = 0;
        this.rate = 0;
        this.accel = 0;
    }

    async attempt(ns) {
        let start = Date.now();
        let amount = await ns.hack(target);

        // duration is in milliseconds.
        let duration = (Date.now() - hack_start) / 1_000;

        this.attempts += 1;

        // if we failed to hack
        if (hacked_amount == 0) {
            return;
        }

        this.successes += 1;
    }

    success(duration, amount) {
        let new_rate = amount / duration;
        let new_accel = new_rate - rate;

        this.rate = new_rate;
        this.accel = new_accel;
    }
}
*/

export async function main(ns) {
    disable_logs(ns);
    let target = ns.args[0];

    //let hack_stats = HackStatistics();

    let hack_successes = 0;
    let hack_attempts = 0;

    // start at whatever current optimal hack rate there is. $10/second?
    let hack_rate = 1;
    let hack_accel = 0;

    while (true) {
        ns.print("\n" + "=" * 80);

        if (hack_accel < 0) {
            ns.print("Growing...");
            await ns.grow(target);
            ns.print("\n" + "=" * 80);
        }

        // hacking statistics
        let hack_start = Date.now();
        let hacked_amount = await ns.hack(target);
        let hack_duration = (Date.now() - hack_start) / 1_000;

        hack_attempts += 1;
        
        // if we failed to hack
        if (hacked_amount == 0) {
            ns.print("Weakening...");
            await ns.weaken(target);
            continue;
        }
        
        let new_hack_rate = hacked_amount / hack_duration;
        let new_hack_accel = new_hack_rate - hack_rate;

        hack_rate = new_hack_rate;
        hack_accel = new_hack_accel;

        hack_successes += 1;

        ns.print("Hack Duration:     " + hack_duration + "s");
        ns.print("Hacked Amount:     $" + hacked_amount);
        ns.print("Hack Rate:         " + new_hack_rate);
        ns.print("Hack Acceleration: " + new_hack_accel);

        //await hack_stats.attempt(ns);

        //ns.print("Hack Duration:     " + hack_stats.duration + "s");
        //ns.print("Hacked Amount:     $" + hack_stats.amount);
        //ns.print("Hack Rate:         " + hack_stats.rate);
        //ns.print("Hack Acceleration: " + hack_stats.accel);
    }
}

