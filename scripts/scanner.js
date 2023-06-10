import { Machine } from "machine_class.js";

const scanned = (machine) => ({
    /**
     * @param {NS} ns
     *     - NetScript environment
     * @returns {string}
     */
    get_backdoor_string: (ns) => {
        if (machine.is_backdoored()) {
            return "";
        }

        if (!machine.is_root()) {
            return "";
        }

        if (machine.is_player_owned()) {
            return "";
        }

        if (ns.getHackingLevel() < machine.get_hacking_skill()) {
            return "";
        }

        let output = "home; ";

        for (let [index, path] of machine.get_path().split("/").entries()) {
            if (index < 2) {
                continue;
            }
            
            output += "connect " + path + "; ";
        }

        return output + "backdoor;\n"
    },

    /**
     * @param {NS} ns
     *     - NetScript environment
     * @returns {integer}
     */
    nuke: (ns) => {
        // no need to nuke a machine already nuked
        if (machine.is_root) {
            return 1;
        }

        // crack as many ports as we can. if it fails, then let it be
        try {
            ns.brutessh(machine.get_hostname());
            ns.ftpcrack(machine.get_hostname());
            ns.relaysmtp(machine.get_hostname());
            ns.httpworm(machine.get_hostname());
            ns.sqlinject(machine.get_hostname());
        }
        catch (err) {
            // do nothing
        }

        try {
            ns.nuke(machine.get_hostname());
            return 2;
        }
        
        catch(err) {
            return 0;
        }
    },

    /**
     * @param {NS} ns
     *     - NetScript environment
     * @returns {string[]}
     */
    sniff_files: (ns) => {
        if (machine.get_degree() == 0) {
            return [];
        }

        return ns.ls(machine.get_hostname());
    }
})

/**
 * @param {NS} ns
 *     - NetScript environment
 * @returns {Machine[]}
 */
export function get_network(ns) {
    let traversed = [];
    let home = new Machine(ns, "home", "", 0);
    Object.assign(home, scanned(home));
    let pending = [home];

    while (0 < pending.length) {
        // BFS, not DFS. therefore, don't use pop()
        let machine = pending.shift();

        // identify the children nodes of this machine
        for (let child of ns.scan(machine.get_hostname())) {
            // don't bother with a child node already traversed
            let found_already = false;
            for (let traversed_child of traversed) {
                if (traversed_child.get_hostname() == child) {
                    found_already = true;
                    break;
                }
            }

            if (found_already) {
                continue;
            }

            // determine the properties of this new child
            let new_child = machine.create_child(ns, child);
            Object.assign(new_child, scanned(new_child));
            pending.push(new_child);
        }

        // put this node into list of traversed machines
        traversed.push(machine);
    }

    return traversed;
}

/**
 * @param {NS} ns
 *     - NetScript environment
 * @param {Machine[]} network
 *     - list of machines in the network
 * @returns {void}
 */
function backdoor_mode(ns, network) {
    let output = "\n";
    for (let machine of network) {
        output += machine.get_backdoor_string(ns);
    }

    ns.tprint(output);
}

/**
 * @param {NS} ns
 *     - NetScript environment
 * @param {Machine[]} network
 *     - list of machines in the network
 * @param {bool} show_all
 *     - whether to also include machines not yet nuked
 * @returns {void}
 */
function nuke_mode(ns, network, show_all) {
    let nuked = [];

    for (let machine of network) {
        ns.tprint(machine);
        ns.tprint(show_all);
        let nuke_status = machine.nuke(ns);

        if (nuke_status == 2) {
            nuked.push([machine, true, true]);
        }

        else if (nuke_status == 1) {
            nuked.push([machine, false, true]);
        }

        else if (show_all && nuke_status == 0) {
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

        print_line += machine.get_hostname();

        ns.tprint(print_line);
    }
}

/**
 * @param {NS} ns
 *     - NetScript environment
 * @param {Machine[]} network
 *     - list of machines in the network
 * @param {bool} path
 *     - whether to show the full traversal path of the network
 * @returns {void}
 */
function display_mode(ns, network, path_mode) {
    let max_str_len = 0;
    for (let machine of network) {
        if (path_mode) {
            if (max_str_len < machine.get_path().length) {
                max_str_len = machine.get_path().length;
            }
        }

        else {
            if (max_str_len < machine.get_hostname().length) {
                max_str_len = machine.get_hostname().length;
            }
        }
    }

    for (let machine of network) {
        if (path_mode) {
            ns.tprint(
                machine.get_path() +
                " ".repeat(max_str_len + 2 - machine.get_path().length) +
                machine.is_root()
            );
        }

        else {
            ns.tprint(
                machine.get_hostname() +
                " ".repeat(max_str_len + 2 - machine.get_hostname().length) +
                machine.is_root()
            );
        }
    }
}

/**
 * @param {NS} ns
 *     - NetScript environment
 * @param {Machine[]} network
 *     - list of machines in the network
 * @param {bool} path
 *     - whether to show the full traversal path of the network
 * @returns {void}
 */
function sniff_mode(ns, network, path_mode) {
    for (let machine of network) {
        if (machine.get_degree() == 0) {
            continue;
        }

        let files = machine.sniff_files(ns);

        if (files.length == 0) {
            continue;
        }

        if (path_mode) {
            ns.tprint(machine.get_path() + ":");
        }

        else {
            ns.tprint(machine.get_hostname() + ":");
        }

        for (let file of files) {
            ns.tprint("> " + file);
        }

        ns.tprint("");
    }
}

/** @param {NS} ns */
export async function main(ns) {
    let flags = ns.flags([
        ["path", false],
        ["backdoor", false],
        ["nuke", false],
        ["show-all", false],
        ["sniff", false],
    ]);

    let network = get_network(ns);

    // backdoor mode
    if (flags["backdoor"]) {
        backdoor_mode(ns, network);
        return;
    }

    // nuke mode
    if (flags["nuke"]) {
        nuke_mode(ns, network, flags["show-all"]);
        return;
    }

    // sniff mode
    if (flags["sniff"]) {
        sniff_mode(ns, network, flags["path"]);
        return;
    }

    display_mode(ns, network, flags["path"]);
}
