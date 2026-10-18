//! Exact-text push planning never uses pull's plaintext disk staging.
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, Metadata},
    io::{IsTerminal, Read, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use zeroize::{Zeroize, Zeroizing};

use crate::config::{Config, MappingKind, PersistenceError, WriteBoundary};
use crate::process::TransferState;
use crate::providers::local_storage::{Agent, PublicKey, PushTarget, VALUE_LIMIT};
use crate::providers::op::push as op;

mod op_updates;
#[cfg(test)]
mod tests;

const INPUT_LIMIT: usize = 4_194_304;
const BATCH_LIMIT: usize = 67_108_864;

#[derive(Clone, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
    mode: u32,
    uid: u32,
    gid: u32,
}

impl Identity {
    fn of(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            uid: metadata.uid(),
            gid: metadata.gid(),
        }
    }
}

#[derive(PartialEq, Eq)]
struct Evidence {
    identity: Identity,
    len: u64,
    links: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl Evidence {
    fn of(metadata: &Metadata) -> Self {
        Self {
            identity: Identity::of(metadata),
            len: metadata.len(),
            links: metadata.nlink(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        }
    }
}

struct Input {
    text: Zeroizing<Vec<u8>>,
    ancestors: Vec<Identity>,
    evidence: Evidence,
}

impl Input {
    fn read(path: &Path, protected: &Protected) -> Result<Self> {
        protected.check(path)?;
        // Open each original component relative to its pinned predecessor.
        // In particular, symlinks cannot disappear through a later `..`.
        let mut directory = File::open("/")?;
        let mut ancestors = vec![Identity::of(&directory.metadata()?)];
        let components: Vec<_> = path.components().collect();
        ensure!(
            path.is_absolute() && components.len() > 1,
            "Invalid input path"
        );
        for component in &components[1..components.len() - 1] {
            directory = open_at(&directory, component.as_os_str(), true)?;
            ancestors.push(Identity::of(&directory.metadata()?));
        }
        let mut file = open_at(&directory, components.last().unwrap().as_os_str(), false)?;
        let metadata = file.metadata()?;
        ensure!(metadata.is_file(), "Input is not a regular file");
        ensure!(metadata.len() <= INPUT_LIMIT as u64, "Input exceeds 4 MiB");
        protected.check_identity(&metadata)?;
        let evidence = Evidence::of(&metadata);
        // Fixed allocation avoids reallocating and freeing unwiped plaintext.
        let capacity = metadata.len() as usize + 1;
        let mut text = Zeroizing::new(Vec::with_capacity(capacity));
        let mut buffer = Zeroizing::new([0u8; 16384]);
        while text.len() < capacity {
            let remaining = (capacity - text.len()).min(buffer.len());
            let count = file.read(&mut buffer[..remaining])?;
            if count == 0 {
                break;
            }
            text.extend_from_slice(&buffer[..count]);
        }
        ensure!(
            text.len() == metadata.len() as usize,
            "Input changed size while reading"
        );
        ensure!(!text.contains(&0), "Input contains NUL");
        ensure!(std::str::from_utf8(&text).is_ok(), "Input is not UTF-8");
        ensure!(
            evidence == Evidence::of(&file.metadata()?),
            "Input changed while reading"
        );
        // Verify the pathname still reaches the same pinned descriptors.
        let current = path_evidence(path)?;
        ensure!(
            current == (ancestors.clone(), evidence.identity_key()),
            "Input path changed while reading"
        );
        Ok(Self {
            text,
            ancestors,
            evidence,
        })
    }

    fn check(&self, path: &Path, protected: &Protected) -> Result<()> {
        let now = Self::read(path, protected)?;
        ensure!(
            self.ancestors == now.ancestors
                && self.evidence == now.evidence
                && self.text == now.text,
            "Input or ancestor changed since planning"
        );
        Ok(())
    }
}

// Only inode identity is needed to compare the reopened pathname here; full
// metadata and exact bytes are compared by Input::check before persistence.
impl Evidence {
    fn identity_key(&self) -> (u64, u64) {
        (self.identity.device, self.identity.inode)
    }
}

fn open_at(parent: &File, name: &std::ffi::OsStr, directory: bool) -> Result<File> {
    use std::os::{
        fd::{AsRawFd, FromRawFd},
        unix::ffi::OsStrExt,
    };
    let name = std::ffi::CString::new(name.as_bytes())?;
    let flags = libc::O_RDONLY
        | libc::O_NOFOLLOW
        | libc::O_NONBLOCK
        | libc::O_CLOEXEC
        | if directory { libc::O_DIRECTORY } else { 0 };
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error()).context("Cannot safely open input");
    }
    // A successful openat transfers a new owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn path_evidence(path: &Path) -> Result<(Vec<Identity>, (u64, u64))> {
    let mut file = File::open("/")?;
    let mut ancestors = vec![Identity::of(&file.metadata()?)];
    let components: Vec<_> = path.components().collect();
    for component in &components[1..components.len() - 1] {
        file = open_at(&file, component.as_os_str(), true)?;
        ancestors.push(Identity::of(&file.metadata()?));
    }
    let target = open_at(&file, components.last().unwrap().as_os_str(), false)?.metadata()?;
    Ok((ancestors, (target.dev(), target.ino())))
}

