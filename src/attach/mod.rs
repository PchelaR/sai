use std::collections::{BTreeSet, HashSet};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use dialoguer::{Confirm, Select};

use crate::agent::Agent;
use crate::config::{self, Config};
use crate::project::content::{self, Selection};
use crate::project::{self, Folder, Project, Registry, SyncTarget, repos};
use crate::vps::SETUP_MARKER;
use crate::{fingerprint, image, mutagen, output, remote, secrets, snapshot, ssh, tools};

const PREPARE_PROJECT: &str = include_str!("scripts/prepare_project.sh");
const REMOVE_SECRETS: &str = include_str!("scripts/remove_secrets.sh");
const ENSURE_IMAGE: &str = concat!(
    include_str!("../image/build.sh"),
    include_str!("scripts/ensure_image.sh")
);
const ENSURE_CONTAINER: &str = include_str!("scripts/ensure_container.sh");
const FREE_SPACE: &str = include_str!("scripts/free_space.sh");
const GIT_CONFIG: &str = include_str!("scripts/git_config.sh");
const OLD_CHATS: &str = include_str!("scripts/old_chats.sh");
const SHOWN_REPOS: usize = 8;
const GIT_WARNING: &str = "If secrets were ever committed, the agent can read them in the history";
const RESERVE: u64 = 2 * 1024 * 1024 * 1024;

pub type Choose = dyn Fn(&Path) -> Result<Option<Selection>>;

pub struct Attached {
    pub project: Project,
    pub agent: Agent,
}

pub fn run(
    name: &str,
    dir_path: &str,
    agent: Option<Agent>,
    alias: Option<&str>,
    choose: &Choose,
    approve_git: &dyn Fn(&Path, &[PathBuf]) -> Result<bool>,
) -> Result<()> {
    let _lock = project::lock::hold(name)?;
    let mut registry = Registry::open()?;
    let Some((folder, path)) = register(
        &mut registry,
        name,
        dir_path,
        agent,
        alias,
        choose,
        approve_git,
    )?
    else {
        output::info("Nothing was attached");
        return Ok(());
    };
    let attached = ensure(&mut registry, name, agent)?;
    output::info(&format!(
        "{} is attached to {name} as /workspace/{folder}, the agent is {}. Talk to it with: sai chat {name}",
        path.display(),
        attached.agent
    ));
    Ok(())
}

pub fn register(
    registry: &mut Registry,
    name: &str,
    dir_path: &str,
    agent: Option<Agent>,
    alias: Option<&str>,
    choose: &Choose,
    approve_git: &dyn Fn(&Path, &[PathBuf]) -> Result<bool>,
) -> Result<Option<(String, PathBuf)>> {
    let path = project::local_path(dir_path)?;
    project::ensure_attachable(&path)?;
    let folder = match alias {
        Some(alias) => alias.to_owned(),
        None => project::folder_name(&path)?,
    };
    match registry.find(name) {
        None => registry.create(name, agent.unwrap_or_default())?,
        Some(existing) => {
            if let (Some(requested), Some(bound)) = (agent, existing.agent)
                && requested != bound
            {
                bail!("{name} is bound to {bound}, switch it with `sai change {requested} {name}`");
            }
        }
    }
    let already = registry
        .get(name)?
        .folders
        .iter()
        .find(|existing| project::same_path(&existing.path, &path))
        .map(|existing| (existing.name.clone(), existing.git));
    let folder = match already {
        Some((existing, git)) => {
            if !git && wants_git(&path, approve_git)? {
                registry.set_git(name, &path, true)?;
                registry.save()?;
            }
            existing
        }
        None => {
            let git = wants_git(&path, approve_git)?;
            let Some(skip) = pick(&path, choose)? else {
                return Ok(None);
            };
            registry.add_folder(
                name,
                Folder {
                    name: folder.clone(),
                    path: path.clone(),
                    git,
                    skip,
                    history: Vec::new(),
                },
            )?;
            registry.save()?;
            folder
        }
    };
    Ok(Some((folder, path)))
}

