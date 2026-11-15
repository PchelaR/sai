use anyhow::Result;

use crate::config::Config;
use crate::project::Registry;
use crate::{attach, quote, remote, ssh};

const WELCOME: &str = "printf '%s\\n' 'You are on the VPS as the sai user, without sudo.' 'Projects: ~/projects   snapshots: ~/snapshots   containers: docker ps' 'Type exit or /exit to return to sai.' ''";
const RC: &str = "printf '%s\\n' '[ -f ~/.bashrc ] && . ~/.bashrc' 'function /exit { exit 0; }'";

pub fn run(name: Option<&str>) -> Result<()> {
    let mut registry = Registry::open()?;
    let name = registry.resolve(name)?.name.clone();
    let attached = attach::ensure(&mut registry, &name, None)?;
    remote::interactive(&format!(
        "exec docker exec -it {} bash -l",
        quote::shell(&attached.project.id())
    ))
}

pub fn vps() -> Result<()> {
    ssh::install_host_alias(&Config::load()?)?;
    remote::interactive(&format!(
        "cd ~; {WELCOME}; bash --rcfile <({RC}) -i; exit 0"
    ))
}
