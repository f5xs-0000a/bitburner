/** @param {NS} ns */
export async function main(ns) {
    let flags = ns.flags([]);
    let sleep_time = Number(flags["_"][1]);

    if (0 < sleep_time) {
        await ns.sleep(sleep_time);
    }

    await ns.weaken(flags["_"][0]);
}