pub fn current_folder(
    registry: &mut Registry,
    agent: Option<Agent>,
    choose: &Choose,
    approve_git: &dyn Fn(&Path, &[PathBuf]) -> Result<bool>,
) -> Result<String> {
    if let Ok(project) = registry.resolve(None) {
        return Ok(project.name.clone());
    }
    let path = project::current_dir()?;
    project::ensure_attachable(&path).map_err(|_| {
        anyhow::anyhow!("run sai inside a project folder or name the project, see `sai list`")
    })?;
    let name = project::folder_name(&path)?;
    ensure!(
        registry.find(&name).is_none(),
        "{} is not attached yet and a project named {name} already exists. Add the folder to it with `sai attach {name} .` or create another project with `sai attach <new-name> .`",
        path.display()
    );
    let git = wants_git(&path, approve_git)?;
    let skip = pick(&path, choose)?.context("nothing was attached")?;
    registry.create(&name, agent.unwrap_or_default())?;
    registry.add_folder(
        &name,
        Folder {
            name: name.clone(),
            path: path.clone(),
            git,
            skip,
            history: Vec::new(),
        },
    )?;
    registry.save()?;
    output::info(&format!(
        "Created project {name} with {}. Add more folders with: sai attach {name} <folder>",
        path.display()
    ));
    Ok(name)
}

pub fn ensure(registry: &mut Registry, name: &str, requested: Option<Agent>) -> Result<Attached> {
    let _lock = project::lock::hold(name)?;
    let config = Config::load()?;
    ssh::install_host_alias(&config)?;
    mutagen::ensure_installed()?;
    let id = project::id(name);
    let remote_path = project::remote_path(name);

    output::step("Preparing the VPS");
    let prepared = remote::output_all(
        "prepare the project",
        &[
            (
                PREPARE_PROJECT,
                vec![SETUP_MARKER.to_owned(), id.clone(), remote_path.clone()],
            ),
            tools::install_step(&config::instructions()?, &config.agents)?,
        ],
    )?;
    let prepared = Prepared::parse(&prepared);
    let label = prepared.agent.as_str();
    forget_other_vps(registry, name, &prepared)?;
    let project = registry.get(name)?.clone();
    let retired = project.retired_agent.clone().or_else(|| {
        (project.agent.is_none() && !label.is_empty() && Agent::parse(label).is_none())
            .then(|| label.to_owned())
    });
    if let (Some(old), None) = (retired, requested) {
        bail!(
            "{name} used {old}, which sai no longer supports. Switch it with `sai change claude {name}` or `sai change codex {name}`, your files stay as they are"
        );
    }
    let agent = choose_agent(name, requested, project.agent.or(Agent::parse(label)))?;
    if project.agent != Some(agent) {
        registry.set_agent(name, agent)?;
        registry.save()?;
    }

    let fresh: Vec<String> = project
        .folders
        .iter()
        .filter(|folder| mutagen::needs_upload(&project.target(folder)).unwrap_or(true))
        .map(|folder| folder.name.clone())
        .collect();
    for target in project.targets() {
        mutagen::sync(&target)?;
    }
    let git_targets = project.git_targets();
    for target in &git_targets {
        mutagen::sync_git(target)?;
    }
    drop_stale_git(&project, &git_targets)?;
    history_notice(registry, &project)?;
    let owned = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    let mut steps = Vec::new();
    if !git_targets.is_empty() {
        let paths: Vec<String> = git_targets
            .iter()
            .map(|target| target.remote.clone())
            .collect();
        steps.push((GIT_CONFIG, paths));
    }
    let mut secret_args = vec![remote_path.as_str()];
    secret_args.extend(secrets::names());
    steps.push((REMOVE_SECRETS, owned(&secret_args)));
    steps.push(snapshot::take_after_upload_step(
        name,
        &prepared.now,
        &fresh,
    ));
    let image = image::agent(agent);
    steps.push((
        ENSURE_IMAGE,
        owned(&[
            &image::base(),
            &image,
            &image::encoded_agent(agent),
            agent.name(),
            &image::encoded_base(),
        ]),
    ));
    let mounts = tools::read_only_mounts(agent);
    let spec = fingerprint::of(&format!("{ENSURE_CONTAINER}{}", mounts.join("\n")));
    let mut container_args = owned(&[
        id.as_str(),
        remote_path.as_str(),
        name,
        image.as_str(),
        spec.as_str(),
        agent.name(),
        agent.login_files(),
        agent.setting_files(),
    ]);
    container_args.extend(mounts);
    steps.push((ENSURE_CONTAINER, container_args));
    output::step("Saving a snapshot and starting the agent container");
    remote::run_all("prepare the agent container", &steps)?;

    for target in project.targets().iter().chain(&git_targets) {
        mutagen::report_issues(&target.id)?;
    }
    let project = registry.get(name)?.clone();
    Ok(Attached { project, agent })
}

