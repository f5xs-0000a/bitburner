use std::collections::VecDeque;

use wasm_bindgen::JsValue;

use crate::{
    netscript::NsWrapper,
    utils::get_attribute,
};

const GROW_TIME_MUL: f64 = 3.2;
const WEAKEN_TIME_MUL: f64 = 4.;

const WEAKEN_SECURITY_DECREASE_THOUSANDTHS: usize = 50;
const HACK_SECURITY_INCREASE_THOUSANDTHS: usize = 2;
const GROW_SECURITY_INCREASE_THOUSANDTHS: usize = 4;

pub const EXEC_MEMORY_USAGE_HUNDREDTHS: u64 = 175;

#[derive(Clone, Debug)]
pub struct Machine {
    hostname: String,
    degree: usize,
    traversal: Vec<String>,

    max_money: u64,
    player_owned: bool,
    hacking_skill: usize,
    min_security: f64,
    cpu_cores: usize,
    //hack_difficulty: f32,
    ip_address: String,
    required_open_ports: usize,
    organization_name: String,
}

impl Machine {
    /// Obtains further details about a machine.
    ///
    /// These details are constant until the next augmentation.
    fn get_further_details(
        &mut self,
        ns: &NsWrapper,
    ) {
        let server = ns.get_server(Some(self.get_hostname()));
        let as_u64 = |x: &JsValue| x.as_f64().map(|x| x as u64);
        let as_usize = |x: &JsValue| x.as_f64().map(|x| x as usize);

        self.max_money = get_attribute(&server, "moneyMax", as_u64)
            .unwrap()
            .unwrap();
        self.player_owned =
            get_attribute(&server, "purchasedByPlayer", JsValue::as_bool)
                .unwrap()
                .unwrap();
        self.hacking_skill =
            get_attribute(&server, "requiredHackingSkill", as_usize)
                .unwrap()
                .unwrap();
        self.min_security =
            get_attribute(&server, "minDifficulty", JsValue::as_f64)
                .unwrap()
                .unwrap();
        self.cpu_cores = get_attribute(&server, "cpuCores", as_usize)
            .unwrap()
            .unwrap();
        //self.hack_difficulty =
        //    get_attribute(&server, "playerOwned", as_usize)
        //      .unwrap()
        //      .unwrap();
        self.ip_address = get_attribute(&server, "ip", JsValue::as_string)
            .unwrap()
            .unwrap();
        self.required_open_ports =
            get_attribute(&server, "numOpenPortsRequired", as_usize)
                .unwrap()
                .unwrap();
        self.organization_name =
            get_attribute(&server, "organizationName", JsValue::as_string)
                .unwrap()
                .unwrap();
    }

    fn dummy() -> Machine {
        Machine {
            traversal: vec![],
            hostname: String::new(),
            degree: 0,

            max_money: 0,
            player_owned: false,
            hacking_skill: 0,
            min_security: 0.,
            cpu_cores: 0,
            //hack_difficulty: f32,
            ip_address: String::new(),
            required_open_ports: 0,
            organization_name: String::new(),
        }
    }

    pub fn home(ns: &NsWrapper) -> Machine {
        let hostname = "home".to_owned();

        let mut retval = Machine {
            traversal: vec![hostname.clone()],
            hostname,
            degree: 0,
            ..Machine::dummy()
        };

        retval.get_further_details(ns);
        retval
    }

    pub fn create_child(
        &self,
        ns: &NsWrapper,
        hostname: String,
    ) -> Machine {
        let mut traversal = self.traversal.clone();
        traversal.push(hostname.clone());

        let mut retval = Machine {
            traversal,
            hostname,
            degree: self.degree + 1,
            ..Machine::dummy()
        };

        retval.get_further_details(ns);
        retval
    }

    pub fn get_hostname(&self) -> &str {
        &*self.hostname
    }

    pub fn get_degree(&self) -> usize {
        self.degree
    }

    pub fn get_traversal(&self) -> &[String] {
        &*self.traversal
    }

    pub fn get_max_money(&self) -> u64 {
        self.max_money
    }

    pub fn is_player_owned(&self) -> bool {
        self.player_owned
    }

    pub fn get_min_hacking_skill(&self) -> usize {
        self.hacking_skill
    }

    pub fn get_min_security(&self) -> f64 {
        self.min_security
    }

    pub fn get_min_security_thousandths(&self) -> usize {
        (self.get_min_security() * 1000.).round() as usize
    }

    pub fn get_cpu_cores(&self) -> usize {
        self.cpu_cores
    }

    pub fn get_ip_address(&self) -> &str {
        &*self.ip_address
    }

    pub fn get_required_open_ports(&self) -> usize {
        self.required_open_ports
    }

    pub fn get_organization_name(&self) -> &str {
        &*self.organization_name
    }

    pub fn get_money_available(&self, ns: &NsWrapper) -> u64 {
        ns.get_server_money_available(self.get_hostname())
    }

    pub fn is_root(
        &self,
        ns: &NsWrapper,
    ) -> bool {
        let server = ns.get_server(Some(self.get_hostname()));
        get_attribute(&server, "hasAdminRights", JsValue::as_bool)
            .unwrap()
            .unwrap()
    }

    pub fn is_backdoored(
        &self,
        ns: &NsWrapper,
    ) -> bool {
        let server = ns.get_server(Some(self.get_hostname()));
        get_attribute(&server, "backdoorInstalled", JsValue::as_bool)
            .unwrap()
            .unwrap()
    }

