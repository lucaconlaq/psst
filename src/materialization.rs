//! Consent-gated managed-file planning and independent atomic persistence.
//!
//! A plan owns private plaintext staging until dropped. Normal errors and
//! cancellation remove staging; process crashes can leave plaintext behind.
use std::{
    collections::{HashMap, HashSet, VecDeque},
    ffi::CString,
    fs::{self, File, Metadata, OpenOptions},
    io::{IsTerminal, Read, Write},
    os::fd::{AsRawFd, FromRawFd},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use tempfile::{NamedTempFile, TempDir};
use zeroize::Zeroizing;

use crate::config::{Config, MappingKind, valid_destination};
use crate::providers;

#[cfg(test)]
mod adversarial;
mod names;
#[cfg(test)]
mod tests;

const RESULT_LIMIT: usize = 4 * 1024 * 1024;
const FILE_LIMIT: usize = 4096;
const PATH_LIMIT: usize = 4 * 1024 * 1024;

/// A bounded set of individual outcomes, owning all staged plaintext.
/// There is deliberately no command-line dry-run interface.
pub struct Plan {
    /// Configuration-order outcomes, including isolated failures.
    pub files: Vec<FilePlan>,
    staging: TempDir,
}

impl Plan {
    /// Differing regular destinations requiring one later batch consent.
    pub fn replacements(&self) -> impl Iterator<Item = &Path> {
        self.files.iter().filter_map(|file| match &file.result {
            Ok(prepared) if prepared.change == Change::Replace => Some(file.destination.as_path()),
            _ => None,
        })
    }

    /// Private system-temp directory; its lifetime is the plan's lifetime.
    pub fn staging_dir(&self) -> &Path {
        self.staging.path()
    }
}

/// One destination and either its preparation or a content-free failure.
pub struct FilePlan {
    /// Absolute literal destination, interpreted beside the configuration.
    pub destination: PathBuf,
    /// Per-file failures never discard independent successes.
    pub result: Result<PreparedFile>,
}

/// Classification before any destination writes or replacement consent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// No destination exists.
    New,
    /// Identical contents in an already private regular file.
    Unchanged,
    /// A regular file differs, and needs replacement consent.
    Replace,
}

/// On-disk contents and conflict evidence for a subsequent persistence phase.
pub struct PreparedFile {
    /// Classification of the destination at planning time.
    pub change: Change,
    /// Original destination and ancestor evidence, without retained plaintext.
    pub snapshot: Snapshot,
    staged: NamedTempFile,
    ancestor_keys: Vec<NamespaceKey>,
}

// A planning-time directory namespace and filesystem-equivalent suffix remain
// stable when missing parents are created or reached through bind aliases.
type NamespaceKey = ((u64, u64), PathBuf);

impl PreparedFile {
    /// Private staged result, valid only while this preparation is owned.
    pub fn staged_path(&self) -> &Path {
        self.staged.path()
    }
}

#[derive(Debug, PartialEq, Eq)]
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

#[derive(Debug, PartialEq, Eq)]
struct FileSnapshot {
    identity: Identity,
    size: u64,
    links: u64,
    modified: (i64, i64),
    changed: (i64, i64),
    digest: [u8; 32],
}

/// Bounded ancestor identities and target metadata/hash for conflict rechecks.
/// Directory timestamps are excluded: independent sibling writes are allowed.
#[derive(Debug, PartialEq, Eq)]
pub struct Snapshot {
    ancestors: Vec<(PathBuf, Option<Identity>)>,
    target: Option<FileSnapshot>,
}

impl Snapshot {
    /// Recheck both safety and contents, including for unchanged destinations.
    /// Persistence must call this immediately before its own atomic write.
    pub fn check(&self, destination: &Path) -> Result<()> {
        if *self != snapshot(destination)? {
            bail!("Destination or ancestor changed since planning");
        }
        Ok(())
    }
}

