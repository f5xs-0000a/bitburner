export async function main(ns) {
    let flags = ns.flags([]);
    await ns.grow(flags["_"][0]);
}
