mod donut;
mod machine;
mod scan;
mod utils;

use std::sync::Mutex; // TODO: you might want to use async Mutex soon.

use clap::{
    error::ErrorKind::DisplayHelp,
    Parser,
};
use js_sys::Array;
use wasm_bindgen::{
    prelude::*,
    JsValue,
};

use crate::scan::ScanMode;

// thank you github.com/paulcdejean
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen]
    fn alert(msg: &str);

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
    async fn sleep(
        this: &NS,
        millis: i32,
    );

    #[wasm_bindgen(method)]
    fn clearLog(this: &NS);

    #[wasm_bindgen(method)]
    fn scan(
        this: &NS,
        scan: Option<&str>,
    ) -> Vec<JsValue>;

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

    pub type Server;
}

pub struct NsWrapper<'a>(Mutex<&'a NS>);

impl<'a> NsWrapper<'a> {
    fn new(ns: &'a NS) -> NsWrapper<'a> {
        NsWrapper(Mutex::new(ns))
    }

    fn tprint(
        &self,
        text: &str,
    ) {
        self.0.lock().unwrap().tprint(text);
    }

    fn print(
        &self,
        text: &str,
    ) {
        self.0.lock().unwrap().print(text);
    }

    async fn sleep(
        &self,
        millis: i32, // TODO: use Duration.
    ) {
        self.0.lock().unwrap().sleep(millis).await;
    }

    fn clear_log(&self) {
        self.0.lock().unwrap().clearLog();
    }

    fn scan(
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

    fn get_server(
        &self,
        host: Option<&str>,
    ) -> Server {
        self.0.lock().unwrap().getServer(host)
    }

    fn nuke(
        &self,
        host: &str,
    ) -> bool {
        self.0.lock().unwrap().nuke(host).is_ok()
    }

    fn brute_ssh(
        &self,
        hostname: &str,
    ) -> bool {
        self.0.lock().unwrap().brutessh(hostname).is_ok()
    }

    fn ftp_crack(
        &self,
        hostname: &str,
    ) -> bool {
        self.0.lock().unwrap().ftpcrack(hostname).is_ok()
    }

    fn relay_smtp(
        &self,
        hostname: &str,
    ) -> bool {
        self.0.lock().unwrap().relaysmtp(hostname).is_ok()
    }

    fn http_worm(
        &self,
        hostname: &str,
    ) -> bool {
        self.0.lock().unwrap().httpworm(hostname).is_ok()
    }

    fn sql_inject(
        &self,
        hostname: &str,
    ) -> bool {
        self.0.lock().unwrap().sqlinject(hostname).is_ok()
    }
}

#[wasm_bindgen]
pub async fn execute_command(
    ns: &NS,
    args: Array,
) {
    let ns = NsWrapper::new(ns);

    let mut strargs = vec!["run your_script.js".to_owned()];
    let strargs_iter = args.iter().map(|a| a.as_string().unwrap());
    strargs.extend(strargs_iter);

    // if the message was matched, process the message
    match AppMode::try_parse_from(strargs) {
        Err(e) if e.kind() == DisplayHelp => {
            let error_msg =
                format!("\n{}", clap::Error::raw(e.kind().clone(), e),);

            ns.tprint(&*error_msg);
        },

        Ok(AppMode::Scan(scan_mode)) => {
            //crate::alert(&format!("{}:{} - {:?}", file!(), line!(), ()));
            scan_mode.execute(&ns)
        },

        Ok(AppMode::Donut) => crate::donut::donut(&ns).await,

        Err(e) => ns.tprint(&format!("unable to process message:\n{}", e)),
    }
}

#[derive(Parser)]
enum AppMode {
    Scan(ScanMode),
    Donut,
}