/// sai before 0.1.9 kept the chats of all projects in one Docker volume per agent, and
/// the upgrade left them there. Nobody knows which chat belongs to which project, so the
/// user decides once: move them into this project, or delete them. Asks in the terminal,
/// so only `sai chat` calls it, never a task running under the menu
pub fn old_chats(name: &str, agent: Agent) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        return Ok(());
    }
    let image = image::agent(agent);
    let image = image.as_str();
    let listing = remote::output(OLD_CHATS, &["list", image, agent.name(), name])?;
    for line in listing.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let ["volume", owner, count] = fields[..] else {
            continue;
        };
        let count: usize = count.trim().parse().unwrap_or_default();
        match Agent::parse(owner) {
            None => {
                let delete = Confirm::new()
                    .with_prompt(format!(
                        "The VPS still keeps the data of {owner} ({count} chats), sai no longer supports it. Delete it?"
                    ))
                    .default(false)
                    .interact()?;
                if delete {
                    remote::run(OLD_CHATS, &["delete", image, owner, name])?;
                }
            }
            Some(other) if other != agent => {}
            Some(_) if count == 0 => {
                remote::run(OLD_CHATS, &["delete", image, owner, name])?;
            }
            Some(_) => {
                let options = vec![
                    format!("Move them into {name}"),
                    "Delete them".to_owned(),
                    "Ask me next time".to_owned(),
                ];
                let choice = Select::new()
                    .with_prompt(format!(
                        "{count} {agent} chats from an older sai are kept on the VPS, from all your projects together. Move them into {name}? Its agent will see all of them"
                    ))
                    .items(&options)
                    .default(2)
                    .interact()?;
                match choice {
                    0 => {
                        remote::run(OLD_CHATS, &["move", image, owner, name])?;
                        output::info(&format!("Moved, they are in the chat list of {name}"));
                    }
                    1 => remote::run(OLD_CHATS, &["delete", image, owner, name])?,
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

pub fn ask_new_items(path: &Path) -> Result<Option<Selection>> {
    ask_items(path, None)
}

pub fn ask_items(path: &Path, skip: Option<Vec<String>>) -> Result<Option<Selection>> {
    output::step(&format!("Measuring {}", path.display()));
    let tree = content::scan(path);
    let mut selected: BTreeSet<String> = match skip {
        Some(skip) => skip.into_iter().collect(),
        None => tree.suggested_skip(),
    };
    let children: Vec<&content::Entry> = tree.selectable().collect();
    if children.is_empty() {
        return Ok(Some(tree.selection(&selected)));
    }
    let labels: Vec<String> = children
        .iter()
        .map(|child| content::label(child, &selected))
        .collect();
    let defaults: Vec<bool> = children
        .iter()
        .map(|child| content::check(&child.path, &selected) != content::Check::Off)
        .collect();
    let chosen = dialoguer::MultiSelect::new()
        .with_prompt(format!(
            "Choose what to upload from {} ({} in total). Space toggles, Enter confirms. What git ignores is unchecked, virtual environments, node_modules and secrets are never uploaded",
            path.display(),
            project::size::human(tree.size)
        ))
        .items(&labels)
        .defaults(&defaults)
        .interact()?;
    for (index, child) in children.iter().enumerate() {
        let wanted = chosen.contains(&index);
        let uploaded = content::check(&child.path, &selected) != content::Check::Off;
        if wanted != uploaded {
            content::toggle(&tree, &mut selected, &child.path);
        }
    }
    if tree.nothing_selected(&selected) {
        return Ok(None);
    }
    Ok(Some(tree.selection(&selected)))
}

fn wants_git(path: &Path, approve_git: &dyn Fn(&Path, &[PathBuf]) -> Result<bool>) -> Result<bool> {
    let found = repos::find(path);
    Ok(!found.is_empty() && approve_git(path, &found)?)
}

/// Stops uploading the history the agent should no longer get (a repository the user
/// stopped uploading or did not pick) and deletes its copy on the VPS, so the agent
/// cannot keep reading it
pub fn drop_stale_git(project: &Project, current: &[SyncTarget]) -> Result<()> {
    let current: HashSet<&str> = current.iter().map(|target| target.id.as_str()).collect();
    for folder in &project.folders {
        let stale: HashSet<String> = project
            .git_session_ids(folder)?
            .into_iter()
            .filter(|id| !current.contains(id.as_str()))
            .collect();
        if stale.is_empty() {
            continue;
        }
        let copies: Vec<String> = repos::find(&folder.path)
            .iter()
            .filter(|relative| stale.contains(&project.git_target(folder, relative).id))
            .map(|relative| repos::git_dir(relative))
            .collect();
        for id in &stale {
            mutagen::drop_if_present(id)?;
        }
        project::files::remove_remote(&project.target(folder).remote, &copies)?;
    }
    Ok(())
}

/// Tells once which repositories get history when a folder has more than repos::LIMIT:
/// the default pick is saved, so the next chats stay quiet until the user changes it
fn history_notice(registry: &mut Registry, project: &Project) -> Result<()> {
    let mut changed = false;
    for folder in project.folders.iter().filter(|folder| folder.git) {
        let found = folder.repos();
        if found.len() <= repos::LIMIT || !folder.history.is_empty() {
            continue;
        }
        let labels: Vec<String> = found
            .iter()
            .map(|relative| repos::label(relative))
            .collect();
        let (picked, rest) = labels.split_at(repos::LIMIT);
        output::warning(&format!(
            "{} has {} git repositories, the agent gets the history of {} of them: {}. Without history: {}. Choose which with `sai history {}` (in the menu: h)",
            folder.path.display(),
            labels.len(),
            repos::LIMIT,
            shorten(picked),
            shorten(rest),
            project.name
        ));
        registry.set_history(&project.name, &folder.path, picked.to_vec())?;
        changed = true;
    }
    if changed {
        registry.save()?;
    }
    Ok(())
}

fn shorten(labels: &[String]) -> String {
    let mut shown: Vec<String> = labels.iter().take(SHOWN_REPOS).cloned().collect();
    if labels.len() > SHOWN_REPOS {
        shown.push(format!("and {} more", labels.len() - SHOWN_REPOS));
    }
    shown.join(", ")
}

pub fn ask_git(path: &Path, found: &[PathBuf]) -> Result<bool> {
    Ok(dialoguer::Confirm::new()
        .with_prompt(git_question(path, found))
        .default(false)
        .interact()?)
}

pub fn git_question_many(folders: &[(PathBuf, Vec<PathBuf>)]) -> String {
    if let [(path, found)] = folders {
        return git_question(path, found);
    }
    let names: Vec<String> = folders
        .iter()
        .map(|(path, _)| path.display().to_string())
        .collect();
    format!(
        "{} have git repositories. Give the agent their history read-only (git log, diff, branches)? {GIT_WARNING}",
        names.join(", ")
    )
}

pub fn git_question(path: &Path, found: &[PathBuf]) -> String {
    if let [only] = found
        && only.as_os_str().is_empty()
    {
        return format!(
            "{} is a git repository. Give the agent its history read-only (git log, diff, branches)? {GIT_WARNING}",
            path.display()
        );
    }
    let mut names: Vec<String> = found
        .iter()
        .take(SHOWN_REPOS)
        .map(|relative| repos::label(relative))
        .collect();
    if found.len() > SHOWN_REPOS {
        names.push(format!("and {} more", found.len() - SHOWN_REPOS));
    }
    let limit = if found.len() > repos::LIMIT {
        format!(
            " The agent gets the history of {} of them, you can choose which later with `sai history`.",
            repos::LIMIT
        )
    } else {
        String::new()
    };
    format!(
        "{} has {} git repositories inside: {}. Give the agent their history read-only (git log, diff, branches)?{limit} {GIT_WARNING}",
        path.display(),
        found.len(),
        names.join(", ")
    )
}

fn pick(path: &Path, choose: &Choose) -> Result<Option<Vec<String>>> {
    let Some(selection) = choose(path)? else {
        return Ok(None);
    };
    ensure_room(path, selection.uploaded)?;
    Ok(Some(selection.skip))
}

pub fn ensure_room(path: &Path, size: u64) -> Result<()> {
    ssh::install_host_alias(&Config::load()?)?;
    let free: u64 = remote::output(FREE_SPACE, &[])?
        .trim()
        .parse()
        .context("could not read the free space on the VPS")?;
    let needed = size * 2 + RESERVE;
    ensure!(
        free >= needed,
        "{} needs about {} on the VPS (the files, their first snapshot and a reserve), only {} is free. Upload less, free space with `sai rm <project> --purge` or use a bigger VPS",
        path.display(),
        project::size::human(needed),
        project::size::human(free)
    );
    Ok(())
}

struct Prepared {
    vps: String,
    now: String,
    existed: bool,
    agent: String,
}

impl Prepared {
    fn parse(text: &str) -> Self {
        let mut prepared = Prepared {
            vps: String::new(),
            now: String::new(),
            existed: false,
            agent: String::new(),
        };
        for line in text.lines() {
            match line.split_once('\t') {
                Some(("vps", id)) => prepared.vps = id.trim().to_owned(),
                Some(("now", stamp)) => prepared.now = stamp.trim().to_owned(),
                Some(("agent", agent)) => prepared.agent = agent.trim().to_owned(),
                _ if line.trim() == "existed" => prepared.existed = true,
                _ => {}
            }
        }
        prepared
    }
}

fn forget_other_vps(registry: &mut Registry, name: &str, prepared: &Prepared) -> Result<()> {
    ensure!(!prepared.vps.is_empty(), "the VPS did not report its id");
    let project = registry.get(name)?.clone();
    if project.vps.as_deref() == Some(prepared.vps.as_str()) {
        return Ok(());
    }
    let live = project.live_sessions()?;
    let moved = match &project.vps {
        Some(_) => true,
        None => !prepared.existed && !live.is_empty(),
    };
    if moved {
        output::warning(&format!(
            "{name} was on another VPS. sai drops its old sync and uploads the folders here from your computer, local files are not touched"
        ));
        for id in &live {
            mutagen::drop_if_present(id)?;
        }
    }
    registry.set_vps(name, &prepared.vps)?;
    registry.save()
}

fn choose_agent(name: &str, requested: Option<Agent>, bound: Option<Agent>) -> Result<Agent> {
    match (requested, bound) {
        (Some(requested), Some(bound)) if requested != bound => {
            bail!("{name} is bound to {bound}, switch it with `sai change {requested} {name}`")
        }
        (requested, bound) => Ok(requested.or(bound).unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests {
    use super::choose_agent;
    use crate::agent::Agent;

    #[test]
    fn reads_what_the_vps_reports() {
        let prepared =
            super::Prepared::parse("vps\tab12\nnow\t20261117T090000Z\nexisted\nagent\tclaude\n");
        assert_eq!(prepared.vps, "ab12");
        assert_eq!(prepared.now, "20261117T090000Z");
        assert!(prepared.existed);
        assert_eq!(prepared.agent, "claude");
        let fresh = super::Prepared::parse("vps\tcd34\nagent\t\n");
        assert!(!fresh.existed);
        assert!(fresh.agent.is_empty());
    }

    #[test]
    fn uses_the_default_for_new_projects() {
        assert_eq!(choose_agent("app", None, None).unwrap(), Agent::Claude);
        assert_eq!(
            choose_agent("app", Some(Agent::Codex), None).unwrap(),
            Agent::Codex
        );
    }

    #[test]
    fn keeps_the_bound_agent() {
        assert_eq!(
            choose_agent("app", None, Some(Agent::Codex)).unwrap(),
            Agent::Codex
        );
        assert_eq!(
            choose_agent("app", Some(Agent::Codex), Some(Agent::Codex)).unwrap(),
            Agent::Codex
        );
    }

    #[test]
    fn refuses_a_second_agent() {
        let error = choose_agent("app", Some(Agent::Codex), Some(Agent::Claude)).unwrap_err();
        assert!(error.to_string().contains("sai change codex app"));
    }
}
