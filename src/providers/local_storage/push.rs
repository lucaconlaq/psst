//! Existing-target push, distinct from creation's stale authoring snapshot.
use std::path::Path;

use anyhow::{Context, Result, ensure};
use zeroize::Zeroizing;

use super::{Agent, PublicKey, StoredKey, phase, reference};
use crate::config::WriteBoundary;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PushTarget {
    id: String,
    key: PublicKey,
}

impl PushTarget {
    pub(crate) async fn resolve(agent: &Agent, dir: &Path, value: &str) -> Result<Self> {
        phase(async { Self::resolve_with_keys(dir, value, &agent.identities().await?) }).await
    }

    pub(crate) fn resolve_with_keys(dir: &Path, value: &str, keys: &[PublicKey]) -> Result<Self> {
        let (id, selector) = reference(value)?;
        let mut selected = None;
        for key in keys {
            if selector.is_some_and(|selector| selector != key.fingerprint()) {
                continue;
            }
            let stored = StoredKey::load(dir, key.clone())?;
            if stored.records().iter().any(|record| record.id == id) {
                ensure!(selected.is_none(), "Ambiguous local-storage push target");
                selected = Some(Self {
                    id: id.to_owned(),
                    key: key.clone(),
                });
            }
        }
        selected.context("No eligible loaded key with an existing local-storage record")
    }

    pub(crate) async fn raw(&self, agent: &Agent, dir: &Path) -> Result<Zeroizing<String>> {
        phase(async {
            let stored = StoredKey::load(dir, self.key.clone())?;
            stored
                .records()
                .iter()
                .find(|record| record.id == self.id)
                .context("Push target disappeared")?
                .decrypt(agent, &self.key, &self.id)
                .await
        })
        .await
    }

    pub(crate) async fn save(
        &self,
        agent: &Agent,
        dir: &Path,
        value: &str,
        validate: impl FnMut(&[PublicKey]) -> Result<()>,
        boundary: impl FnMut(WriteBoundary) -> Result<()>,
    ) -> Result<()> {
        phase(async {
            // Fresh baseline: approval authorizes overwriting the latest value.
            let mut stored = StoredKey::load(dir, self.key.clone())?;
            stored
                .replace_checked(agent, &self.id, value, validate, boundary)
                .await
        })
        .await
    }
}
