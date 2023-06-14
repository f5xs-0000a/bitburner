mod autohack;
mod machine;
mod netscript;
mod scan;
mod utils;

use clap::{
    error::ErrorKind::DisplayHelp,
    Args,
    Parser,
};
use js_sys::Array;
// TODO: don't use glob unless necessary.
use wasm_bindgen::{
    prelude::*,
};

use crate::scan::ScanMode;

#[derive(Args)]
struct HGWTarget {
    inner: String,
}

#[derive(Parser)]
enum AppMode {
    //#[clap(help = "scans the network")]
    Scan(ScanMode),
    //#[clap(help = "grow on a machine")]
    Grow(HGWTarget),
    //#[clap(help = "weaken a machine")]
    Weaken(HGWTarget),
    //#[clap(help = "hack a machine")]
    Hack(HGWTarget),
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

        // hgw section
        Ok(AppMode::Grow(hostname)) => ns.grow(&*hostname.inner).await,
        Ok(AppMode::Weaken(hostname)) => ns.weaken(&*hostname.inner).await,
        Ok(AppMode::Hack(hostname)) => ns.hack(&*hostname.inner).await,

        Ok(AppMode::AutoHack) => crate::autohack::auto_hack(&ns).await,

        Ok(AppMode::Contract) => ns.tprint("Not yet implemented."),

        Err(e) => ns.tprint(&format!("unable to process message:\n{}", e)),
    }
}
