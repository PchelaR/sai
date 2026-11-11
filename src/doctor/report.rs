use std::fmt::Display;

use anyhow::{Result, bail};

use crate::output;

#[derive(Default)]
pub struct Report {
    failures: usize,
}

impl Report {
    pub fn section(&self, title: &str) {
        output::info("");
        output::info(title);
    }

    pub fn check<T>(&mut self, name: &str, result: Result<T>) -> Option<T> {
        match result {
            Ok(value) => {
                self.ok(name);
                Some(value)
            }
            Err(error) => {
                self.fail(name, format!("{error:#}"));
                None
            }
        }
    }

    pub fn ok(&self, name: &str) {
        output::info(&format!("  ok    {name}"));
    }

    pub fn warn(&self, name: &str, detail: impl Display) {
        output::info(&format!("  warn  {name}: {detail}"));
    }

    pub fn fail(&mut self, name: &str, detail: impl Display) {
        self.failures += 1;
        output::info(&format!("  FAIL  {name}: {detail}"));
    }

    pub fn finish(self) -> Result<()> {
        output::info("");
        match self.failures {
            0 => {
                output::info("Everything looks good");
                Ok(())
            }
            count => bail!("found {count} problem(s)"),
        }
    }
}
