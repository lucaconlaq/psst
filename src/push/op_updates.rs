use std::collections::{HashMap, HashSet};

use anyhow::{Result, ensure};
use zeroize::Zeroizing;

use crate::config::{Config, FileLock, MappingKind};
use crate::process::TransferState;
use crate::providers::op::push as op;

use super::{LatestConfig, Outcome, Plan, Protected, path_evidence};

struct PreparedOp {
    target: op::Target,
    payload: Zeroizing<Vec<u8>>,
    participating: Vec<usize>,
    _lock: FileLock,
}

impl Plan {
    pub(super) fn op_groups(&self, by_item: bool) -> Vec<Vec<usize>> {
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for (index, file) in self.files.iter().enumerate() {
            let Some(target) = &file.op_target else {
                continue;
            };
            if !matches!(file.outcome, Outcome::Pending) {
                continue;
            }
            if let Some(group) = groups.iter_mut().find(|group| {
                let other = self.files[group[0]].op_target.as_ref().unwrap();
                if by_item {
                    target.same_item(other)
                } else {
                    target == other
                }
            }) {
                group.push(index);
            } else {
                groups.push(vec![index]);
            }
        }
        groups
    }

    pub(super) async fn plan_op(&mut self, config: &Config, oversized: &HashSet<String>) {
        let mut resolved: HashMap<&str, Option<op::Target>> = HashMap::new();
        for file in &mut self.files {
            let entry = &config.entries[&file.name];
            if entry.source != "op"
                || (!matches!(file.outcome, Outcome::Pending) && !oversized.contains(&file.name))
            {
                continue;
            }
            let target = if let Some(target) = resolved.get(entry.value.as_str()) {
                target.clone()
            } else {
                let target = op::Target::resolve(&entry.value, config.dir()).await.ok();
                resolved.insert(entry.value.as_str(), target.clone());
                target
            };
            if target.is_none() {
                file.input = None;
                file.outcome = Outcome::Failed("cannot resolve existing eligible op target");
            }
            file.op_target = target;
        }
        let blocked: Vec<_> = self
            .files
            .iter()
            .filter(|file| oversized.contains(&file.name))
            .filter_map(|file| file.op_target.clone())
            .collect();
        for file in &mut self.files {
            if file
                .op_target
                .as_ref()
                .is_some_and(|target| blocked.contains(target))
            {
                file.input = None;
                file.outcome = Outcome::Failed("op input exceeds 64 KiB for shared target");
            }
        }
        for group in self.op_groups(false) {
            let first = &self.files[group[0]];
            let input = &first.input.as_ref().unwrap().text;
            let outcome = if group
                .iter()
                .any(|&index| self.files[index].input.as_ref().unwrap().text != *input)
            {
                Outcome::Failed("conflicting local contents for shared op target")
            } else {
                match first
                    .op_target
                    .as_ref()
                    .unwrap()
                    .unchanged(config.dir(), input)
                    .await
                {
                    Ok(true) => Outcome::Unchanged,
                    Ok(false) => Outcome::Pending,
                    Err(_) => Outcome::Failed("cannot read existing eligible op target"),
                }
            };
            for index in group {
                self.files[index].outcome = outcome;
                if !matches!(outcome, Outcome::Pending) {
                    self.files[index].input = None;
                }
            }
        }
    }

    pub(super) async fn persist_op(&mut self, config: &Config) {
        for group in self.op_groups(true) {
            let result = self.prepare_op(config, &group).await;
            match result {
                Ok(Some(PreparedOp {
                    target,
                    payload,
                    participating,
                    _lock,
                })) => {
                    // State lives in Plan rather than the future: Drop can report
                    // uncertainty when cancellation interrupts an edit await.
                    self.op_state = TransferState::NotStarted;
                    for &index in &participating {
                        self.files[index].outcome = Outcome::RemotePending;
                    }
                    let result = target.edit(config.dir(), payload, &mut self.op_state).await;
                    let outcome = match result {
                        Ok(()) => Outcome::Saved,
                        Err(_) if self.op_state == TransferState::NotStarted => {
                            Outcome::Failed("remote edit failed before start")
                        }
                        Err(_) => Outcome::Failed(
                            "remote edit failed or interrupted; outcome uncertain; verify provider",
                        ),
                    };
                    for index in participating {
                        self.files[index].outcome = outcome;
                        self.files[index].input = None;
                    }
                }
                Ok(None) => (),
                Err(_) => {
                    for &index in &group {
                        if matches!(self.files[index].outcome, Outcome::Pending) {
                            self.files[index].outcome = Outcome::Failed(
                                "op item no longer eligible or preparation failed; no edit started",
                            );
                            self.files[index].input = None;
                        }
                    }
                }
            }
            tokio::task::yield_now().await;
        }
    }