    pub fn run_brute_ssh(
        &mut self,
        ns: &NsWrapper,
    ) -> bool {
        ns.brute_ssh(self.get_hostname())
    }

    pub fn run_ftp_crack(
        &mut self,
        ns: &NsWrapper,
    ) -> bool {
        ns.ftp_crack(self.get_hostname())
    }

    pub fn run_relay_smtp(
        &mut self,
        ns: &NsWrapper,
    ) -> bool {
        ns.relay_smtp(self.get_hostname())
    }

    pub fn run_http_worm(
        &mut self,
        ns: &NsWrapper,
    ) -> bool {
        ns.http_worm(self.get_hostname())
    }

    pub fn run_sql_inject(
        &mut self,
        ns: &NsWrapper,
    ) -> bool {
        ns.sql_inject(self.get_hostname())
    }

    pub fn nuke(
        &mut self,
        ns: &NsWrapper,
    ) -> bool {
        ns.nuke(self.get_hostname())
    }

    pub fn get_hack_time(
        &self,
        ns: &NsWrapper,
    ) -> f64 {
        ns.get_hack_time(self.get_hostname())
    }

    pub fn get_hgw_time(
        &self,
        ns: &NsWrapper,
    ) -> (f64, f64, f64) {
        let hack_time = self.get_hack_time(ns);
        (
            hack_time,
            hack_time * GROW_TIME_MUL,
            hack_time * WEAKEN_TIME_MUL,
        )
    }

    pub fn get_grow_time(
        &self,
        ns: &NsWrapper,
    ) -> f64 {
        self.get_hgw_time(ns).1
    }

    pub fn get_weaken_time(
        &self,
        ns: &NsWrapper,
    ) -> f64 {
        self.get_hgw_time(ns).2
    }

    pub fn get_max_gb_ram(
        &self,
        ns: &NsWrapper,
    ) -> f64 {
        ns.get_server_max_ram(self.get_hostname())
    }

    pub fn get_used_gb_ram(
        &self,
        ns: &NsWrapper,
    ) -> f64 {
        ns.get_server_used_ram(self.get_hostname())
    }

    pub fn get_max_gb_ram_hundredths(
        &self,
        ns: &NsWrapper,
    ) -> u64 {
        (self.get_max_gb_ram(ns) * 100.).round() as u64
    }

    pub fn get_used_gb_ram_hundredths(
        &self,
        ns: &NsWrapper,
    ) -> u64 {
        (self.get_used_gb_ram(ns) * 100.).round() as u64
    }

    pub fn get_free_ram_hundredths(
        &self,
        ns: &NsWrapper,
    ) -> u64 {
        self.get_max_gb_ram_hundredths(ns) - self.get_used_gb_ram_hundredths(ns)
    }

    pub fn get_security_level(
        &self,
        ns: &NsWrapper,
    ) -> f64 {
        ns.get_server_security_level(self.get_hostname())
    }

    pub fn get_security_level_thousandths(
        &self,
        ns: &NsWrapper,
    ) -> usize {
        (self.get_security_level(ns) * 1000.).round() as usize
    }

    /// Returns the number of `weaken()` threads
    pub fn get_weaken_threads_to_reduce(
        &self,
        ns: &NsWrapper,
    ) -> usize {
        let security_left = self.get_security_level_thousandths(ns)
            - self.get_min_security_thousandths();
        let mut weaken_threads_left =
            security_left / WEAKEN_SECURITY_DECREASE_THOUSANDTHS;

        // add one for remainders
        if 0 < security_left % WEAKEN_SECURITY_DECREASE_THOUSANDTHS {
            weaken_threads_left += 1;
        }

        weaken_threads_left
    }

    pub fn get_threads_left(
        &self,
        ns: &NsWrapper,
    ) -> u64 {
        self.get_free_ram_hundredths(ns) / EXEC_MEMORY_USAGE_HUNDREDTHS
    }

    pub fn get_hack_rate(
        &self,
        ns: &NsWrapper,
    ) -> f64 {
        ns.hack_analyze(self.get_hostname())
    }

    pub fn get_yield_per_hack(
        &self,
        ns: &NsWrapper,
    ) -> f64 {
        self.get_max_money() as f64 * self.get_hack_rate(ns)
    }

    pub fn get_hack_chance(
        &self,
        ns: &NsWrapper,
    ) -> f64 {
        ns.hack_analyze_chance(self.get_hostname())
    }

    pub fn get_average_yield(
        &self,
        ns: &NsWrapper,
    ) -> f64 {
        self.get_yield_per_hack(ns) * self.get_hack_chance(ns)
    }
}

pub fn get_machines(ns: &NsWrapper) -> Vec<Machine> {
    let mut traversed: Vec<Machine> = vec![];
    let mut pending = VecDeque::new();
    pending.push_front(Machine::home(ns));

    while let Some(machine) = pending.pop_back() {
        // put this node into the list of traversed machines
        for child_name in ns.scan(Some(machine.get_hostname())) {
            // don't consider machines that are already found
            let found_already =
                traversed.iter().any(|t| t.get_hostname() == child_name);
            if found_already {
                continue;
            }

            pending.push_front(machine.create_child(ns, child_name));
        }

        // put this node into the list of traversed machines
        traversed.push(machine);
    }

    traversed
}
