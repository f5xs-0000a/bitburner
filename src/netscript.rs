use std::sync::Mutex;

use js_sys::JsString;
use wasm_bindgen::{
    prelude::*,
    JsValue,
};

// thank you github.com/paulcdejean
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen]
    pub fn alert(msg: &str);

    pub type NS;

    #[wasm_bindgen(method)]
    fn tprint(
        this: &NS,
        print: &str,
    );

    #[wasm_bindgen(method)]
    fn print(
        this: &NS,
        print: &str,
    );

    #[wasm_bindgen(method)]
    fn ls(
        this: &NS,
        machine: &str,
    ) -> Vec<JsValue>;

    #[wasm_bindgen(method)]
    async fn sleep(
        this: &NS,
        millis: i32,
    );

    #[wasm_bindgen(method)]
    fn clearLog(this: &NS);

    #[wasm_bindgen(method)]
    fn isRunning(
        this: &NS,
        pid: i32,
    ) -> bool;

    #[wasm_bindgen(method)]
    fn getScriptName(this: &NS) -> JsValue;

    #[wasm_bindgen(catch, method, variadic)]
    fn exec(
        this: &NS,
        script_name: &str,
        host: &str,
        num_threads: Option<i32>,
        args: Box<[JsString]>,
    ) -> Result<i32, JsValue>;

    #[wasm_bindgen(method)]
    fn kill(
        this: &NS,
        pid: i32,
    ) -> bool;

    #[wasm_bindgen(method)]
    fn scan(
        this: &NS,
        scan: Option<&str>,
    ) -> Vec<JsValue>;

    #[wasm_bindgen(method)]
    async fn hack(
        this: &NS,
        machine: &str,
    );

    #[wasm_bindgen(method)]
    async fn grow(
        this: &NS,
        machine: &str,
    );

    #[wasm_bindgen(method)]
    async fn weaken(
        this: &NS,
        machine: &str,
    );

    #[wasm_bindgen(catch, method)]
    fn nuke(
        this: &NS,
        host: &str,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(catch, method)]
    fn brutessh(
        this: &NS,
        hostname: &str,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(catch, method)]
    fn ftpcrack(
        this: &NS,
        hostname: &str,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(catch, method)]
    fn relaysmtp(
        this: &NS,
        hostname: &str,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(catch, method)]
    fn httpworm(
        this: &NS,
        hostname: &str,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(catch, method)]
    fn sqlinject(
        this: &NS,
        hostname: &str,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(method)]
    fn getServer(
        this: &NS,
        host: Option<&str>,
    ) -> Server;

    #[wasm_bindgen(method)]
    fn getHackingLevel(this: &NS) -> i32;

    #[wasm_bindgen(method)]
    fn getHackTime(
        this: &NS,
        host: &str,
    ) -> f64;

    #[wasm_bindgen(method)]
    fn getServerMaxRam(
        this: &NS,
        host: &str,
    ) -> f64;

    #[wasm_bindgen(method)]
    fn getServerUsedRam(
        this: &NS,
        host: &str,
    ) -> f64;

    #[wasm_bindgen(method)]
    fn getServerSecurityLevel(
        this: &NS,
        host: &str,
    ) -> f64;

    #[wasm_bindgen(method)]
    fn hackAnalyze(
        this: &NS,
        host: &str,
    ) -> f64;

    #[wasm_bindgen(method)]
    fn hackAnalyzeChance(
        this: &NS,
        host: &str,
    ) -> f64;

    #[wasm_bindgen(method)]
    fn write(
        this: &NS,
        filename: &str,
        data: &str,
        mode: char,
    );

    #[wasm_bindgen(method)]
    fn scp(
        this: &NS,
        file: &str,
        destination: &str,
        source: &str,
    ) -> bool;

    #[wasm_bindgen(method)]
    fn fileExists(
        this: &NS,
        file: &str,
        host: &str,
    ) -> bool;

    #[wasm_bindgen(method)]
    fn growthAnalyze(
        this: &NS,
        host: &str,
        growth_factor: f64,
        cores: Option<i32>,
    ) -> f64;

    #[wasm_bindgen(catch, method)]
    fn getServerMoneyAvailable(
        this: &NS,
        host: &str,
    ) -> Result<f64, JsValue>;

    #[wasm_bindgen(method)]
    fn getHostname(this: &NS) -> JsValue;

    pub type Server;

    pub type Date;

    #[wasm_bindgen(static_method_of = Date)]
    pub fn now() -> f64;
}

pub struct NsWrapper<'a>(Mutex<&'a NS>);

impl<'a> NsWrapper<'a> {
    pub fn new(ns: &'a NS) -> NsWrapper<'a> {
        NsWrapper(Mutex::new(ns))
    }

    pub fn tprint(
        &self,
        text: &str,
    ) {
        self.0.lock().unwrap().tprint(text);
    }

    pub fn print(
        &self,
        text: &str,
    ) {
        self.0.lock().unwrap().print(text);
    }

    pub fn ls(
        &self,
        hostname: &str,
    ) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .ls(hostname)
            .into_iter()
            .map(|x| x.as_string().unwrap())
            .collect::<Vec<_>>()
    }

    pub async fn sleep(
        &self,
        millis: i32, // TODO: use Duration.
    ) {
        self.0.lock().unwrap().sleep(millis).await;
    }