/// Preflight every destination before any fetch, then resolve sequentially.
/// Staging and snapshots never create or repair destination paths or modes.
pub async fn plan(config: &Config) -> Result<Plan> {
    plan_with_names(config, names::key).await
}

async fn plan_with_names(
    config: &Config,
    name_key: impl Fn(&Path) -> Result<PathBuf>,
) -> Result<Plan> {
    plan_with_identity(config, name_key, inode).await
}

async fn plan_with_identity(
    config: &Config,
    name_key: impl Fn(&Path) -> Result<PathBuf>,
    inode: impl Fn(&Path) -> Option<(u64, u64)>,
) -> Result<Plan> {
    plan_with_staging(config, name_key, inode, |_| Ok(())).await
}

// Internal fault seam; never selected by CLI flags or environment variables.
async fn plan_with_staging(
    config: &Config,
    name_key: impl Fn(&Path) -> Result<PathBuf>,
    inode: impl Fn(&Path) -> Option<(u64, u64)>,
    mut boundary: impl FnMut(crate::config::WriteBoundary) -> Result<()>,
) -> Result<Plan> {
    let cwd = std::env::current_dir()?;
    let dir = cwd.join(config.dir());
    let config_path = cwd.join(&config.path);
    let mut paths = Vec::new();
    let mut aliases = HashSet::new();
    let mut inodes = HashSet::new();
    let mut path_bytes = 0usize;
    let protected = [
        config_path.clone(),
        append_lock(&config_path),
        dir.join("psst.json"),
        dir.join("psst.json.lock"),
        dir.join(".psst.json"),
        dir.join(".psst.json.lock"),
    ];
    // Broken/inaccessible protected artifacts have no reachable alias to
    // resolve. Their literal names remain protected; do not turn an unrelated
    // local-storage failure into a configuration-wide provider failure.
    let protected_paths: Vec<_> = protected.iter().map(|p| resolved(p)).collect();
    let storage = resolved(&dir.join(".psst"));
    let protected_keys: Vec<_> = protected_paths
        .iter()
        .map(|path| name_key(path))
        .collect::<Result<_>>()?;
    let storage_key = name_key(&storage)?;
    let protected_namespaces: HashSet<_> = protected_paths
        .iter()
        .zip(&protected_keys)
        .flat_map(|(path, key)| namespace_keys(path, key, &inode))
        .collect();
    let mut protected_inodes: HashSet<_> = protected.iter().filter_map(|p| inode(p)).collect();
    // Bind-mounted files need not have multiple links. Scan without reading
    // contents, including directory identities protecting storage aliases.
    let mut pending = vec![storage.clone()];
    let mut scanned = 0;
    let mut visited = HashSet::new();
    while let Some(directory) = pending.pop() {
        if let Some(id) = inode(&directory) {
            protected_inodes.insert(id);
            if !visited.insert(id) {
                continue;
            }
        }
        match fs::read_dir(&directory) {
            Ok(children) => {
                for child in children {
                    scanned += 1;
                    if scanned > FILE_LIMIT {
                        bail!("Protected storage scan limit exceeded");
                    }
                    let child = child?;
                    if let Some(id) = inode(&child.path()) {
                        protected_inodes.insert(id);
                    }
                    if child.file_type()?.is_dir() {
                        pending.push(child.path());
                    }
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) || error.raw_os_error() == Some(libc::ELOOP) => {}
            Err(error) => return Err(error).context("Cannot identify protected storage"),
        }
    }
    for (name, entry) in &config.entries {
        if entry.kind != MappingKind::File {
            continue;
        }
        // Ancestor spellings retained in snapshots count toward the budget.
        let path = dir.join(name);
        for ancestor in path.ancestors() {
            path_bytes = path_bytes.saturating_add(ancestor.as_os_str().len());
        }
        if paths.len() >= FILE_LIMIT || path_bytes > PATH_LIMIT {
            bail!("Managed-file planning metadata limit exceeded");
        }
        if !valid_destination(name) {
            bail!("Invalid managed-file destination");
        }
        let identity = resolved(&path);
        let key = name_key(&identity)?;
        let namespaces = namespace_keys(&identity, &key, &inode);
        if namespaces
            .iter()
            .any(|key| protected_namespaces.contains(key))
            || identity
                .ancestors()
                .any(|p| inode(p).is_some_and(|id| protected_inodes.contains(&id)))
            || protected_name(&path)
            || protected_name(&identity)
            || identity.starts_with(&storage)
            || protected_paths.contains(&identity)
            || protected_name(&key)
            || key.starts_with(&storage_key)
            || protected_keys.contains(&key)
            || inode(&identity).is_some_and(|id| protected_inodes.contains(&id))
        {
            bail!("Managed-file destination targets protected psst artifacts");
        }
        let target_inode = inode(&identity);
        let namespace_alias = namespaces.iter().any(|key| aliases.contains(key));
        aliases.extend(namespaces);
        if namespace_alias || target_inode.is_some_and(|id| !inodes.insert(id)) {
            bail!("Managed-file destinations alias one another");
        }
        let before = snapshot(&path);
        let ancestor_keys = match &before {
            Ok(snapshot) => snapshot
                .ancestors
                .iter()
                .map(|(path, _)| {
                    let path = location_key(path);
                    namespace_keys(&path, &name_key(&path)?, &inode)
                        .into_iter()
                        .next()
                        .context("Cannot identify ancestor namespace")
                })
                .collect::<Result<Vec<_>>>()?,
            Err(_) => Vec::new(),
        };
        paths.push((path, entry, before, ancestor_keys));
    }

    // Classify unsafe targets before staging eligibility: a directory mapping
    // is an individual failure, not a reason to discard independent files.
    // Canonicalize the system temp root (notably /var on macOS).
    let temp_root = fs::canonicalize(std::env::temp_dir())?;
    let staging = tempfile::Builder::new()
        .prefix("psst-files-")
        .tempdir_in(temp_root)?;
    fs::set_permissions(staging.path(), fs::Permissions::from_mode(0o700))?;
    for (path, _, before, _) in &paths {
        if before.is_ok() {
            let path = resolved(path);
            if path.starts_with(staging.path()) || staging.path().starts_with(&path) {
                bail!("Cannot stage outside managed destinations");
            }
        }
    }
    let mut files = Vec::with_capacity(paths.len());
    for (destination, entry, before, ancestor_keys) in paths {
        let result = match before {
            Ok(before) => {
                prepare(
                    entry,
                    &dir,
                    &destination,
                    &staging,
                    before,
                    ancestor_keys,
                    &mut boundary,
                )
                .await
            }
            Err(error) => Err(error),
        };
        files.push(FilePlan {
            destination,
            result,
        });
    }
    Ok(Plan { files, staging })
}

// Every existing ancestor identifies a namespace independently of its mounted
// spelling. Suffixes already carry the established filesystem name equivalence.
fn namespace_keys(
    path: &Path,
    key: &Path,
    inode: &impl Fn(&Path) -> Option<(u64, u64)>,
) -> Vec<NamespaceKey> {
    let components: Vec<_> = key.components().collect();
    path.ancestors()
        .filter_map(|ancestor| {
            let id = inode(ancestor)?;
            let suffix = components[ancestor.components().count()..].iter().collect();
            Some((id, suffix))
        })
        .collect()
}

/// Resolve files, obtain batch consent, then report independent outcomes.
/// The owned plan and temporary files clean up when this future is dropped.
pub async fn run(config: &Config, yes: bool) -> Result<bool> {
    if !config
        .entries
        .values()
        .any(|entry| entry.kind == MappingKind::File)
    {
        eprintln!("📄 No managed files. Add mappings in the Files view.");
        return Ok(true);
    }
    let planned = plan(config).await?;
    let mut success = true;
    for file in &planned.files {
        if let Err(error) = &file.result {
            eprintln!("❌ {}: failed: {error}", file.destination.display());
            success = false;
        }
    }
    if planned.replacements().next().is_some() && !yes {
        eprintln!("Existing files to replace:");
        for path in planned.replacements() {
            eprintln!("  {}", path.display());
        }
        let approved = if std::io::stdin().is_terminal() {
            eprint!("Replace all? [y/N] ");
            std::io::stderr().flush()?;
            consent().await?
        } else {
            eprintln!("Replacement requires interactive consent or --yes.");
            false
        };
        if !approved {
            for file in &planned.files {
                if file.result.is_ok() {
                    eprintln!(
                        "❌ {}: failed: batch cancelled; no writes",
                        file.destination.display()
                    );
                }
            }
            return Ok(false);
        }
    }
    Ok(persist_plan(&planned, success, |_| std::future::ready(()), |_| Ok(())).await)
}

// Async per-file boundary permits deterministic cancellation between commits.
async fn persist_plan<F: std::future::Future<Output = ()>>(
    planned: &Plan,
    mut success: bool,
    mut before_write: impl FnMut(usize) -> F,
    mut boundary: impl FnMut(crate::config::WriteBoundary) -> Result<()>,
) -> bool {
    let mut created = HashMap::new();
    for (index, file) in planned.files.iter().enumerate() {
        // Let cancellation drop staging between independent completed writes.
        tokio::task::yield_now().await;
        before_write(index).await;
        let Ok(prepared) = &file.result else { continue };
        match persist(prepared, &file.destination, &mut created, &mut boundary) {
            Ok(Change::Unchanged) => eprintln!("⏭️  {}: unchanged", file.destination.display()),
            Ok(_) => eprintln!("📄 {}: written", file.destination.display()),
            Err(error) => {
                if error.changed {
                    eprintln!(
                        "❌ {}: failed: replacement completed; durability uncertain: {}",
                        file.destination.display(),
                        error.source
                    );
                } else {
                    eprintln!(
                        "❌ {}: failed before replacement: {}",
                        file.destination.display(),
                        error.source
                    );
                }
                success = false;
            }
        }
    }
    success
}

// Nonblocking terminal input avoids a detached blocking stdin reader that
// would outlive cancellation and prevent runtime shutdown.
async fn consent() -> Result<bool> {
    let terminal = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOCTTY)
        .open("/dev/tty")?;
    let terminal = tokio::io::unix::AsyncFd::new(terminal)?;
    let mut answer = Vec::new();
    loop {
        let mut ready = terminal.readable().await?;
        let mut byte = [0];
        match ready.try_io(|fd| fd.get_ref().read(&mut byte)) {
            Err(_) => continue,
            Ok(Err(error)) => return Err(error.into()),
            Ok(Ok(0)) => return Ok(false),
            Ok(Ok(_)) if byte[0] == b'\n' => {
                return Ok(answer.eq_ignore_ascii_case(b"y") || answer.eq_ignore_ascii_case(b"yes"));
            }
            Ok(Ok(_)) => {
                if answer.len() >= 16 {
                    return Ok(false);
                }
                answer.push(byte[0]);
            }
        }
    }
}

