//! Experimental local-storage provider, v1 envelope, agent boundary, and
//! persistence.
//! See docs/ARCHITECTURE.md for byte encodings and security limitations.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::future::Future;
use std::io::{self, Write};
use std::marker::PhantomData;
use std::path::Path;

use aes_gcm::aead::{AeadInPlace, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result, bail, ensure};
use data_encoding::{BASE64, BASE64_NOPAD, HEXLOWER};
use ed25519_dalek::VerifyingKey;
use futures::FutureExt;
use futures::future::{BoxFuture, Shared};
use hkdf::Hkdf;
use rand_core::{OsRng, RngCore};
use serde::de::{MapAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

use super::{Author, FetchFuture, Identity, PreparedDelete, PreparedSave, Provider};
use crate::browser::{BrowserLevel, Choice, Level, Target};

pub use agent::Agent;
pub use storage::StoredKey;

mod agent;
mod storage;

pub(super) struct LocalStorage;

impl Provider for LocalStorage {
    fn id(&self) -> &'static str {
        "local-storage"
    }

    fn reference_label(&self) -> &'static str {
        "Local storage ID · Ctrl+B browse"
    }

    fn browser(&self) -> Option<Level> {
        Some(Level::new(LocalStorage))
    }

    fn author(&self) -> Option<&'static dyn Author> {
        Some(&LocalStorage)
    }

    fn fetch<'a>(&'a self, value: &'a str, dir: &'a Path) -> FetchFuture<'a> {
        Box::pin(async move { Operation::default().fetch(value, dir).await })
    }
}

impl BrowserLevel for LocalStorage {
    fn title(&self) -> String {
        "Local storage IDs".into()
    }

    fn list<'a>(&'a self, dir: &'a Path) -> BoxFuture<'a, Result<Vec<Choice>>> {
        Box::pin(async move {
            Ok(storage::list(dir)?
                .into_iter()
                .map(|(id, fingerprint)| Choice {
                    target: Target::Reference(format!("{id}#{fingerprint}")),
                    label: id,
                    detail: fingerprint,
                })
                .collect())
        })
    }
}

impl Author for LocalStorage {
    fn identities(&self) -> BoxFuture<'_, Result<Vec<Identity>>> {
        Box::pin(async {
            let keys = Agent::from_env()?.identities().await?;
            ensure!(!keys.is_empty(), "No eligible Ed25519 SSH agent keys");
            Ok(keys
                .into_iter()
                .map(|key| Identity {
                    fingerprint: key.fingerprint(),
                    token: key.blob(),
                })
                .collect())
        })
    }

    fn prepare(&self, dir: &Path, id: &str, identity: &Identity) -> Result<Box<dyn PreparedSave>> {
        let (id, selector) = reference(id)?;
        let key = PublicKey::from_blob(&identity.token)?;
        ensure!(
            selector.is_none_or(|s| s == key.fingerprint()),
            "Reference selector does not match selected SSH identity"
        );
        ensure!(
            key.fingerprint() == identity.fingerprint,
            "Mismatched selected identity"
        );
        let stored = StoredKey::load(dir, key.clone())?;
        let replacing = stored.records().iter().any(|record| record.id == id);
        Ok(Box::new(Creation {
            dir: dir.to_owned(),
            id: id.to_owned(),
            key,
            stored,
            replacing,
        }))
    }

    fn prepare_deletion(
        &self,
        dir: &Path,
        value: &str,
    ) -> BoxFuture<'static, Result<Box<dyn PreparedDelete>>> {
        let dir = dir.to_owned();
        let value = value.to_owned();
        Box::pin(async move {
            let (id, selector) = reference(&value)?;
            let agent = Agent::from_env()?;
            phase(async {
                let keys = agent.identities().await?;
                let mut selected = None;
                for key in keys {
                    if selector.is_some_and(|s| s != key.fingerprint()) {
                        continue;
                    }
                    let stored = StoredKey::load(&dir, key)?;
                    if stored.records().iter().any(|record| record.id == id) {
                        ensure!(
                            selected.is_none(),
                            "Ambiguous local-storage identity for ID; ciphertext kept"
                        );
                        selected = Some(stored);
                    }
                }
                let stored = selected
                    .context("No loaded eligible key with a stored value; ciphertext kept")?;
                Ok(Box::new(Deletion {
                    id: id.to_owned(),
                    stored,
                }) as Box<dyn PreparedDelete>)
            })
            .await
        })
    }

    fn prepare_browser_deletion(&self, dir: &Path, value: &str) -> Result<Box<dyn PreparedDelete>> {
        let (id, selector) = reference(value)?;
        let fingerprint = selector.context("Browser deletion requires a qualified reference")?;
        let stored = storage::load_selected(dir, fingerprint)?;
        ensure!(
            stored.records().iter().any(|record| record.id == id),
            "No stored value for selected local-storage reference"
        );
        Ok(Box::new(Deletion {
            id: id.to_owned(),
            stored,
        }))
    }

    fn may_share(&self, left: &str, right: &str) -> bool {
        match (reference(left), reference(right)) {
            (Ok((left, a)), Ok((right, b))) => {
                left == right && (a.is_none() || b.is_none() || a == b)
            }
            _ => false,
        }
    }

    fn value_limit(&self) -> usize {
        VALUE_LIMIT
    }
}

