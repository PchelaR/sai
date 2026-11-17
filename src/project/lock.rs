use std::collections::HashSet;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::sync::Mutex;

use anyhow::Result;

use crate::{config, output};

static HELD: Mutex<Option<HashSet<String>>> = Mutex::new(None);

pub struct ProjectLock {
    name: String,
    file: Option<File>,
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        if self.file.take().is_some()
            && let Ok(mut held) = HELD.lock()
        {
            held.get_or_insert_with(HashSet::new).remove(&self.name);
        }
    }
}

pub fn hold(name: &str) -> Result<ProjectLock> {
    let already = HELD
        .lock()
        .map(|held| held.as_ref().is_some_and(|held| held.contains(name)))
        .unwrap_or(false);
    if already {
        return Ok(ProjectLock {
            name: name.to_owned(),
            file: None,
        });
    }
    let dir = config::dir()?.join("locks");
    fs::create_dir_all(&dir)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(format!("{name}.lock")))?;
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            output::info(&format!(
                "Another sai is working with {name}, waiting until it finishes"
            ));
            file.lock()?;
        }
        Err(TryLockError::Error(error)) => return Err(error.into()),
    }
    if let Ok(mut held) = HELD.lock() {
        held.get_or_insert_with(HashSet::new)
            .insert(name.to_owned());
    }
    Ok(ProjectLock {
        name: name.to_owned(),
        file: Some(file),
    })
}

#[cfg(test)]
mod tests {
    use super::hold;

    #[test]
    fn the_same_process_can_hold_a_project_twice() {
        let name = format!("lock-test-{}", std::process::id());
        let outer = hold(&name).unwrap();
        let inner = hold(&name).unwrap();
        drop(inner);
        drop(outer);
        drop(hold(&name).unwrap());
    }
}
