import { get_network } from "scanner.js";

export async function main(ns) {
    let nuked_machines = [];
    let machines = get_network(ns);

    for (let machine of machines) {
        if (machine["is_root"]) {
            nuked_machines.push(machine["hostname"]);

        ns.tprint("Found `" + machine["hostname"] + "`");
            continue;
        }
        
        ns.print("Nuking " + machine["hostname"]);
        try {
            ns.brutessh(machine["hostname"]);
            ns.nuke(machine["hostname"]);
            ns.print("Nuked " + machine["hostname"]);
            nuked_machines.push(machine["hostname"]);
        }
        
        catch(err) {
            ns.print("Failed to nuke `" + machine["hostname"] + "`: " + err);
        }
    }

    nuked_machines.sort();

    ns.tprint("Rooted machines: " + nuked_machines.length + "/" + machines.length);
    for (let machine of nuked_machines) {
        ns.tprint("- " + machine);
    }
}