struct Deletion {
    id: String,
    stored: StoredKey,
}

impl PreparedDelete for Deletion {
    fn remove(
        &mut self,
        boundary: &mut dyn FnMut(crate::config::WriteBoundary) -> Result<()>,
    ) -> Result<()> {
        self.stored.remove_checked(&self.id, boundary)
    }
}

struct Creation {
    dir: std::path::PathBuf,
    id: String,
    key: PublicKey,
    stored: StoredKey,
    replacing: bool,
}

impl PreparedSave for Creation {
    fn replacing(&self) -> bool {
        self.replacing
    }

    fn matches_reference(&self, value: &str) -> bool {
        reference(value).is_ok_and(|(id, selector)| {
            id == self.id && selector.is_none_or(|s| s == self.key.fingerprint())
        })
    }

    fn save(mut self: Box<Self>, value: Zeroizing<String>) -> BoxFuture<'static, Result<String>> {
        Box::pin(async move {
            let agent = Agent::from_env()?;
            // One deadline includes re-listing, queued signing, and encryption.
            phase(async {
                let keys = agent.identities().await?;
                ensure!(
                    keys.contains(&self.key),
                    "Selected SSH agent key is no longer loaded"
                );
                // Validate other loaded keys before requesting signatures.
                for key in keys.iter().filter(|key| **key != self.key) {
                    StoredKey::load(&self.dir, key.clone())?;
                }
                let keys = self
                    .stored
                    .set_validated(&agent, &self.id, &value, async {
                        let keys = agent.identities().await?;
                        ensure!(
                            keys.contains(&self.key),
                            "Selected SSH agent key is no longer loaded"
                        );
                        Ok(keys)
                    })
                    .await?;
                // Approval may change identities and records. Use refreshed keys
                // and recheck records without awaiting after replacement.
                self.authored_reference(&keys)
            })
            .await
        })
    }
}