    pub fn clear_log(&self) {
        self.0.lock().unwrap().clearLog();
    }

    pub fn scan(
        &self,
        host: Option<&str>,
    ) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .scan(host)
            .into_iter()
            .map(|m| m.as_string().unwrap())
            .collect::<Vec<_>>()
    }

    pub fn get_server(
        &self,
        host: Option<&str>,
    ) -> Server {
        self.0.lock().unwrap().getServer(host)
    }

    pub fn nuke(
        &self,
        host: &str,
    ) -> bool {
        self.0.lock().unwrap().nuke(host).is_ok()
    }

    pub fn brute_ssh(
        &self,
        hostname: &str,
    ) -> bool {
        self.0.lock().unwrap().brutessh(hostname).is_ok()
    }

    pub fn ftp_crack(
        &self,
        hostname: &str,
    ) -> bool {
        self.0.lock().unwrap().ftpcrack(hostname).is_ok()
    }

    pub fn relay_smtp(
        &self,
        hostname: &str,
    ) -> bool {
        self.0.lock().unwrap().relaysmtp(hostname).is_ok()
    }

    pub fn http_worm(
        &self,
        hostname: &str,
    ) -> bool {
        self.0.lock().unwrap().httpworm(hostname).is_ok()
    }

    pub fn sql_inject(
        &self,
        hostname: &str,
    ) -> bool {
        self.0.lock().unwrap().sqlinject(hostname).is_ok()
    }

    pub fn get_player_hacking_level(&self) -> usize {
        self.0.lock().unwrap().getHackingLevel() as usize
    }

    pub async fn grow(
        &self,
        hostname: &str,
    ) {
        self.0.lock().unwrap().grow(hostname).await;
    }

    pub async fn hack(
        &self,
        hostname: &str,
    ) {
        self.0.lock().unwrap().hack(hostname).await;
    }

    pub async fn weaken(
        &self,
        hostname: &str,
    ) {
        self.0.lock().unwrap().weaken(hostname).await;
    }

    pub fn is_running(
        &self,
        pid: usize,
    ) -> bool {
        self.0.lock().unwrap().isRunning(pid as i32)
    }

    pub fn get_script_name(&self) -> String {
        self.0.lock().unwrap().getScriptName().as_string().unwrap()
    }

    pub fn get_hostname(&self) -> String {
        self.0.lock().unwrap().getHostname().as_string().unwrap()
    }

    pub fn exec<'b>(
        &self,
        script_name: &str,
        host: &str,
        num_threads: Option<usize>,
        args: &[impl core::ops::Deref<Target = str>],
    ) -> Result<Option<usize>, JsValue> {
        use std::str::FromStr as _;

        let args = args
            .iter()
            .map(|a| JsString::from_str(&*a).unwrap())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        match self.0.lock().unwrap().exec(
            script_name,
            host,
            num_threads.map(|x| x as i32),
            args,
        ) {
            Ok(0) => Ok(None),
            Ok(x) => Ok(Some(x as usize)),
            Err(e) => Err(e),
        }
    }

    pub fn get_hack_time(
        &self,
        hostname: &str,
    ) -> f64 {
        self.0.lock().unwrap().getHackTime(hostname)
    }

    pub fn get_server_max_ram(
        &self,
        hostname: &str,
    ) -> f64 {
        self.0.lock().unwrap().getServerMaxRam(hostname)
    }

    pub fn get_server_used_ram(
        &self,
        hostname: &str,
    ) -> f64 {
        self.0.lock().unwrap().getServerUsedRam(hostname)
    }

    pub fn get_server_security_level(
        &self,
        hostname: &str,
    ) -> f64 {
        self.0.lock().unwrap().getServerSecurityLevel(hostname)
    }

    pub fn hack_analyze(
        &self,
        hostname: &str,
    ) -> f64 {
        self.0.lock().unwrap().hackAnalyze(hostname)
    }

    pub fn hack_analyze_chance(
        &self,
        hostname: &str,
    ) -> f64 {
        self.0.lock().unwrap().hackAnalyzeChance(hostname)
    }

    pub fn write(
        &self,
        filename: &str,
        data: &str,
        mode: char,
    ) {
        self.0.lock().unwrap().write(filename, data, mode)
    }

    pub fn scp(
        &self,
        file: &str,
        destination: &str,
        source: &str,
    ) -> bool {
        self.0.lock().unwrap().scp(file, destination, source)
    }

    pub fn file_exists(
        &self,
        file: &str,
        host: &str,
    ) -> bool {
        self.0.lock().unwrap().fileExists(file, host)
    }

    pub fn kill(
        &self,
        pid: i32,
    ) -> bool {
        self.0.lock().unwrap().kill(pid)
    }

    pub fn growth_analyze(
        &self,
        host: &str,
        growth_factor: f64,
        cores: Option<i32>,
    ) -> f64 {
        self.0.lock().unwrap().growthAnalyze(host, growth_factor, cores)
    }

    pub fn get_server_money_available(
        &self,
        hostname: &str
    ) -> Result<u64, JsValue> {
        self.0.lock().unwrap().getServerMoneyAvailable(hostname).map(|val| val.round() as u64)
    }
}
