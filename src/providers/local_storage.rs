use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use data_encoding::BASE64_NOPAD;
use futures::FutureExt;
use futures::future::{BoxFuture, Shared};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

use super::{FetchFuture, Provider};
use crate::local_storage::{Agent, PublicKey, StoredKey, validate_id};

pub(super) struct LocalStorage;

impl Provider for LocalStorage {
    fn id(&self) -> &'static str {
        "local-storage"
    }

    fn reference_label(&self) -> &'static str {
        "Local storage · secret-id[#SHA256:fingerprint]"
    }

    fn fetch<'a>(&'a self, value: &'a str, dir: &'a Path) -> FetchFuture<'a> {
        Box::pin(async move { Operation::default().fetch(value, dir).await })
    }
}

fn reference(value: &str) -> Result<(&str, Option<&str>)> {
    let (id, selector) = match value.split_once('#') {
        Some((id, selector)) => {
            let encoded = selector
                .strip_prefix("SHA256:")
                .context("Expected OpenSSH SHA256 fingerprint")?;
            let bytes = BASE64_NOPAD
                .decode(encoded.as_bytes())
                .context("Invalid OpenSSH fingerprint")?;
            ensure!(
                bytes.len() == 32 && BASE64_NOPAD.encode(&bytes) == encoded,
                "Invalid OpenSSH fingerprint"
            );
            (id, Some(selector))
        }
        None => (value, None),
    };
    validate_id(id)?;
    Ok((id, selector))
}

type Work<T> = Shared<BoxFuture<'static, Result<T, String>>>;

#[derive(Default)]
struct State {
    keys: Option<Work<Vec<PublicKey>>>,
    values: HashMap<(String, String), Work<Zeroizing<String>>>,
}

// Only bulk dispatch shares this context. Single fetches and UI refetches
// create fresh contexts. Zeroizing cached values are discarded with the
// operation.
#[derive(Default)]
pub(super) struct Operation(Mutex<State>);

impl Operation {
    pub(super) async fn fetch(&self, value: &str, dir: &Path) -> Result<Zeroizing<String>> {
        let (id, selector) = reference(value)?;
        let agent = Agent::from_env()?;
        // Includes operation queueing, identity listing and signing in one budget.
        crate::local_storage::phase(async {
            let mut state = self.0.lock().await;
            let listing_agent = agent.clone();
            let keys = state
                .keys
                .get_or_insert_with(|| {
                    async move {
                        listing_agent
                            .identities()
                            .await
                            .map_err(|error| format!("{error:#}"))
                    }
                    .boxed()
                    .shared()
                })
                .clone();
            drop(state);
            let keys = keys.await.map_err(|error| anyhow::anyhow!(error))?;
            let mut selected = None;
            for key in &keys {
                if selector.is_some_and(|selector| selector != key.fingerprint()) {
                    continue;
                }
                let stored = StoredKey::load(dir, key.clone())?;
                if let Some(record) = stored.records().iter().find(|record| record.id == id) {
                    ensure!(
                        selected.is_none(),
                        "Ambiguous local-storage identity for ID"
                    );
                    selected = Some((key.clone(), record.clone()));
                }
            }
            let Some((key, record)) = selected else {
                bail!("No loaded eligible key with a stored value for local-storage ID");
            };
            let identity = (id.to_owned(), key.fingerprint());
            let mut state = self.0.lock().await;
            let id = id.to_owned();
            let work = state
                .values
                .entry(identity)
                .or_insert_with(|| {
                    async move {
                        record
                            .decrypt(&agent, &key, &id)
                            .await
                            .map_err(|error| format!("{error:#}"))
                    }
                    .boxed()
                    .shared()
                })
                .clone();
            drop(state);
            work.await.map_err(|error| anyhow::anyhow!(error))
        })
        .await
    }
}