impl Creation {
    fn authored_reference(&self, keys: &[PublicKey]) -> Result<String> {
        let mut ambiguous = false;
        for key in keys.iter().filter(|key| **key != self.key) {
            let stored = StoredKey::load(&self.dir, key.clone()).map_err(|source| {
                anyhow::Error::from(crate::config::PersistenceError {
                    changed: true,
                    source: source.context("Ciphertext saved; reference validation failed"),
                })
            })?;
            ambiguous |= stored.records().iter().any(|record| record.id == self.id);
        }
        Ok(if ambiguous {
            format!("{}#{}", self.id, self.key.fingerprint())
        } else {
            self.id.clone()
        })
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
        phase(async {
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

/// Maximum encoded per-key JSON bytes, on both reads and writes.
pub const FILE_LIMIT: usize = 4 * 1024 * 1024;
/// Maximum exact UTF-8 secret bytes (before encryption).
pub const VALUE_LIMIT: usize = 1024 * 1024;
/// Maximum exact UTF-8 ID bytes.
pub const ID_LIMIT: usize = 256;
const RECORD_LIMIT: usize = 1024;
const VERSION: u32 = 1;

/// Validated Ed25519 public identity, never private-key material.
#[derive(Clone, PartialEq, Eq)]
pub struct PublicKey([u8; 32]);

impl PublicKey {
    /// Parse the complete standard SSH public-key blob, not an authorized_keys
    /// line.
    pub fn from_blob(blob: &[u8]) -> Result<Self> {
        let mut reader = agent::Reader::new(blob);
        ensure!(
            reader.string()? == b"ssh-ed25519",
            "Unsupported SSH key type"
        );
        let key = Self(
            reader
                .string()?
                .try_into()
                .context("Invalid Ed25519 key size")?,
        );
        reader.finish()?;
        key.verifier()?;
        Ok(key)
    }

    fn verifier(&self) -> Result<VerifyingKey> {
        let key = VerifyingKey::from_bytes(&self.0).context("Invalid Ed25519 public key")?;
        ensure!(!key.is_weak(), "Weak Ed25519 public key");
        Ok(key)
    }

    /// Canonical SSH blob: string("ssh-ed25519"), string(32 key bytes).
    pub fn blob(&self) -> Vec<u8> {
        let mut blob = Vec::with_capacity(51);
        agent::put_string(&mut blob, b"ssh-ed25519");
        agent::put_string(&mut blob, &self.0);
        blob
    }

    /// OpenSSH SHA256 fingerprint of the entire SSH blob, without base64 padding.
    pub fn fingerprint(&self) -> String {
        format!(
            "SHA256:{}",
            BASE64_NOPAD.encode(&Sha256::digest(self.blob()))
        )
    }

    /// Lowercase hex SHA256 of the SSH blob, with .json suffix;
    /// IDs are never paths.
    pub fn filename(&self) -> String {
        format!("{}.json", HEXLOWER.encode(&Sha256::digest(self.blob())))
    }
}

/// Validate an exact, case-sensitive logical ID without normalizing it.
pub fn validate_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty() && id.len() <= ID_LIMIT,
        "Invalid local-storage ID size"
    );
    ensure!(
        !id.contains('#') && !id.chars().any(char::is_control),
        "Invalid local-storage ID"
    );
    Ok(())
}

fn validate_value(value: &str) -> Result<()> {
    ensure!(value.len() <= VALUE_LIMIT, "Local-storage value too large");
    ensure!(!value.contains('\0'), "Local-storage value contains NUL");
    Ok(())
}

/// One authenticated ciphertext record.
/// Binary fields use canonical padded base64.
#[derive(Clone, Serialize)]
pub struct Envelope {
    pub version: u32,
    pub id: String,
    pub public_key: String,
    pub salt: String,
    pub nonce: String,
    /// AES-GCM ciphertext followed by the 16-byte authentication tag.
    pub ciphertext: String,
}

impl<'de> Deserialize<'de> for Envelope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            version: u32,
            id: String,
            public_key: String,
            salt: String,
            nonce: String,
            ciphertext: String,
        }

        let fields: Fields = deserialize_object(deserializer)?;
        Ok(Self {
            version: fields.version,
            id: fields.id,
            public_key: fields.public_key,
            salt: fields.salt,
            nonce: fields.nonce,
            ciphertext: fields.ciphertext,
        })
    }
}

