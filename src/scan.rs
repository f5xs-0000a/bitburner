use core::ops::{
    Deref,
    DerefMut,
};
use std::fmt::Write as _;

use clap::{
    Args,
    ValueEnum,
};

use crate::{
    machine::{
        get_machines,
        Machine,
    },
    netscript::NsWrapper,
};

#[derive(Debug, Clone)]
pub struct ScannedMachine(Machine);

impl From<Machine> for ScannedMachine {
    fn from(from: Machine) -> ScannedMachine {
        ScannedMachine(from)
    }
}

impl Deref for ScannedMachine {
    type Target = Machine;

    fn deref(&self) -> &Machine {
        &self.0
    }
}

impl DerefMut for ScannedMachine {
    fn deref_mut(&mut self) -> &mut Machine {
        &mut self.0
    }
}

enum NukeResult {
    JustNuked,
    WasNuked,
    NotNuked,
}

impl ScannedMachine {
    fn nuke(
        &mut self,
        ns: &NsWrapper,
    ) -> NukeResult {
        use NukeResult::*;

        if self.is_root(ns) {
            return WasNuked;
        }

        if ns.get_player_hacking_level() < self.get_min_hacking_skill() {
            return NotNuked;
        }

        self.run_brute_ssh(ns);
        self.run_ftp_crack(ns);
        self.run_relay_smtp(ns);
        self.run_http_worm(ns);
        self.run_sql_inject(ns);

        if self.0.nuke(ns) {
            JustNuked
        }
        else {
            NotNuked
        }
    }
}

#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum DisplayMode {
    Path,
    Cd,
    Name,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum ExecMode {
    Scan,
    Nuke,
    Backdoor,
    Sniff,
}

#[derive(Args, Debug, Clone)]
pub struct ScanMode {
    #[arg(long, short, value_enum, default_value_t = ExecMode::Scan)]
    exec: ExecMode,
    #[arg(long, short, value_enum, default_value_t = DisplayMode::Name)]
    display: DisplayMode,
}

impl ScanMode {
    pub fn execute(
        &self,
        ns: &NsWrapper,
    ) {
        use ExecMode::*;

        let mut machines = get_machines(ns)
            .into_iter()
            .map(|m| ScannedMachine::from(m))
            .collect::<Vec<_>>();

        match self.exec {
            Nuke => nuke_mode(ns, &mut *machines, self.display),
            Scan => scan_mode(ns, &mut *machines, self.display),
            Sniff => sniff_mode(ns, &mut *machines, self.display),
            Backdoor => backdoor_mode(ns, &*machines),
        }
    }
}

// hostname, ip address, organization, max money hacking skill, min security,
// cpu cores, required open ports
pub fn get_longest_stuff<'a>(
    iter: impl Iterator<Item = &'a Machine>
) -> (usize, usize, usize, usize, usize, usize, usize, usize) {
    let mut hostname = 0;
    let mut ip_address = 0;
    let mut organization = 0;
    let mut max_money = 0;
    let mut hacking_skill = 0;
    let mut security = 0;
    let mut cpu_cores = 0;
    let mut required_open_ports = 0;

    for machine in iter {
        hostname = hostname.max(machine.get_hostname().len());
        ip_address = ip_address.max(machine.get_ip_address().len());
        organization = organization.max(machine.get_organization_name().len());

        // these are numerical and therefore should have a far more numerical
        // way to get their length. and it's actually easy!
        // \lfloor \log _{10} x \rfloor + 2
        // note: doesn't work on values between 0 and 1. those need
        // conditionals.
        max_money =
            max_money.max(machine.get_max_money().max(1).ilog10() as usize + 2);
        hacking_skill = hacking_skill
            .max(machine.get_min_hacking_skill().max(1).ilog(10) as usize + 2);
        security = security
            .max(machine.get_min_security().log(10.).floor() as usize + 2);
        cpu_cores =
            cpu_cores.max(machine.get_cpu_cores().max(1).ilog10() as usize + 2);
        required_open_ports = required_open_ports.max(
            machine.get_required_open_ports().max(1).ilog10() as usize + 2,
        );
    }

    (
        hostname,
        ip_address,
        organization,
        max_money,
        hacking_skill,
        security,
        cpu_cores,
        required_open_ports,
    )
}

