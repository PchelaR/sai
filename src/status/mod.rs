use anyhow::Result;

use crate::remote;

const STATUS: &str = include_str!("scripts/status.sh");

pub fn run() -> Result<()> {
    remote::run(STATUS, &[])
}