// Dispatch only maps to the derived field parser, preserving duplicate
// rejection.
// Do not round-trip through Value, which would discard duplicate JSON fields.
fn deserialize_object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Object<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for Object<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("an object with named fields")
        }

        fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<T, M::Error> {
            T::deserialize(MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_map(Object(PhantomData))
}

struct Validated {
    salt: [u8; 32],
    nonce: [u8; 12],
    ciphertext: Vec<u8>,
    aad: Vec<u8>,
    challenge: Vec<u8>,
}

fn decode(value: &str, min: usize, max: usize) -> Result<Vec<u8>> {
    ensure!(
        value.len() <= max.div_ceil(3) * 4,
        "Oversized encoded field"
    );
    let bytes = BASE64
        .decode(value.as_bytes())
        .context("Invalid canonical base64")?;
    ensure!(
        (min..=max).contains(&bytes.len()),
        "Invalid binary field size"
    );
    ensure!(BASE64.encode(&bytes) == value, "Noncanonical base64");
    Ok(bytes)
}

impl Envelope {
    fn validate(&self, key: &PublicKey) -> Result<Validated> {
        ensure!(
            self.version == VERSION,
            "Unknown local-storage envelope version"
        );
        validate_id(&self.id)?;
        ensure!(
            decode(&self.public_key, 51, 51)? == key.blob(),
            "Mismatched envelope key"
        );
        let salt: [u8; 32] = decode(&self.salt, 32, 32)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid salt size"))?;
        let nonce: [u8; 12] = decode(&self.nonce, 12, 12)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid nonce size"))?;
        let ciphertext = decode(&self.ciphertext, 16, VALUE_LIMIT + 16)?;
        let mut aad = b"psst/local-storage/aad\0".to_vec();
        aad.extend_from_slice(&VERSION.to_be_bytes());
        agent::put_string(&mut aad, &key.blob());
        agent::put_string(&mut aad, self.id.as_bytes());
        aad.extend_from_slice(&salt);
        aad.extend_from_slice(&nonce);
        let mut challenge = b"psst/local-storage/sign\0".to_vec();
        challenge.extend_from_slice(&aad);
        Ok(Validated {
            salt,
            nonce,
            ciphertext,
            aad,
            challenge,
        })
    }

    /// Encrypt exact text with two verified identical signatures in one phase.
    /// Cancelling by dropping the future closes its socket and releases the queue.
    pub async fn encrypt(agent: &Agent, key: &PublicKey, id: &str, value: &str) -> Result<Self> {
        validate_id(id)?;
        validate_value(value)?;
        let mut salt = [0; 32];
        let mut nonce = [0; 12];
        OsRng
            .try_fill_bytes(&mut salt)
            .map_err(|_| anyhow::anyhow!("Cannot generate encryption salt"))?;
        OsRng
            .try_fill_bytes(&mut nonce)
            .map_err(|_| anyhow::anyhow!("Cannot generate encryption nonce"))?;
        let mut envelope = Self {
            version: VERSION,
            id: id.to_owned(),
            public_key: BASE64.encode(&key.blob()),
            salt: BASE64.encode(&salt),
            nonce: BASE64.encode(&nonce),
            ciphertext: BASE64.encode(&[0; 16]),
        };
        let validated = envelope.validate(key)?;
        // Reserve the tag too, so appending it cannot free an unwiped plaintext
        // allocation.
        let mut buffer = Zeroizing::new(Vec::with_capacity(value.len() + 16));
        buffer.extend_from_slice(value.as_bytes());
        phase(async {
            let _guard = agent::SIGNING.lock().await;
            let mut socket = agent.connect().await?;
            let first = agent::sign(&mut socket, key, &validated.challenge).await?;
            let second = agent::sign(&mut socket, key, &validated.challenge).await?;
            check_repeatability(&first, &second)?;
            let derived = derive(&first, &validated)?;
            let cipher = Aes256Gcm::new_from_slice(derived.as_ref()).context("Invalid AES key")?;
            cipher
                .encrypt_in_place(Nonce::from_slice(&nonce), &validated.aad, &mut *buffer)
                .map_err(|_| anyhow::anyhow!("Local-storage encryption failed"))?;
            Ok(())
        })
        .await?;
        envelope.ciphertext = BASE64.encode(&buffer);
        Ok(envelope)
    }

    /// Validate ID/key/metadata/sizes before signing, then decrypt exact UTF-8
    /// text.
    pub async fn decrypt(
        &self,
        agent: &Agent,
        key: &PublicKey,
        id: &str,
    ) -> Result<Zeroizing<String>> {
        validate_id(id)?;
        ensure!(self.id == id, "Mismatched envelope ID");
        let validated = self.validate(key)?;
        phase(async {
            let _guard = agent::SIGNING.lock().await;
            let mut socket = agent.connect().await?;
            let signature = agent::sign(&mut socket, key, &validated.challenge).await?;
            let derived = derive(&signature, &validated)?;
            let cipher = Aes256Gcm::new_from_slice(derived.as_ref()).context("Invalid AES key")?;
            let mut buffer = Zeroizing::new(validated.ciphertext.clone());
            cipher
                .decrypt_in_place(
                    Nonce::from_slice(&validated.nonce),
                    &validated.aad,
                    &mut *buffer,
                )
                .map_err(|_| anyhow::anyhow!("Local-storage authentication failed"))?;
            let text =
                std::str::from_utf8(&buffer).context("Local-storage plaintext is not UTF-8")?;
            validate_value(text)?;
            Ok(Zeroizing::new(text.to_owned()))
        })
        .await
    }
}

fn check_repeatability(first: &[u8; 64], second: &[u8; 64]) -> Result<()> {
    ensure!(first == second, "SSH agent signature is not repeatable");
    Ok(())
}

fn derive(signature: &[u8; 64], value: &Validated) -> Result<Zeroizing<[u8; 32]>> {
    let mut key = Zeroizing::new([0; 32]);
    let mut info = b"psst/local-storage/hkdf\0".to_vec();
    info.extend_from_slice(&value.aad);
    Hkdf::<Sha256>::new(Some(&value.salt), signature)
        .expand(&info, key.as_mut())
        .map_err(|_| anyhow::anyhow!("Key derivation failed"))?;
    Ok(key)
}

pub(crate) async fn phase<T>(work: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(crate::process::timeout(), work)
        .await
        .context("SSH agent phase timed out (PSST_TIMEOUT controls seconds)")?
}

/// One per-key JSON document containing separate IDs. No filesystem IO here.
#[derive(Clone, Serialize)]
pub struct KeyFile {
    pub version: u32,
    pub public_key: String,
    pub fingerprint: String,
    pub records: Vec<Envelope>,
}

impl<'de> Deserialize<'de> for KeyFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            version: u32,
            public_key: String,
            fingerprint: String,
            records: Vec<Envelope>,
        }

        let fields: Fields = deserialize_object(deserializer)?;
        Ok(Self {
            version: fields.version,
            public_key: fields.public_key,
            fingerprint: fields.fingerprint,
            records: fields.records,
        })
    }
}

