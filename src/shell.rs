use anyhow::Result;

use crate::project::Registry;
use crate::{attach, quote, remote};

pub fn run(name: Option<&str>) -> Result<()> {
    let mut registry = Registry::open()?;
    let name = registry.resolve(name)?.name.clone();
    let attached = attach::ensure(&mut registry, &name, None)?;
    remote::interactive(&format!(
        "exec docker exec -it {} bash -l",
        quote::shell(&attached.project.id())
    ))
}
