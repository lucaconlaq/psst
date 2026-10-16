use crate::{
    browser::Level,
    config::{Entries, Entry},
};

use anyhow::{Result, bail};
use futures::future::{BoxFuture, join_all};
use std::path::Path;
use zeroize::Zeroizing;

pub use op::totp;

/// Experimental SSH-agent encryption, bounded v1 storage, and secret
/// resolution.
pub mod local_storage;
mod manual;
mod op;
mod shell;
mod terraform;
mod vault;

/// A sendable provider fetch, using inherited authentication and the config
/// cwd.
pub type FetchFuture<'a> = BoxFuture<'a, Result<Zeroizing<String>>>;

/// Public identity metadata; the token is opaque to the UI.
#[derive(Clone)]
pub struct Identity {
    /// Displayable fingerprint, never agent comments or secret text.
    pub fingerprint: String,
    /// Provider-owned identity encoding.
    pub token: Vec<u8>,
}

/// Provider-owned encrypted authoring, separate from reference browsing.
pub trait Author: Sync {
    /// List eligible identities with a bounded agent-work deadline.
    fn identities(&self) -> BoxFuture<'_, Result<Vec<Identity>>>;
    /// Validate and load the selected record before requesting replacement consent.
    fn prepare(&self, dir: &Path, id: &str, key: &Identity) -> Result<Box<dyn PreparedSave>>;
    /// Select one loaded ciphertext snapshot without decrypting it.
    fn prepare_deletion(
        &self,
        dir: &Path,
        reference: &str,
    ) -> BoxFuture<'static, Result<Box<dyn PreparedDelete>>>;
    /// Select a qualified browser reference from storage without contacting an agent.
    /// Removal is independent of configuration and keeps existing mappings.
    fn prepare_browser_deletion(
        &self,
        dir: &Path,
        reference: &str,
    ) -> Result<Box<dyn PreparedDelete>>;
    /// Conservative shared-reference warning, limited to loaded configuration.
    fn may_share(&self, left: &str, right: &str) -> bool;
    /// Maximum exact UTF-8 value bytes accepted by this author.
    fn value_limit(&self) -> usize;
}

/// Selected ciphertext snapshot, with conflict-checked removal.
/// Mapping deletion must save configuration before removal. Independent browser
/// deletion keeps mappings, which may subsequently fail to resolve.
pub trait PreparedDelete: Send {
    /// Remove only this record, rejecting changes since selection.
    fn remove(
        &mut self,
        boundary: &mut dyn FnMut(crate::config::WriteBoundary) -> Result<()>,
    ) -> Result<()>;
}

/// A loaded record snapshot. Implementations must reject concurrent changes.
pub trait PreparedSave: Send {
    /// Whether saving would replace ciphertext, affecting all its references.
    fn replacing(&self) -> bool;
    /// Whether a loaded configuration reference may refer to this record.
    fn matches_reference(&self, reference: &str) -> bool;
    /// Persist ciphertext and return its authored reference, never plaintext.
    /// Dropping before replacement cancels work; completed writes are not undone.
    fn save(self: Box<Self>, value: Zeroizing<String>) -> BoxFuture<'static, Result<String>>;
}

/// Shared fetch, editor metadata, and optional browsing interface for a source.
/// Implementations live in individual modules and are registered in
/// `PROVIDERS`.
pub trait Provider: Sync {
    /// Case-sensitive identifier persisted in configuration files.
    fn id(&self) -> &'static str;
    /// Editor label describing the stored reference or literal value.
    fn reference_label(&self) -> &'static str;
    /// Whether stored references must be masked unless explicitly revealed.
    fn sensitive_reference(&self) -> bool {
        false
    }
    /// Starting browser location, or none for text-only references.
    fn browser(&self) -> Option<Level> {
        None
    }
    /// Optional encrypted value authoring; the UI never interprets identity tokens.
    fn author(&self) -> Option<&'static dyn Author> {
        None
    }
    /// Resolve a stored reference without modifying the process environment.
    /// Call the module's `fetch` function to also enforce shared NUL validation.
    fn fetch<'a>(&'a self, value: &'a str, dir: &'a Path) -> FetchFuture<'a>;
}

// Registry order is also the editor source-selector order.
static PROVIDERS: &[&dyn Provider] = &[
    &op::Op { otp: false },
    &op::Op { otp: true },
    &vault::Vault,
    &shell::Shell,
    &manual::Manual,
    &terraform::Terraform,
    &local_storage::LocalStorage,
];