// Normalize only tracking keys. Safety checks still inspect every component of
// the original spelling, including directories traversed before `..`.
fn location_key(path: &Path) -> PathBuf {
    let mut key = PathBuf::new();
    for component in path.components() {
        if component == std::path::Component::ParentDir {
            key.pop();
        } else {
            key.push(component);
        }
    }
    key
}

// Keep cleanup attached to the opened directory even if its pathname moves.
struct AnchoredTemporary {
    file: File,
    parent: File,
    name: CString,
}

impl AnchoredTemporary {
    fn new(parent: &Path) -> Result<Self> {
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(parent)?;
        let temporary =
            tempfile::Builder::new()
                .prefix(".psst-write-")
                .make_in(parent, |path| {
                    let name = CString::new(
                        path.file_name()
                            .ok_or_else(|| std::io::Error::other("Temporary has no name"))?
                            .as_bytes(),
                    )?;
                    // Create relative to the same descriptor used for cleanup,
                    // not a pathname that could now refer to another directory.
                    let fd = unsafe {
                        libc::openat(
                            directory.as_raw_fd(),
                            name.as_ptr(),
                            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC,
                            0o600,
                        )
                    };
                    if fd < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    // A successful openat transfers a fresh owned descriptor.
                    Ok(unsafe { File::from_raw_fd(fd) })
                })?;
        let name = CString::new(
            temporary
                .path()
                .file_name()
                .context("Temporary has no name")?
                .as_bytes(),
        )?;
        let (file, mut path) = temporary.into_parts();
        path.disable_cleanup(true);
        let anchored = Self {
            file,
            parent: directory,
            name,
        };
        if Identity::of(&anchored.parent.metadata()?) != Identity::of(&fs::metadata(parent)?) {
            bail!("Temporary parent changed");
        }
        Ok(anchored)
    }

