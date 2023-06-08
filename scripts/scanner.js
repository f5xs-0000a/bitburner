export class Machine {
    constructor(
        ns,
        hostname,
        parent_host,
        degree
    ) {
        this.hostname = hostname;
        this.parent_host = parent_host;
        this.degree = degree;

        let stats = ns.getServer(hostname);

        this.is_root = stats.hasAdminRights;
        this.backdoored = stats.backdoorInstalled;
        this.max_money = stats.moneyMax;
        this.player_owned = stats.purchasedByPlayer;
        this.hacking_skill = stats.requiredHackingSkill;

        // to be filled later.
        this.path = "";
    }
}

export function get_network(ns) {
    let traversed = [];
    let pending = [new Machine(ns, "home", "", 0)];

    while (0 < pending.length) {
        // BFS, not DFS. therefore, don't use pop()
        let machine = pending.shift();

        // identify the children nodes of this machine
        for (let child of ns.scan(machine.hostname)) {
            // don't bother with a child node already traversed
            let found_already = false;
            for (let traversed_child of traversed) {
                ns.tprint(traversed_child.hostname, " ",  child)
                if (traversed_child.hostname == child) {
                    found_already = true;
                    break;
                }
            }

            if (found_already) {
                continue;
            }
            ns.tprint("NEW! " + child)

            // determine the properties of this new child
            let new_child = new Machine(
                ns,
                child,
                machine.hostname,
                machine.degree + 1
            );

            pending.push(new_child);
        }

        // put this node into list of traversed machines
        traversed.push(machine);
    }

    // adjust the traversal tree
    let max_degree = Math.max(...traversed.map(m => m.degree));
    for (let i = 0; i <= max_degree; i += 1) {
        for (let node of traversed) {
            if (node.degree != i) {
                continue;
            }

            if (i == 0) {
                node.path = "/" + node.hostname;
            }

            else {
                node.path = traversed
                    .find(function(n) { return n.hostname == node.parent_host })
                    .path + "/" + node.hostname;
            }
        }
    }

    return traversed;
}

export async function main(ns) {
    let flags = ns.flags([
        ["path", false],
        ["json", false],
    ]);

    let network = get_network(ns);

    // get the string length
    let max_str_len = 0;
    for (let machine of network) {
        if (flags["path"]) {
            if (max_str_len < machine.path.length) {
                max_str_len = machine.path.length;
            }
        }

        else {
            if (max_str_len < machine.hostname.length) {
                max_str_len = machine.hostname.length;
            }
        }
    }

    for (let machine of network) {
        if (ns.args.includes("--path")) {
            ns.tprint(
                machine.path +
                " ".repeat(max_str_len + 2 - machine.path.length) +
                machine.is_root
            );
        }

        else {
            ns.tprint(
                machine.hostname +
                " ".repeat(max_str_len + 2 - machine.hostname.length) +
                machine.is_root
            );
        }
    }
}
