use anyhow::Result;

use super::RUN;
use crate::project::Registry;
use crate::{output, remote, tools};

pub struct Change {
    pub kind: String,
    pub path: String,
}

pub fn unreviewed(name: &str) -> Result<Vec<Change>> {
    let text = remote::output(RUN, &[&tools::snapshots_path(), "watched", name])?;
    Ok(parse(&text))
}

pub fn mark(name: &str) -> Result<()> {
    remote::run(RUN, &[&tools::snapshots_path(), "reviewed", name])
}

pub fn run(name: Option<&str>) -> Result<()> {
    let registry = Registry::open()?;
    let name = registry.resolve(name)?.name.clone();
    let changes = unreviewed(&name)?;
    if changes.is_empty() {
        output::info(&format!("Nothing to review in {name}"));
        return Ok(());
    }
    output::info(&format!(
        "Files in {name} that can run code on your computer changed since your last review:"
    ));
    for line in lines(&changes) {
        output::info(&line);
    }
    output::info(
        "Look at them before you run, build or commit anything. They are marked as reviewed now",
    );
    mark(&name)
}

pub fn after_chat(name: &str) {
    if super::take(name).is_err() {
        return;
    }
    let Ok(changes) = unreviewed(name) else {
        return;
    };
    if changes.is_empty() {
        return;
    }
    output::warning(&format!(
        "{} file(s) that can run code on your computer changed, check them before you run anything:",
        changes.len()
    ));
    for line in lines(&changes) {
        output::info(&line);
    }
    output::info(&format!("Mark them as reviewed with: sai review {name}"));
}

pub fn lines(changes: &[Change]) -> Vec<String> {
    changes
        .iter()
        .map(|change| format!("  {:<8} {}", change.kind, change.path))
        .collect()
}

fn parse(text: &str) -> Vec<Change> {
    let mut changes: Vec<Change> = Vec::new();
    for line in text.lines() {
        let mut fields = line.splitn(3, '\t');
        let (Some(_), Some(kind), Some(path)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        changes.retain(|change| change.path != path);
        changes.push(Change {
            kind: kind.to_owned(),
            path: path.to_owned(),
        });
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn keeps_the_latest_change_of_each_file() {
        let changes = parse(
            "20261116T100000Z\tadded\tfront/.husky/pre-commit\n20261116T100300Z\tchanged\tfront/package.json\n20261116T100600Z\tchanged\tfront/.husky/pre-commit\nbroken line\n",
        );
        let summary: Vec<(&str, &str)> = changes
            .iter()
            .map(|change| (change.kind.as_str(), change.path.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                ("changed", "front/package.json"),
                ("changed", "front/.husky/pre-commit")
            ]
        );
    }
}
