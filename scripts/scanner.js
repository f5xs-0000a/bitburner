export function get_network(ns) {
    let traversed = [];
    let pending = [{ "hostname": "home", "parent": "", "degree": 0 }];

    while (0 < pending.length) {
        let machine = pending.pop();

        // identify the children nodes of this machine
        for (let child of ns.scan(machine["hostname"])) {
            // don't bother with a child node already traversed
            let found_already = false;
            for (let traversed_child of traversed) {
                if (traversed_child["hostname"] == child) {
                    found_already = true;
                    break;
                }
            }

            if (found_already) {
                continue;
            }

            // determine the properties of this new child
            let new_child = {
                "hostname": child,
                "parent": machine["hostname"],
                "degree": machine["degree"] + 1,
            };

            pending.push(new_child);
        }

        let stats = ns.getServer(machine["hostname"]);

        machine["is_root"] = stats.hasAdminRights;
        machine["backdoored"] = stats.backdoorInstalled;
        machine["max_money"] = stats.moneyMax;
        machine["player_owned"] = stats.purchasedByPlayer;
        machine["hacking_skill"] = stats.requiredHackingSkill;

        // put this node into list of traversed machines
        traversed.push(machine);
    }

    // adjust the traversal tree
    let max_degree = 0;
    for (let node of traversed) {
        if (max_degree < node["degree"]) {
            max_degree = node["degree"];
        }
    }

    for (let i = 0; i <= max_degree; i += 1) {
        for (let node of traversed) {
            if (node["degree"] != i) {
                continue;
            }

            if (i == 0) {
                node["path"] = "/" + node["hostname"];
            }

            else {
                node["path"] = traversed
                    .find(function(n) { return n["hostname"] == node["parent"] })
                    ["path"] + "/" + node["hostname"];
            }
        }
    }

    return traversed;
}

export async function main(ns) {
    let max_str_len = 0;
    let network = get_network(ns);

    // get the string length
    for (let machine of network) {
        if (ns.args.includes("--path")) {
            if (max_str_len < machine["path"].length) {
                max_str_len = machine["path"].length;
            }
        }

        else {
            if (max_str_len < machine["hostname"].length) {
                max_str_len = machine["hostname"].length;
            }
        }
    }

    if (ns.args.includes("--json")) {
        ns.print(network);
    }

    else {
        for (let machine of network) {
            if (ns.args.includes("--path")) {
                ns.tprint(
                    machine["path"] +
                    " ".repeat(max_str_len + 2 - machine["path"].length) +
                    machine["is_root"]
                );
            }

            else {
                ns.tprint(
                    machine["hostname"] +
                    " ".repeat(max_str_len + 2 - machine["hostname"].length) +
                    machine["is_root"]
                );
            }
        }
    }
}