    async fn prepare_op(&mut self, config: &Config, group: &[usize]) -> Result<Option<PreparedOp>> {
        loop {
            path_evidence(&config.path)?;
            let (latest, _lock) = config.reload_locked()?;
            let latest = LatestConfig(latest);
            let source_path = path_evidence(&config.path)?;
            let protected = Protected::load(&latest.0)?;
            // Revalidate mappings by resolved identity, never reference spelling.
            // Fail a whole shared target if any represented input changed.
            for fields in self.op_groups(false) {
                if !group.contains(&fields[0]) {
                    continue;
                }
                let mut valid = true;
                for &index in &fields {
                    let file = &self.files[index];
                    let mapping =
                        latest.0.entries.get(&file.name).filter(|entry| {
                            entry.kind == MappingKind::File && entry.source == "op"
                        });
                    valid &= if let Some(mapping) = mapping {
                        file.input
                            .as_ref()
                            .unwrap()
                            .check(&file.path, &protected)
                            .is_ok()
                            && op::Target::resolve(&mapping.value, latest.0.dir())
                                .await
                                .ok()
                                .as_ref()
                                == file.op_target.as_ref()
                    } else {
                        false
                    };
                }
                if !valid {
                    for index in fields {
                        self.files[index].outcome = Outcome::Failed(
                            "op mapping, eligibility, or local input changed; no edit started",
                        );
                        self.files[index].input = None;
                    }
                }
            }
            let participating: Vec<_> = group
                .iter()
                .copied()
                .filter(|&index| matches!(self.files[index].outcome, Outcome::Pending))
                .collect();
            let Some(&first) = participating.first() else {
                return Ok(None);
            };
            let target = self.files[first].op_target.as_ref().unwrap().clone();
            // Read the whole item again after resolution. Latest values are approved
            // overwrites, not stale-value conflicts. No conditional-write guarantee.
            let mut item = target.load(latest.0.dir()).await?;
            for &index in &participating {
                let file = &self.files[index];
                item.set(
                    file.op_target.as_ref().unwrap(),
                    &file.input.as_ref().unwrap().text,
                )?;
            }
            let payload = item.payload()?;
            // Provider work and serialization may take time; reread inputs and config
            // at the final local boundary, with no subsequent asynchronous reads.
            ensure!(
                source_path == path_evidence(&config.path)?,
                "Configuration path changed"
            );
            let current = LatestConfig(Config::load(config.path.clone())?);
            let protected = Protected::load(&current.0)?;
            let mut invalid = Vec::new();
            for &index in &participating {
                let file = &self.files[index];
                let before = latest.0.entries.get(&file.name).unwrap();
                let valid = current.0.entries.get(&file.name).is_some_and(|entry| {
                    entry.kind == MappingKind::File
                        && entry.source == "op"
                        && entry.value == before.value
                }) && file
                    .input
                    .as_ref()
                    .unwrap()
                    .check(&file.path, &protected)
                    .is_ok();
                if !valid {
                    invalid.push(file.op_target.clone());
                }
            }
            if !invalid.is_empty() {
                for &index in &participating {
                    if invalid.contains(&self.files[index].op_target) {
                        self.files[index].outcome =
                            Outcome::Failed("op mapping or local input changed; no edit started");
                        self.files[index].input = None;
                    }
                }
                // Rebuild without failed fields so independent valid targets proceed.
                continue;
            }
            return Ok(Some(PreparedOp {
                target,
                payload,
                participating,
                _lock,
            }));
        }
    }
}