impl KeyFile {
    /// Empty v1 document for a validated identity.
    pub fn new(key: &PublicKey) -> Self {
        Self {
            version: VERSION,
            public_key: BASE64.encode(&key.blob()),
            fingerprint: key.fingerprint(),
            records: Vec::new(),
        }
    }

    /// Parse bounded JSON, rejecting duplicate fields/IDs and key/path mismatches.
    pub fn parse(bytes: &[u8], key: &PublicKey, filename: &str) -> Result<Self> {
        ensure!(
            bytes.len() <= FILE_LIMIT,
            "Local-storage key file exceeds 4 MiB"
        );
        let file: Self =
            serde_json::from_slice(bytes).context("Malformed local-storage key file")?;
        ensure!(
            filename == key.filename(),
            "Mismatched local-storage filename"
        );
        file.validate(key)?;
        Ok(file)
    }

    fn validate(&self, key: &PublicKey) -> Result<()> {
        ensure!(
            self.version == VERSION,
            "Unknown local-storage file version"
        );
        ensure!(
            decode(&self.public_key, 51, 51)? == key.blob(),
            "Mismatched file key"
        );
        ensure!(
            self.fingerprint == key.fingerprint(),
            "Mismatched file fingerprint"
        );
        ensure!(
            self.records.len() <= RECORD_LIMIT,
            "Too many local-storage records"
        );
        let mut ids = HashSet::new();
        for record in &self.records {
            record.validate(key)?;
            ensure!(ids.insert(&record.id), "Duplicate local-storage ID");
        }
        Ok(())
    }

    /// Validate the complete file and its final encoded size before persistence.
    pub fn encode(&self, key: &PublicKey) -> Result<Vec<u8>> {
        self.validate(key)?;
        let mut writer = BoundedJson(Vec::new());
        serde_json::to_writer(&mut writer, self)
            .context("Cannot encode bounded local-storage file")?;
        Ok(writer.0)
    }
}

// Stop serialization at the bound, rather than allocating an arbitrarily large
// aggregate of otherwise valid records and only then checking its size.
struct BoundedJson(Vec<u8>);

impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > FILE_LIMIT - self.0.len() {
            return Err(io::Error::other("Local-storage key file exceeds 4 MiB"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_encoding_and_derivation_vector() {
        let file: KeyFile = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/local-storage-v1.json"
        ))
        .unwrap();
        let key =
            PublicKey::from_blob(&BASE64.decode(file.public_key.as_bytes()).unwrap()).unwrap();
        let record = file.records[0].validate(&key).unwrap();
        let aad = concat!(
            "707373742f6c6f63616c2d73746f726167652f616164000000000100000033",
            "0000000b7373682d6564323535313900000020",
            "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c",
            "0000000766697874757265",
            "0101010101010101010101010101010101010101010101010101010101010101",
            "020202020202020202020202",
        );
        assert_eq!(HEXLOWER.encode(&record.aad), aad);
        assert_eq!(
            HEXLOWER.encode(&record.challenge),
            format!("707373742f6c6f63616c2d73746f726167652f7369676e00{aad}")
        );
        let signature = HEXLOWER
            .decode(
                concat!(
                    "8336f0db0be5dbe97128aa502b26b5a9fd39bb6c51a81b3b1a32ecf44148f9909",
                    "274678f82a2afd55bc4a2817fcf362d6cd9d3e939e980aef7fbd666a069ee04",
                )
                .as_bytes(),
            )
            .unwrap();
        let signature: [u8; 64] = signature.try_into().unwrap();
        key.verifier()
            .unwrap()
            .verify_strict(
                &record.challenge,
                &ed25519_dalek::Signature::from_bytes(&signature),
            )
            .unwrap();
        assert_eq!(
            HEXLOWER.encode(derive(&signature, &record).unwrap().as_ref()),
            "81e0bb0d85aadb08630da834df74405d81b05bde51fca33ff7cea2419798bf0d"
        );
    }

    #[test]
    fn repeatability_guard() {
        assert!(check_repeatability(&[1; 64], &[1; 64]).is_ok());
        assert!(check_repeatability(&[1; 64], &[2; 64]).is_err());
    }
}
