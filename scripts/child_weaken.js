export async function main(ns) {
    let flags = ns.flags([]);
    await ns.weaken(flags["_"][0]);
}
