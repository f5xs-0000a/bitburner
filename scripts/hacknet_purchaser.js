class UpgradeGovernor {
    constructor() {
        this.ipp_log = -Infinity;
        this.action = async function(_ns) {};
    }

    assess_improvement_general(ipp_log, action) {
        if (this.ipp_log < ipp_log) {
            this.action = action;
            this.ipp_log = ipp_log;
        }
    }

    assess_better_level(level, ram, cores, price, action) {
        if (level == 200) {
            return;
        }
        
        let improvement = rate_improvement_level(level, ram, cores);
        this.assess_improvement_general(
            Math.log(improvement) - Math.log(price),
            action,
        );
    }

    assess_better_cores(level, ram, cores, price, action) {
        if (cores == 16) {
            return;
        }
        
        let improvement = rate_improvement_cores(level, ram, cores);
        this.assess_improvement_general(
            Math.log(improvement) - Math.log(price),
            action,
        );
    }

    assess_better_ram(level, ram, cores, price, action) {
        if (ram == 64) {
            return;
        }
        
        let improvement = rate_improvement_ram(level, ram, cores);
        this.assess_improvement_general
            Math.log(improvement) - Math.log(price),
            action,
        );
    }

    assess_hacknet_node(ns, index) {
        let stats = ns.hacknet.getNodeStats(index);
        
        let cores = stats.cores;
        let level = stats.level;
        let ram = stats.ram;

        this.assess_better_level(
            level,
            ram,
            cores,
            ns.hacknet.getLevelUpgradeCost(index),
            async function (ns) {
                ns.print("Upgrading Level of Machine " + index);
                await wait_until_enough_money(
                    ns,
                    ns.hacknet.getLevelUpgradeCost(index),
                );
                ns.hacknet.upgradeLevel(index);
            }
        );

        this.assess_better_cores(
            level,
            ram,
            cores,
            ns.hacknet.getCoreUpgradeCost(index),
            async function (ns) {
                ns.print("Upgrading Cores of Machine " + index);
                await wait_until_enough_money(
                    ns,
                    ns.hacknet.getCoreUpgradeCost(index)
                );
                ns.hacknet.upgradeCore(index);
            }
        );

        this.assess_better_ram(
            level,
            ram,
            cores,
            ns.hacknet.getRamUpgradeCost(index),
            async function (ns) {
                ns.print("Upgrading RAM of Machine " + index);
                await wait_until_enough_money(
                    ns,
                    ns.hacknet.getRamUpgradeCost(index)
                );
                ns.hacknet.upgradeRam(index);
            }
        );
    }

    assess_purchasing_new_node(ns) {
        let improvement = money_gain_rate(1, 1, 1);
        let price = ns.hacknet.getPurchaseNodeCost();

        this.assess_improvement_general(
            Math.log(improvement) - Math.log(price),
            async function (ns) {
                ns.print("Purchasing a new machine.");
                await wait_until_enough_money(ns, price);
                ns.hacknet.purchaseNode();
            }
        );
    }
}

export async function main(ns) {
    while (true) {
        let governor = new UpgradeGovernor();
        
        for (let i = 0; i < ns.hacknet.numNodes(); i += 1) {
            governor.assess_hacknet_node(ns, i);
        }
        governor.assess_purchasing_new_node(ns);

        await governor.action(ns);
        await ns.sleep(100);
    }
}

async function wait_until_enough_money(ns, money) {
    let has_printed = false;
    
    while (ns.getPlayer().money < money) {
        if (!has_printed) {
            ns.print(
                "Oops. You don't have enough money: (" +
                ns.getPlayer().money +
                "/" +
                money +
                ")"
            );
            has_printed = true;
        }
        await ns.sleep(1000);
    }
}

function rate_improvement_level(level, ram, cores) {
    return money_gain_rate(level + 1, ram, cores)
        - money_gain_rate(level, ram, cores);
}

function rate_improvement_ram(level, ram, cores) {
    return money_gain_rate(level, ram * 2, cores)
        - money_gain_rate(level, ram, cores);
}

function rate_improvement_cores(level, ram, cores) {
    return money_gain_rate(level, ram, cores + 1)
        - money_gain_rate(level, ram, cores);
}

// stolen directly from src/Hacknet/data/Constants.ts
// it's not accurate but it's rudimentary enough.
// let's hope that the values produced by this is
// directly proportional to what's produced in the actual
// game
function money_gain_rate(level, ram, cores) {
    const level_mult = level * 1.5;
    const ram_mult = Math.pow(1.035, ram - 1);
    const cores_mult = (cores + 5) / 6;

    return level_mult * ram_mult * cores_mult;
}