    fn persist(&self, destination: &Path) -> Result<()> {
        let name = CString::new(
            destination
                .file_name()
                .context("Destination has no name")?
                .as_bytes(),
        )?;
        // Both names are relative to the same live directory descriptor.
        let result = unsafe {
            libc::renameat(
                self.parent.as_raw_fd(),
                self.name.as_ptr(),
                self.parent.as_raw_fd(),
                name.as_ptr(),
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
}

impl Drop for AnchoredTemporary {
    fn drop(&mut self) {
        // After replacement the temporary name is absent; never remove the
        // completed destination. The descriptor survives parent renames.
        unsafe { libc::unlinkat(self.parent.as_raw_fd(), self.name.as_ptr(), 0) };
    }
}

fn check_with_created(
    prepared: &PreparedFile,
    destination: &Path,
    created: &HashMap<NamespaceKey, Identity>,
) -> Result<()> {
    let expected = &prepared.snapshot;
    let current = snapshot(destination)?;
    if expected.target != current.target || expected.ancestors.len() != current.ancestors.len() {
        bail!("Destination or ancestor changed since planning");
    }
    for (((path, before), (now_path, now)), key) in expected
        .ancestors
        .iter()
        .zip(&current.ancestors)
        .zip(&prepared.ancestor_keys)
    {
        let allowed = before.as_ref().or_else(|| created.get(key));
        if path != now_path || allowed != now.as_ref() {
            bail!("Destination or ancestor changed since planning");
        }
    }
    Ok(())
}

fn persist(
    prepared: &PreparedFile,
    destination: &Path,
    created: &mut HashMap<NamespaceKey, Identity>,
    mut boundary: impl FnMut(crate::config::WriteBoundary) -> Result<()>,
) -> std::result::Result<Change, crate::config::PersistenceError> {
    use crate::config::WriteBoundary;
    let mut changed = false;
    let result = (|| {
        check_with_created(prepared, destination, created)?;
        for ((path, before), key) in prepared
            .snapshot
            .ancestors
            .iter()
            .zip(&prepared.ancestor_keys)
        {
            if before.is_none() && !created.contains_key(key) {
                // Do not accept AlreadyExists: only this run's own creations
                // may advance a missing ancestor snapshot for another file.
                fs::DirBuilder::new().mode(0o700).create(path)?;
                fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
                let metadata = fs::symlink_metadata(path)?;
                if !metadata.is_dir()
                    || metadata.file_type().is_symlink()
                    || metadata.mode() & 0o7777 != 0o700
                {
                    bail!("Created ancestor changed or is not private 0700");
                }
                created.insert(key.clone(), Identity::of(&metadata));
            }
            if before.is_none() {
                // Persist each new directory entry, including the first one
                // in an existing parent. Retry owned entries too: an earlier
                // file may have failed before their sync completed.
                let parent = path.parent().context("Ancestor has no parent")?;
                let directory = OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
                    .open(parent)?;
                boundary(WriteBoundary::DirectorySync)?;
                directory.sync_all()?;
            }
        }
        check_with_created(prepared, destination, created)?;
        // An identical target may still need traversal directories to make its
        // literal spelling reachable. Establish and sync them without touching
        // the target inode or contents before reporting unchanged.
        if prepared.change == Change::Unchanged {
            return Ok(Change::Unchanged);
        }
        let parent = destination.parent().context("Destination has no parent")?;
        let mut temporary = AnchoredTemporary::new(parent)?;
        temporary
            .file
            .set_permissions(fs::Permissions::from_mode(0o600))?;
        let mut staged = prepared.staged.reopen()?;
        // Zeroize the copy buffer rather than retaining aggregate plaintext.
        let mut buffer = Zeroizing::new([0u8; 16384]);
        loop {
            let count = staged.read(buffer.as_mut())?;
            if count == 0 {
                break;
            }
            boundary(WriteBoundary::Write)?;
            temporary.file.write_all(&buffer[..count])?;
        }
        boundary(WriteBoundary::FileSync)?;
        temporary.file.sync_all()?;
        boundary(WriteBoundary::Replace)?;
        check_with_created(prepared, destination, created)?;
        temporary.persist(destination)?;
        changed = true;
        boundary(WriteBoundary::DirectorySync)?;
        temporary.parent.sync_all()?;
        Ok(prepared.change)
    })();
    result.map_err(|source| crate::config::PersistenceError { changed, source })
}

async fn prepare(
    entry: &crate::config::Entry,
    dir: &Path,
    destination: &Path,
    staging: &TempDir,
    before: Snapshot,
    ancestor_keys: Vec<NamespaceKey>,
    boundary: &mut impl FnMut(crate::config::WriteBoundary) -> Result<()>,
) -> Result<PreparedFile> {
    before.check(destination)?;
    // No bulk provider operation/cache: only one resolved plaintext is alive.
    let contents = providers::fetch(entry, dir)
        .await
        .map_err(|_| anyhow::anyhow!("Provider resolution failed"))?;
    if contents.len() > RESULT_LIMIT {
        bail!("Resolved file exceeds 4 MiB");
    }
    let digest: [u8; 32] = Sha256::digest(contents.as_bytes()).into();
    let change = match &before.target {
        None => Change::New,
        Some(target) if target.size == contents.len() as u64 && target.digest == digest => {
            if target.identity.mode & 0o7777 != 0o600 {
                bail!("Identical destination does not have private 0600 permissions");
            }
            Change::Unchanged
        }
        Some(_) => Change::Replace,
    };
    let mut staged = NamedTempFile::new_in(staging.path())?;
    staged
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    // Split once so tests can fail with a populated staging temporary.
    let split = contents.len() / 2;
    boundary(crate::config::WriteBoundary::Write)?;
    staged.write_all(&contents.as_bytes()[..split])?;
    boundary(crate::config::WriteBoundary::Write)?;
    staged.write_all(&contents.as_bytes()[split..])?;
    boundary(crate::config::WriteBoundary::FileSync)?;
    staged.as_file().sync_all()?;
    drop(contents);
    before.check(destination)?;
    Ok(PreparedFile {
        change,
        snapshot: before,
        staged,
        ancestor_keys,
    })
}

fn append_lock(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".lock");
    name.into()
}

fn protected_name(path: &Path) -> bool {
    path.components().any(|part| part.as_os_str() == ".psst")
        || path.file_name().is_some_and(|name| {
            [
                "psst.json",
                ".psst.json",
                "psst.json.lock",
                ".psst.json.lock",
            ]
            .iter()
            .any(|protected| name == *protected)
        })
}

fn inode(path: &Path) -> Option<(u64, u64)> {
    fs::metadata(path).ok().map(|m| (m.dev(), m.ino()))
}

// Resolve existing prefixes before folding missing components or `..`.
// This detects reachable aliases, not path safety. Inspection failures leave
// components literal; snapshot() reports unsafe/unusable paths per file rather
// than aborting independent work during configuration-wide preflight.
fn resolved(path: &Path) -> PathBuf {
    let mut pending: VecDeque<_> = path
        .components()
        .map(|part| part.as_os_str().to_os_string())
        .collect();
    let mut result = PathBuf::new();
    let mut links = 0;
    while let Some(part) = pending.pop_front() {
        if part == ".." {
            result.pop();
            continue;
        }
        if part == "." {
            continue;
        }
        result.push(part);
        match fs::symlink_metadata(&result) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                links += 1;
                if links > 40 {
                    continue;
                }
                let Ok(target) = fs::read_link(&result) else {
                    continue;
                };
                result.pop();
                // Follow even dangling symlinks for alias/protection checks.
                for part in target.components().rev() {
                    pending.push_front(part.as_os_str().to_os_string());
                }
            }
            Ok(_) => {
                if let Ok(canonical) = fs::canonicalize(&result) {
                    result = canonical;
                }
            }
            Err(_) => {}
        }
    }
    result
}