fn scan_mode(
    ns: &NsWrapper,
    network: &mut [ScannedMachine],
    display_mode: DisplayMode,
) {
    use DisplayMode::*;

    network.sort_unstable_by(|m1, m2| {
        m1.get_min_hacking_skill().cmp(&m2.get_min_hacking_skill())
            .then(m1.get_max_money().cmp(&m2.get_max_money()))
    });

    let (name_len, ip_len, org_len, mm_len, hs_len, sec_len, cc_len, rop_len) =
        get_longest_stuff(network.iter().map(|m| &m.0));

    let mut print_str = "\n".to_owned();
    for machine in network.iter() {
        match display_mode {
            Path => {
                write!(&mut print_str, "\n").unwrap();
                for traversal in machine.get_traversal().iter() {
                    write!(&mut print_str, "/{}", traversal).unwrap();
                }
                write!(&mut print_str, "\n ").unwrap();
            },

            Cd => {
                write!(&mut print_str, "\nhome; ").unwrap();
                for traversal in machine.get_traversal().iter().skip(1) {
                    write!(&mut print_str, "connect {}; ", traversal).unwrap();
                }
                write!(&mut print_str, "\n ").unwrap();
            },

            Name => {
                write!(
                    &mut print_str,
                    "{: <lnl$}   ",
                    machine.get_hostname(),
                    lnl = name_len
                )
                .unwrap();
            },
        }

        let player_owned = match machine.is_player_owned() {
            true => "  Owned  ",
            false => "Not Owned",
        };

        let is_root = match machine.is_root(ns) {
            true => "ROOT",
            false => "user",
        };

        writeln!(
            &mut print_str,
            "   {}   {: >lip$}   {: <lorg$}   {: >2}°   {: <lmm$}${}   {}   \
             Hack Lvl{: >lhs$}   {: >lms$} Sec   {: >lcc$}-Core   {: >lrop$} \
             Ports",
            is_root,
            machine.get_ip_address(),
            machine.get_organization_name(),
            machine.get_degree(),
            "",
            machine.get_max_money(),
            player_owned,
            machine.get_min_hacking_skill(),
            machine.get_min_security(),
            machine.get_cpu_cores(),
            machine.get_required_open_ports(),
            lip = ip_len,
            lorg = org_len,
            lmm = mm_len - machine.get_max_money().max(1).ilog10() as usize - 2,
            lhs = hs_len,
            lms = sec_len,
            lcc = cc_len,
            lrop = rop_len,
        )
        .unwrap();
    }

    ns.tprint(&*print_str);
}

