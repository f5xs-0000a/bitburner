mod autohack;
mod machine;
mod netscript;
mod scan;
mod time_consts;
mod utils;
mod script_deploy;
//mod contracts;
mod event_pool;
//mod range_set;

use clap::{
    error::ErrorKind::DisplayHelp,
    Args,
    Parser,
};
use js_sys::Array;
// TODO: don't use glob unless necessary.
use wasm_bindgen::prelude::*;

use crate::scan::ScanMode;

#[derive(Args)]
struct HGWTarget {
    inner: String,
}

#[derive(Parser)]
enum AppMode {
    //#[clap(help = "scans the network")]
    Scan(ScanMode),
    //#[clap(help = "perform automated hacking on the network")]
    AutoHack,
    //#[clap(help = "automatically solve a contract")]
    Contract,
}

#[wasm_bindgen]
pub async fn execute_command(
    ns: &crate::netscript::NS,
    args: Array,
) {
    let ns = crate::netscript::NsWrapper::new(ns);

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

        Ok(AppMode::Scan(scan_mode)) => scan_mode.execute(&ns),

        Ok(AppMode::AutoHack) => crate::autohack::auto_hack(&ns).await,

        Ok(AppMode::Contract) => ns.tprint("Not yet implemented."),

        Err(e) => ns.tprint(&format!("unable to process message:\n{}", e)),
    }
}

#[macro_export]
macro_rules! debug {
    ($ns: expr) => {
        //crate::netscript::alert(&*format!("{}:{}", file!(), line!()));
        $ns.tprint(&*format!("{}:{}", file!(), line!()));
    };
    ($ns: expr, $($arg:tt)*) => {
        let mut first = format!("{}:{}\n", file!(), line!());
        first += &*format!($($arg)*);
        //crate::netscript::alert(&*first);
        $ns.tprint(&*first);
    };
}
