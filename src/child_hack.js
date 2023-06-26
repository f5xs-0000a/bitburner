/** @param {NS} ns */
export async function main(ns) {
    let flags = ns.flags([]);
    await ns.hack(flags["_"][0]);
}