// Keep the existing public Config ownership API unchanged; only push's latest
// configuration owns new local plaintext that needs this additional guard.
struct LatestConfig(Config);

impl Drop for LatestConfig {
    fn drop(&mut self) {
        self.0.clear_plaintext();
    }
}

struct Protected {
    inodes: HashSet<(u64, u64)>,
    names: Vec<PathBuf>,
}

impl Protected {
    fn load(config: &Config) -> Result<Self> {
        let mut names = vec![config.path.clone(), config.lock_path()];
        for name in [
            "psst.json",
            ".psst.json",
            "psst.json.lock",
            ".psst.json.lock",
        ] {
            names.push(config.dir().join(name));
        }
        let mut inodes = HashSet::new();
        for path in &names {
            if let Ok(metadata) = fs::metadata(path) {
                inodes.insert((metadata.dev(), metadata.ino()));
            }
        }
        let mut pending = vec![config.dir().join(".psst")];
        let mut visited = HashSet::new();
        let mut count = 0;
        while let Some(path) = pending.pop() {
            let metadata = match fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => bail!("Cannot identify protected storage"),
            };
            let id = (metadata.dev(), metadata.ino());
            inodes.insert(id);
            if metadata.is_dir() && visited.insert(id) {
                for entry in fs::read_dir(path)? {
                    count += 1;
                    ensure!(count <= 4096, "Protected storage scan limit exceeded");
                    let entry = entry?;
                    let metadata = match fs::metadata(entry.path()) {
                        Ok(metadata) => metadata,
                        // An unreachable entry has no reachable identity to
                        // protect. Literal .psst paths remain forbidden.
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(_) => bail!("Cannot identify protected storage entry"),
                    };
                    inodes.insert((metadata.dev(), metadata.ino()));
                    if entry.file_type()?.is_dir() {
                        pending.push(entry.path());
                    }
                }
            }
        }
        Ok(Self { inodes, names })
    }

    fn check_identity(&self, metadata: &Metadata) -> Result<()> {
        ensure!(
            !self.inodes.contains(&(metadata.dev(), metadata.ino())),
            "Input targets protected psst artifacts"
        );
        Ok(())
    }

    fn check(&self, path: &Path) -> Result<()> {
        for ancestor in path.ancestors() {
            ensure!(
                !ancestor.file_name().is_some_and(|name| [
                    ".psst",
                    "psst.json",
                    ".psst.json",
                    "psst.json.lock",
                    ".psst.json.lock"
                ]
                .iter()
                .any(|protected| name == *protected)),
                "Input targets protected psst artifacts"
            );
            if let Ok(metadata) = fs::metadata(ancestor) {
                self.check_identity(&metadata)?;
            }
        }
        let canonical = fs::canonicalize(path).context("Cannot resolve input")?;
        ensure!(
            !self
                .names
                .iter()
                .any(|name| fs::canonicalize(name).is_ok_and(|name| name == canonical)),
            "Input targets protected psst artifacts"
        );
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Outcome {
    Pending,
    RemotePending,
    Saved,
    Unchanged,
    Failed(&'static str),
}

struct PlannedFile {
    name: String,
    path: PathBuf,
    input: Option<Input>,
    target: Option<PushTarget>,
    op_target: Option<op::Target>,
    outcome: Outcome,
}

struct Plan {
    files: Vec<PlannedFile>,
    agent: Option<Agent>,
    op_state: TransferState,
    report_on_drop: bool,
}

impl Drop for Plan {
    fn drop(&mut self) {
        if self.report_on_drop {
            self.report();
        }
    }
}

impl Plan {
    async fn build(config: &Config) -> Self {
        Self::build_with_agent(config, Agent::from_env().ok()).await
    }

    async fn build_with_agent(config: &Config, agent: Option<Agent>) -> Self {
        let protected = Protected::load(config);
        let files = config
            .entries
            .iter()
            .filter(|(_, entry)| entry.kind == MappingKind::File)
            .map(|(name, _)| PlannedFile {
                name: name.clone(),
                path: config.dir().join(name),
                input: None,
                target: None,
                op_target: None,
                outcome: Outcome::Pending,
            })
            .collect();
        // A dropped planning future also reports all unfinished mappings.
        let mut plan = Self {
            files,
            agent,
            op_state: TransferState::NotStarted,
            report_on_drop: true,
        };
        let mut aggregate = 0usize;
        let mut over_cap = false;
        let mut oversized = HashSet::new();
        for file in &mut plan.files {
            tokio::task::yield_now().await;
            let entry = &config.entries[&file.name];
            let input = protected
                .as_ref()
                .map_err(|_| ())
                .and_then(|protected| Input::read(&file.path, protected).map_err(|_| ()));
            let (input, outcome) = match input {
                Err(_) => (
                    None,
                    Outcome::Failed("unsafe, missing, unreadable, or invalid input"),
                ),
                Ok(input) => {
                    aggregate = aggregate.saturating_add(input.text.len());
                    over_cap |= aggregate > BATCH_LIMIT;
                    if entry.source == "local-storage" {
                        if input.text.len() > VALUE_LIMIT {
                            oversized.insert(file.name.clone());
                            (None, Outcome::Failed("local-storage input exceeds 1 MiB"))
                        } else if over_cap {
                            (
                                None,
                                Outcome::Failed("batch exceeds 64 MiB; no source writes"),
                            )
                        } else {
                            (Some(input), Outcome::Pending)
                        }
                    } else if entry.source == "op" {
                        if input.text.len() > op::VALUE_LIMIT {
                            oversized.insert(file.name.clone());
                            (None, Outcome::Failed("op input exceeds 64 KiB"))
                        } else if over_cap {
                            (
                                None,
                                Outcome::Failed("batch exceeds 64 MiB; no source writes"),
                            )
                        } else {
                            (Some(input), Outcome::Pending)
                        }
                    } else if entry.source != "manual" {
                        (None, Outcome::Failed("provider does not support push"))
                    } else if input.text.as_slice() == entry.value.as_bytes() {
                        (None, Outcome::Unchanged)
                    } else if over_cap {
                        (
                            None,
                            Outcome::Failed("batch exceeds 64 MiB; no source writes"),
                        )
                    } else {
                        (Some(input), Outcome::Pending)
                    }
                }
            };
            file.input = input;
            file.outcome = outcome;
        }
        if over_cap {
            for file in &mut plan.files {
                file.input = None;
                file.outcome = Outcome::Failed("batch exceeds 64 MiB; no source writes");
            }
        }
        if !over_cap {
            // Finish the pre-deduplication resource check before agent work.
            // Cache only public resolution metadata, never provider plaintext.
            let mut resolved: HashMap<&str, Option<PushTarget>> = HashMap::new();
            for file in &mut plan.files {
                let entry = &config.entries[&file.name];
                if entry.source != "local-storage"
                    || (!matches!(file.outcome, Outcome::Pending)
                        && !oversized.contains(&file.name))
                {
                    continue;
                }
                let target = if let Some(target) = resolved.get(entry.value.as_str()) {
                    target.clone()
                } else {
                    let target = if let Some(agent) = &plan.agent {
                        PushTarget::resolve(agent, config.dir(), &entry.value)
                            .await
                            .ok()
                    } else {
                        None
                    };
                    resolved.insert(&entry.value, target.clone());
                    target
                };
                if target.is_none() {
                    file.input = None;
                    file.outcome = Outcome::Failed("cannot resolve existing encrypted target");
                }
                file.target = target;
            }
            // Oversized plaintext is already dropped, but its resolved target
            // must still reject aliases with otherwise valid-sized contents.
            let blocked: Vec<_> = plan
                .files
                .iter()
                .filter(|file| oversized.contains(&file.name))
                .filter_map(|file| file.target.clone())
                .collect();
            for file in &mut plan.files {
                if file
                    .target
                    .as_ref()
                    .is_some_and(|target| blocked.contains(target))
                {
                    file.input = None;
                    file.outcome =
                        Outcome::Failed("local-storage input exceeds 1 MiB for shared target");
                }
            }
            for group in plan.encrypted_groups() {
                let first = group[0];
                let input = &plan.files[first].input.as_ref().unwrap().text;
                let conflict = group
                    .iter()
                    .any(|&index| plan.files[index].input.as_ref().unwrap().text != *input);
                let outcome = if conflict {
                    Outcome::Failed("conflicting local contents for shared encrypted target")
                } else {
                    let target = plan.files[first].target.as_ref().unwrap();
                    match target.raw(plan.agent.as_ref().unwrap(), config.dir()).await {
                        Ok(raw) if raw.as_bytes() == input.as_slice() => Outcome::Unchanged,
                        Ok(_) => Outcome::Pending,
                        Err(_) => Outcome::Failed("cannot authenticate encrypted target"),
                    }
                };
                for index in group {
                    plan.files[index].outcome = outcome;
                    if !matches!(outcome, Outcome::Pending) {
                        plan.files[index].input = None;
                    }
                }
            }
        }
        if !over_cap {
            plan.plan_op(config, &oversized).await;
        }
        plan.report_on_drop = false;
        plan
    }

    fn encrypted_groups(&self) -> Vec<Vec<usize>> {
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for (index, file) in self.files.iter().enumerate() {
            if file.target.is_none() || !matches!(file.outcome, Outcome::Pending) {
                continue;
            }
            if let Some(group) = groups
                .iter_mut()
                .find(|group| self.files[group[0]].target == file.target)
            {
                group.push(index);
            } else {
                groups.push(vec![index]);
            }
        }
        groups
    }

    async fn persist_encrypted(
        &mut self,
        config: &Config,
        mut boundary: impl FnMut(WriteBoundary) -> Result<()>,
    ) {
        for group in self.encrypted_groups() {
            let result = self
                .persist_encrypted_group(config, &group, &mut boundary)
                .await;
            let outcome = match result {
                Ok(()) => Outcome::Saved,
                Err(error)
                    if error
                        .downcast_ref::<PersistenceError>()
                        .is_some_and(|error| error.changed) =>
                {
                    Outcome::Failed("ciphertext replacement completed; durability uncertain")
                }
                Err(_) => Outcome::Failed("encrypted save failed before replacement"),
            };
            for index in group {
                self.files[index].outcome = outcome;
                self.files[index].input = None;
            }
            // Cancellation between independent writes retains completed results.
            tokio::task::yield_now().await;
        }
    }

    async fn persist_encrypted_group(
        &self,
        config: &Config,
        group: &[usize],
        boundary: impl FnMut(WriteBoundary) -> Result<()>,
    ) -> Result<()> {
        path_evidence(&config.path)?;
        let (latest, _lock) = config.reload_locked()?;
        let latest = LatestConfig(latest);
        let source_path = path_evidence(&config.path)?;
        let target = self.files[group[0]].target.as_ref().unwrap();
        let validate = |keys: &[PublicKey]| -> Result<()> {
            ensure!(
                source_path == path_evidence(&config.path)?,
                "Configuration path changed"
            );
            let current = LatestConfig(Config::load(config.path.clone())?);
            let protected = Protected::load(&current.0)?;
            let mut references = HashSet::new();
            for &index in group {
                let file = &self.files[index];
                file.input.as_ref().unwrap().check(&file.path, &protected)?;
                let mapping = current
                    .0
                    .entries
                    .get(&file.name)
                    .filter(|entry| {
                        entry.kind == MappingKind::File && entry.source == "local-storage"
                    })
                    .context("Mapping removed or retargeted")?;
                if references.insert(&mapping.value) {
                    ensure!(
                        PushTarget::resolve_with_keys(current.0.dir(), &mapping.value, keys)?
                            == *target,
                        "Mapping no longer resolves to approved target"
                    );
                }
            }
            Ok(())
        };
        let agent = self.agent.as_ref().context("SSH agent unavailable")?;
        // Include queued identity work in a bounded phase before signing.
        let keys = crate::providers::local_storage::phase(agent.identities()).await?;
        validate(&keys)?;
        let value = std::str::from_utf8(&self.files[group[0]].input.as_ref().unwrap().text)?;
        target
            .save(agent, latest.0.dir(), value, validate, boundary)
            .await
    }

    fn report(&self) -> bool {
        let mut success = true;
        let mut saved = false;
        for file in &self.files {
            let message = match file.outcome {
                Outcome::Saved => {
                    saved = true;
                    "saved"
                }
                Outcome::Unchanged => "unchanged",
                Outcome::Failed(reason) => {
                    success = false;
                    reason
                }
                Outcome::RemotePending if self.op_state != TransferState::NotStarted => {
                    success = false;
                    "failed or interrupted remote edit; outcome uncertain; verify provider"
                }
                Outcome::Pending | Outcome::RemotePending => {
                    success = false;
                    "cancelled; no source write for this file"
                }
            };
            let failed = matches!(
                file.outcome,
                Outcome::Failed(_) | Outcome::Pending | Outcome::RemotePending
            );
            if failed {
                crate::console::error(format_args!("❌ {}: failed: {message}", file.name));
            } else if matches!(file.outcome, Outcome::Saved) {
                crate::console::info(format_args!("✅ {}: {message}", file.name));
            } else {
                eprintln!("⏭️  {}: {message}", file.name);
            }
        }
        if !success && saved {
            crate::console::warn(format_args!(
                "Push partially completed; completed saves kept."
            ));
        }
        success
    }

    fn persist(&mut self, config: &Config, boundary: impl FnMut(WriteBoundary) -> Result<()>) {
        let result = self.persist_inner(config, boundary);
        if let Err(error) = result {
            let reason = if error
                .downcast_ref::<PersistenceError>()
                .is_some_and(|error| error.changed)
            {
                "configuration replacement completed; durability uncertain"
            } else {
                "configuration save failed before replacement"
            };
            for file in &mut self.files {
                if matches!(file.outcome, Outcome::Pending) {
                    file.outcome = Outcome::Failed(reason);
                }
            }
        }
        // Clear owned snapshots on success and failure, before returning.
        for file in &mut self.files {
            file.input = None;
        }
    }

    fn persist_inner(
        &mut self,
        config: &Config,
        mut boundary: impl FnMut(WriteBoundary) -> Result<()>,
    ) -> Result<()> {
        path_evidence(&config.path)?;
        let (latest, _lock) = config.reload_locked()?;
        let source_path = path_evidence(&config.path)?;
        let mut latest = LatestConfig(latest);
        loop {
            let protected = Protected::load(&latest.0)?;
            let mut originals = Vec::new();
            for file in &mut self.files {
                if !matches!(file.outcome, Outcome::Pending) {
                    continue;
                }
                let Some(input) = &file.input else {
                    continue;
                };
                let target =
                    latest.0.entries.get_mut(&file.name).filter(|entry| {
                        entry.kind == MappingKind::File && entry.source == "manual"
                    });
                let Some(target) = target else {
                    file.outcome = Outcome::Failed("mapping removed or retargeted");
                    continue;
                };
                if input.check(&file.path, &protected).is_err() {
                    file.outcome = Outcome::Failed("input or path safety changed since planning");
                    continue;
                }
                // Keep latest literals in zeroizing owners until this attempt
                // finishes, so a rejected group can be rebuilt without them.
                let value = std::str::from_utf8(&input.text)?.to_owned();
                originals.push((
                    file.name.clone(),
                    Zeroizing::new(std::mem::replace(&mut target.value, value)),
                ));
            }
            if originals.is_empty() {
                break;
            }
            let mut rebuild = false;
            let result = latest.0.save_current_locked(|stage| {
                boundary(stage)?;
                if stage == WriteBoundary::Replace {
                    ensure!(
                        source_path == path_evidence(&config.path)?,
                        "Configuration path changed"
                    );
                    // Serialization, temporary writing and syncing can take
                    // time. Check again at the actual replacement boundary.
                    let protected = Protected::load(&latest.0)?;
                    for file in &mut self.files {
                        if matches!(file.outcome, Outcome::Pending)
                            && let Some(input) = &file.input
                            && input.check(&file.path, &protected).is_err()
                        {
                            file.outcome =
                                Outcome::Failed("input or path safety changed since planning");
                            rebuild = true;
                        }
                    }
                    ensure!(!rebuild, "Pending input group changed before replacement");
                }
                Ok(())
            });
            for (name, mut original) in originals {
                let value = &mut latest.0.entries[&name].value;
                value.zeroize();
                *value = std::mem::take(&mut *original);
            }
            if !rebuild {
                result?;
                break;
            }
            // The rejected temporary file is gone. Each retry removes at
            // least one failed target and retains the same lock and baseline.
        }
        for file in &mut self.files {
            if matches!(file.outcome, Outcome::Pending) {
                file.outcome = Outcome::Saved;
            }
        }
        Ok(())
    }
}

/// Snapshot all managed files and obtain one approval. Save existing encrypted
/// and eligible op targets independently, then manual literals in one group.
pub async fn run(config: &Config, yes: bool) -> Result<bool> {
    // Config::discover supplies absolute paths at the CLI boundary. Support
    // library callers with relative configuration paths as well.
    let absolute = std::env::current_dir()?.join(&config.path);
    let mut selected = Config::load(absolute)?;
    selected.entries = config.entries.clone();
    let mut plan = Plan::build(&selected).await;
    plan.report_on_drop = true;
    if plan.files.is_empty() {
        eprintln!("📄 No managed files. Add mappings in the Files view.");
    }
    if plan
        .files
        .iter()
        .any(|file| matches!(file.outcome, Outcome::Pending))
    {
        eprintln!("Push local → provider:");
        for file in &plan.files {
            if matches!(file.outcome, Outcome::Pending) {
                eprintln!("  {} [{}]", file.name, selected.entries[&file.name].source);
            }
        }
        for group in plan
            .encrypted_groups()
            .into_iter()
            .chain(plan.op_groups(false))
        {
            if group.len() > 1 {
                let names = group
                    .iter()
                    .map(|&index| plan.files[index].name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                crate::console::warn(format_args!("Shared target: {names}"));
            }
        }
        if plan
            .files
            .iter()
            .any(|file| file.op_target.is_some() && matches!(file.outcome, Outcome::Pending))
        {
            crate::console::warn(format_args!("op edits may race with other writers."));
        }
        crate::console::warn(format_args!(
            "Other references may change; only this config was checked."
        ));
        let approved = if yes {
            true
        } else if std::io::stdin().is_terminal() {
            eprint!("Save all? [y/N] ");
            std::io::stderr().flush()?;
            crate::materialization::consent().await.unwrap_or(false)
        } else {
            crate::console::warn(format_args!("Push requires interactive consent or --yes."));
            false
        };
        if !approved {
            for file in &mut plan.files {
                if matches!(file.outcome, Outcome::Pending) {
                    file.outcome =
                        Outcome::Failed("approval refused or unavailable; no source writes");
                }
            }
        } else {
            // Deliver pending cancellation before the synchronous atomic group.
            tokio::task::yield_now().await;
            plan.persist_encrypted(&selected, |_| Ok(())).await;
            plan.persist_op(&selected).await;
            plan.persist(&selected, |_| Ok(()));
        }
    }
    plan.report_on_drop = false;
    Ok(plan.report())
}