fn nuke_mode(
    ns: &NsWrapper,
    network: &mut [ScannedMachine],
    display_mode: DisplayMode,
) {
    use DisplayMode::*;
    use NukeResult::*;

    let mut nuked_machines = network
        .iter_mut()
        .map(|m| {
            let nuke_stat = m.nuke(ns);
            (m, nuke_stat)
        })
        .collect::<Vec<_>>();

    nuked_machines.sort_unstable_by(|(m1, _), (m2, _)| {
        m1.get_degree()
            .cmp(&m2.get_degree())
            .then(m1.get_hostname().cmp(&m2.get_hostname()))
    });

    let (name_len, ip_len, org_len, mm_len, hs_len, sec_len, cc_len, rop_len) =
        get_longest_stuff(nuked_machines.iter().map(|m| &**m.0));

    let mut print_str = "\n".to_owned();
    for (machine, status) in nuked_machines.iter() {
        match display_mode {
            Path => {
                write!(&mut print_str, "\n").unwrap();
                for traversal in machine.get_traversal().iter() {
                    write!(&mut print_str, "/{}", traversal).unwrap();
                }
                write!(&mut print_str, "\n ").unwrap();
            },

            Cd => {
                write!(&mut print_str, "\nhome; ").unwrap();
                for traversal in machine.get_traversal().iter().skip(1) {
                    write!(&mut print_str, "connect {}; ", traversal).unwrap();
                }
                write!(&mut print_str, "\n ").unwrap();
            },

            Name => {
                write!(
                    &mut print_str,
                    "{: <lnl$}   ",
                    machine.get_hostname(),
                    lnl = name_len
                )
                .unwrap();
            },
        }

        let player_owned = match machine.is_player_owned() {
            true => "  Owned  ",
            false => "Not Owned",
        };

        let is_root = match machine.is_root(ns) {
            true => "ROOT",
            false => "user",
        };

        let nuke_mode = match status {
            WasNuked => "nuked",
            JustNuked => "NUKED",
            NotNuked => "     ",
        };

        writeln!(
            &mut print_str,
            "   {}   {}   {: >lip$}   {: <lorg$}   {: >2}°   {: <lmm$}${}   \
             {}   Hack Lvl{: >lhs$}   {: >lms$} Sec   {: >lcc$}-Core   {: \
             >lrop$} Ports",
            is_root,
            nuke_mode,
            machine.get_ip_address(),
            machine.get_organization_name(),
            machine.get_degree(),
            "",
            machine.get_max_money(),
            player_owned,
            machine.get_min_hacking_skill(),
            machine.get_min_security(),
            machine.get_cpu_cores(),
            machine.get_required_open_ports(),
            lip = ip_len,
            lorg = org_len,
            lmm = mm_len - machine.get_max_money().max(1).ilog10() as usize - 2,
            lhs = hs_len,
            lms = sec_len,
            lcc = cc_len,
            lrop = rop_len,
        )
        .unwrap();
    }

    ns.tprint(&*print_str);
}

fn sniff_mode(
    ns: &NsWrapper,
    network: &mut [ScannedMachine],
    display_mode: DisplayMode,
) {
    use DisplayMode::*;

    let mut print_str = "\n".to_owned();
    for machine in network.iter() {
        if machine.is_player_owned() {
            continue;
        }

        let files = ns.ls(machine.get_hostname());

        if files.is_empty() {
            continue;
        }

        match display_mode {
            Path => {
                write!(&mut print_str, "\n").unwrap();
                for traversal in machine.get_traversal().iter() {
                    write!(&mut print_str, "/{}", traversal).unwrap();
                }
                write!(&mut print_str, "\n").unwrap();
            },

            Cd => {
                write!(&mut print_str, "\nhome; ").unwrap();
                for traversal in machine.get_traversal().iter().skip(1) {
                    write!(&mut print_str, "connect {}; ", traversal).unwrap();
                }
                write!(&mut print_str, "\n").unwrap();
            },

            Name => {
                writeln!(&mut print_str, "\n{}", machine.get_hostname())
                    .unwrap();
            },
        }

        for filename in files.into_iter() {
            writeln!(&mut print_str, "- {}", filename).unwrap();
        }
    }

    ns.tprint(&*print_str);
}

fn backdoor_mode(
    ns: &NsWrapper,
    network: &[ScannedMachine],
) {
    let mut print_str = "\n".to_owned();
    for machine in network.iter() {
        if machine.is_player_owned() {
            continue;
        }

        if machine.is_backdoored(ns) {
            continue;
        }

        if ns.get_player_hacking_level() < machine.get_min_hacking_skill() {
            continue;
        }

        write!(&mut print_str, "\nhome; ").unwrap();
        for traversal in machine.get_traversal().iter().skip(1) {
            write!(&mut print_str, "connect {}; ", traversal).unwrap();
        }
        writeln!(&mut print_str, "backdoor;").unwrap();
    }

    if print_str == "\n" {
        ns.tprint("No machines to backdoor.");
    }
    else {
        ns.tprint(&*print_str);
    }
}
