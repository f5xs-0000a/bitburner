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
        this.weaken_effect = ns.weakenAnalyze(1, stats.cpuCores);
        this.min_security = stats.minDifficulty;

        // to be filled later.
        this.path = "";
    }
}
