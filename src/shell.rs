use anyhow::Result;

use crate::agent::Agent;
use crate::{attach, quote, remote};

pub fn run(dir_path: &str, agent: Agent) -> Result<()> {
    let project = attach::ensure(dir_path, agent)?;
    remote::interactive(&format!(
        "exec docker exec -it {} bash -l",
        quote::shell(&project.id())
    ))
}
