use std::fmt::Display;

use anyhow::{Result, bail};

#[derive(Default)]
pub struct Report {
    failures: usize,
}

impl Report {
    pub fn section(&self, title: &str) {
        println!("\n{title}");
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
        println!("  ok    {name}");
    }

    pub fn warn(&self, name: &str, detail: impl Display) {
        println!("  warn  {name}: {detail}");
    }

    pub fn fail(&mut self, name: &str, detail: impl Display) {
        self.failures += 1;
        println!("  FAIL  {name}: {detail}");
    }

    pub fn finish(self) -> Result<()> {
        println!();
        match self.failures {
            0 => {
                println!("Everything looks good");
                Ok(())
            }
            count => bail!("found {count} problem(s)"),
        }
    }
}
