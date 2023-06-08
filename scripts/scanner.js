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

    get_backdoor_string() {
        if (this.backdoored) {
            return "";
        }

        if (!this.is_root) {
            return "";
        }

        if (this.player_owned) {
            return "";
        }

        let output = "home; ";


        for (let [index, path] of this.path.split("/").entries()) {
            if (index < 2) {
                continue;
            }
            
            output += "connect " + path + "; ";
        }

        return output + "backdoor;\n"
    }

    nuke(ns) {
        // no need to nuke a machine already nuked
        if (this.is_root) {
            return 1;
        }

        // crack as many ports as we can. if it fails, then let it be
        try {
            ns.brutessh(this.hostname);
            ns.ftpcrack(this.hostname);
            ns.relaysmtp(this.hostname);
            ns.httpworm(this.hostname);
            ns.sqlinject(this.hostname);
        }
        catch (err) {
            // do nothing
        }

        try {
            ns.nuke(this.hostname);
            return 2;
        }
        
        catch(err) {
            return 0;
        }
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
                if (traversed_child.hostname == child) {
                    found_already = true;
                    break;
                }
            }

            if (found_already) {
                continue;
            }

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
        ["backdoor", false],
        ["nuke", false],
        ["show-all", false],
    ]);

    let network = get_network(ns);

    // backdoor mode
    if (flags["backdoor"]) {
        let output = "\n";
        for (let machine of network) {
            output += machine.get_backdoor_string();
        }

        ns.tprint(output);
        return;
    }

    // nuke mode
    if (flags["nuke"]) {
        let nuked = [];

        for (let machine of network) {
            let nuke_status = machine.nuke(ns);

            if (nuke_status == 2) {
                nuked.push([machine, true, true]);
            }

            else if (nuke_status == 1) {
                nuked.push([machine, false, true]);
            }

            else if (nuke_status == 0) {
                nuked.push([machine, false, false]);
            }
        }

        for (let [machine, newly_nuked, is_nuked] of nuked) {
            let print_line = "";
            if (newly_nuked) {
                print_line += "! ";
            }

            else {
                print_line += "  ";
            }

            if (is_nuked) {
                print_line += "Y ";
            }

            else {
                print_line += "  ";
            }

            print_line += machine.hostname;

            ns.tprint(print_line);
        }

        return;
    }

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

