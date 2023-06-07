/*
// checker: impl Future<Output = bool>
// method: impl Future<Output = ()>
async function checked_decorator(checker, performer) {
    if (!await checker) {
        return;
    }

    await performer;
}

// target: &str
// threshold: f64
async function check_if_low_security_level(target, threshold) {
    return ns.getServerSecurityLevel(target) <= threshold;
}
*/

export async function main(ns) {
    let target = ns.args[0];

    let hack_successes = 0;
    let hack_attempts = 0;

    // start at whatever current optimal hack rate there is. $10/second?
    let hack_rate = 1;
    let hack_accel = 0;

    while (true) {
        if (hack_accel < 0) {
            await ns.grow();
        }

        // hacking statistics
        let hack_start = Date.now();
        let hacked_amount = await ns.hack(target);
        let hack_duration = (Date.now() - hack_start) / 1_000;

        hack_attempts += 1;
        
        // if we failed to hack
        if (hacked_amount == 0) {
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
    }
        //iteration = iteration % 65536;
    //}
}

