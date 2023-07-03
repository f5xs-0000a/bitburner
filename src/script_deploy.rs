use crate::{
    machine::Machine,
    netscript::NsWrapper,
};

pub struct DynamicFile<'a> {
    pub filename: &'a str,
    pub contents: &'a str,
}

impl<'a> DynamicFile<'a> {
    pub fn deploy_to_machine(
        &self,
        ns: &NsWrapper<'_>,
        machine: &Machine,
        force: bool,
    ) -> bool {
        if !force && ns.file_exists(self.filename, machine.get_hostname()) {
            return true;
        }

        let current_hostname = ns.get_hostname();
        ns.write(self.filename, self.contents, 'w');
        ns.scp(
            "child_weaken.js",
            machine.get_hostname(),
            &*current_hostname,
        );

        ns.file_exists(self.filename, machine.get_hostname())
    }
}

pub const WEAKEN_SCRIPT: DynamicFile<'static> = DynamicFile {
    filename: "child_weaken.js",
    contents: include_str!("child_weaken.js"),
};

pub const HACK_SCRIPT: DynamicFile<'static> = DynamicFile {
    filename: "child_hack.js",
    contents: include_str!("child_hack.js"),
};

pub const GROW_SCRIPT: DynamicFile<'static> = DynamicFile {
    filename: "child_grow.js",
    contents: include_str!("child_grow.js"),
};

pub enum HGW {
    Hack,
    Weaken,
    Grow,
}

impl HGW {
    pub fn script(&self) -> &'static DynamicFile<'static> {
        use HGW::*;

        match self {
            Hack => &HACK_SCRIPT,
            Weaken => &WEAKEN_SCRIPT,
            Grow => &GROW_SCRIPT,
        }
    }
}
