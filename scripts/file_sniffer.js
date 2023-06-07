import { get_network } from "scanner.js";

export async function main(ns) {
    let nuked_machines = [];
    let machines = get_network(ns);

    for (let machine of machines) {
        if (machine["hostname"] == "home") {
            continue;
        }

        let files = ns.ls(machine["hostname"]);

        if (files.length == 0) {
            continue;
        }
        
        ns.tprint(machine["path"] + ":");
        for (let file of files) {
            ns.tprint("> " + file);
        }

        ns.tprint("");
    }
}