fn optional_metadata(path: &Path) -> Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("Cannot inspect destination path"),
    }
}

fn snapshot(path: &Path) -> Result<Snapshot> {
    // Components discard these suffixes, but literal filesystem access requires
    // a directory. Never classify or replace the normalized regular-file name.
    let literal = path.as_os_str().as_bytes();
    if literal.ends_with(b"/") || literal.ends_with(b"/.") {
        bail!("Destination has a directory-only suffix");
    }
    let mut ancestors = Vec::new();
    // Inspect every original component before folding the next `..`, so a
    // symlink cannot disappear through normalization. Look up each resulting
    // location independently: missing parents will be created by this run,
    // and traversal out of them can expose existing directories and targets
    // that must be classified before consent.
    let mut prefix = PathBuf::new();
    let components: Vec<_> = path.components().collect();
    for component in &components[..components.len().saturating_sub(1)] {
        prefix.push(component);
        let metadata = optional_metadata(&location_key(&prefix))?;
        if metadata
            .as_ref()
            .is_some_and(|m| !m.is_dir() || m.file_type().is_symlink())
        {
            bail!("Destination ancestor is a symlink or not a directory");
        }
        ancestors.push((prefix.clone(), metadata.as_ref().map(Identity::of)));
    }
    let target_path = location_key(path);
    let target = match optional_metadata(&target_path)? {
        None => None,
        Some(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                bail!("Destination is a symlink or not a regular file");
            }
            let mut file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(&target_path)?;
            let opened = file.metadata()?;
            if Identity::of(&metadata) != Identity::of(&opened) || !opened.is_file() {
                bail!("Destination changed while opening");
            }
            let mut hash = Sha256::new();
            let mut buffer = Zeroizing::new([0u8; 16384]);
            loop {
                let count = file.read(buffer.as_mut())?;
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
            }
            let evidence = |m: &Metadata, digest| FileSnapshot {
                identity: Identity::of(m),
                size: m.len(),
                links: m.nlink(),
                modified: (m.mtime(), m.mtime_nsec()),
                changed: (m.ctime(), m.ctime_nsec()),
                digest,
            };
            let digest = hash.finalize().into();
            let before = evidence(&opened, digest);
            if before != evidence(&file.metadata()?, digest) {
                bail!("Destination changed while reading");
            }
            Some(before)
        }
    };
    Ok(Snapshot { ancestors, target })
}
