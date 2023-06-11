mod scan;

use clap::{error::ErrorKind::DisplayHelp, Args, Parser};
use crate::scan::ScanMode;
use std::sync::Mutex; // TODO: you might want to use async Mutex soon.
use wasm_bindgen::prelude::*;
use js_sys::{Object, Array};
use wasm_bindgen::JsValue;

// thank you github.com/paulcdejean
#[wasm_bindgen]
extern "C" {
    pub type NS;

    #[wasm_bindgen(method)]
    fn tprint(this: &NS, print: &str);
}

pub struct NsWrapper<'a>(Mutex<&'a NS>);

impl<'a> NsWrapper<'a> {
    fn new(ns: &'a NS) -> NsWrapper<'a> {
        NsWrapper(Mutex::new(ns))
    }

    fn tprint(&self, text: &str) {
        self.0.lock().unwrap().tprint(text);
    }
}

#[wasm_bindgen]
pub fn execute_command(ns: &NS, args: Array) {
    let ns = NsWrapper::new(ns);
    
    let mut strargs = vec!["run your_script.js".to_owned()];
    let strargs_iter = args
        .iter()
        .map(|a| a.as_string().unwrap());
    strargs.extend(strargs_iter);
    
    // if the message was matched, process the message
    match AppMode::try_parse_from(strargs) {
        Err(e) if e.kind() == DisplayHelp => {
            let error_msg = format!(
                "\n{}",
                clap::Error::raw(e.kind().clone(), e),
            );

            ns.tprint(&*error_msg);
        },

        Ok(AppMode::Scan(internal)) => {
            // do nothing for now
        },

        Err(e) => ns.tprint("unable to process message"),
    }
}

#[derive(Parser)]
enum AppMode {
    Scan(ScanMode),
}
