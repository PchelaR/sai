use anyhow::Result;

use crate::{attach, quote, remote};

pub fn run(dir_path: &str) -> Result<()> {
    let attached = attach::ensure(dir_path, None)?;
    remote::interactive(&format!(
        "exec docker exec -it {} bash -l",
        quote::shell(&attached.project.id())
    ))
}
