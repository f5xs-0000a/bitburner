export class Machine {
    /**
     * @param {NS} ns
     *     - NetScript environment
     * @param {string} hostname
     *     - hostname of the machine
     * @param {string} parent_host
     *     - hostname of the parent machine (closer to home)
     *     - defaults to ""
     * @param {number} degree
     *     - least number of jumps required to get from home
     *     - defaults to 0
    */
    constructor(
        ns,
        hostname,
        parent_host = "",
        degree = 0
    ) {
        this._hostname = hostname;
        this._parent_host = parent_host;
        this._degree = degree;

        let stats = ns.getServer(hostname);

        this._is_root = stats.hasAdminRights;
        this._backdoored = stats.backdoorInstalled;
        this._max_money = stats.moneyMax;
        this._player_owned = stats.purchasedByPlayer;
        this._hacking_skill = stats.requiredHackingSkill;
        this._min_security = stats.minDifficulty;

        // to be filled later.
        this._path = "";
    }

    get_hostname() {
        return this._hostname;
    }

    get_path() {
        return this._path;
    }

    is_root() {
        return this._is_root;
    }

    upget_root() {
        this._is_root = ns.getServer(this.get_hostname()).hasAdminRights;
        return this.get_root();
    }

    is_backdoored() {
        return this._backdoored;
    }

    upget_backdoor() {
        this._backdoored = ns.getServer(this.get_hostname()).backdoorInstalled;
        return this.get_backdoor();
    }

    get_max_money() {
        return this._max_money;
    }

    is_player_owned() {
        return this._player_owned;
    }

    get_hacking_skill() {
        return this._hacking_skill;
    }

    upget_hacking_skill() {
        this._backdoored = ns.getServer(this.get_hostname()).requiredHackingSkill;
        return this.get_backdoor();
    }

    get_min_security() {
        return this._min_security;
    }
}