/// Find a registered provider by its exact persisted identifier.
/// Unknown sources remain editable but cannot fetch or browse.
pub fn get(source: &str) -> Option<&'static dyn Provider> {
    PROVIDERS
        .iter()
        .copied()
        .find(|provider| provider.id() == source)
}
/// All registered provider identifiers in editor order.
pub fn sources() -> Vec<&'static str> {
    PROVIDERS.iter().map(|provider| provider.id()).collect()
}
/// Resolve a reference using the original environment and config directory.
///
/// Shell references execute trusted code. All returned values are NUL-free;
/// empty shell and JSON string values are allowed, unlike manual/op values.
pub async fn fetch(entry: &Entry, dir: &Path) -> Result<Zeroizing<String>> {
    let Some(provider) = get(&entry.source) else {
        bail!("Unknown source {:?}", entry.source);
    };
    let value = provider.fetch(&entry.value, dir).await?;
    if value.contains('\0') {
        bail!("Resolved secret contains a NUL byte");
    }
    Ok(value)
}
/// Resolve environment entries only, concurrently and in configuration order.
/// File mappings are never fetched by this environment operation.
/// Provider subprocesses cannot depend on previously resolved secrets.
pub async fn all(entries: &Entries, dir: &Path) -> Vec<(String, Result<Zeroizing<String>>)> {
    let operation = local_storage::Operation::default();
    join_all(
        entries
            .iter()
            .filter(|(_, entry)| entry.kind == crate::config::MappingKind::Env)
            .map(|(name, entry)| {
                let operation = &operation;
                async move {
                    let result = if entry.source == "local-storage" {
                        operation.fetch(&entry.value, dir).await
                    } else {
                        fetch(entry, dir).await
                    };
                    (name.clone(), result)
                }
            }),
    )
    .await
}
fn scalar(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
fn reference(value: &str) -> Result<(&str, &str)> {
    let mut pieces = value.split('#');
    match (pieces.next(), pieces.next()) {
        (Some(p), Some(k)) if !p.is_empty() && !k.is_empty() => Ok((p, k)),
        _ => bail!("Expected path#field reference"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_unique_identifiers_and_provider_owned_metadata() {
        let mut ids = std::collections::HashSet::new();
        for provider in PROVIDERS {
            assert!(ids.insert(provider.id()));
            assert!(!provider.reference_label().is_empty());
        }
        assert_eq!(
            sources(),
            [
                "op",
                "op 2FA",
                "vault",
                "shell",
                "manual",
                "terraform",
                "local-storage"
            ]
        );
        assert!(get("missing").is_none());
        assert!(get("Local-storage").is_none());
        let local = get("local-storage").unwrap();
        assert!(!local.sensitive_reference());
        assert!(local.browser().is_some());
        assert_eq!(local.reference_label(), "Local storage ID · Ctrl+B browse");
        assert!(get("manual").unwrap().sensitive_reference());
        assert!(!get("shell").unwrap().sensitive_reference());
        for source in ["op", "op 2FA", "vault"] {
            assert!(get(source).unwrap().browser().is_some());
        }
        for source in ["manual", "shell"] {
            assert!(get(source).unwrap().browser().is_none());
        }
        assert_eq!(
            get("vault").unwrap().reference_label(),
            "Vault (KV v2) · path#field · Ctrl+B browse"
        );
        let terraform = get("terraform").unwrap();
        assert_eq!(terraform.id(), "terraform");
        assert!(get("terraform").is_some());
        assert!(terraform.browser().is_none());
    }
    #[tokio::test]
    async fn shared_dispatch_rejects_nul_and_unknown_sources() {
        let dir = tempfile::tempdir().unwrap();
        for (source, value, expected) in [
            (
                "manual",
                "bad\0value",
                "Resolved secret contains a NUL byte",
            ),
            ("missing", "value", "Unknown source"),
        ] {
            let error = fetch(
                &Entry {
                    kind: crate::config::MappingKind::Env,
                    source: source.into(),
                    value: value.into(),
                },
                dir.path(),
            )
            .await
            .unwrap_err();
            assert!(error.to_string().contains(expected));
        }
    }
    #[tokio::test]
    async fn parallel_fetches_start_together() {
        let t = tempfile::tempdir().unwrap();
        let entries: Entries = (0..4).map(|n| (format!("S{n}"), Entry { kind: crate::config::MappingKind::Env, source: "shell".into(), value: format!("touch ready{n}; while [ $(find . -name 'ready*' | wc -l) -lt 4 ]; do sleep 0.01; done; printf done") })).collect();
        let results =
            tokio::time::timeout(std::time::Duration::from_secs(3), all(&entries, t.path()))
                .await
                .expect("fetches were not concurrent");
        assert!(
            results
                .iter()
                .all(|(_, r)| r.as_ref().is_ok_and(|v| v.as_str() == "done"))
        );
        assert_eq!(
            results.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            vec!["S0", "S1", "S2", "S3"]
        );
    }
}
