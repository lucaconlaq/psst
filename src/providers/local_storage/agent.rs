use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::Signature;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use zeroize::Zeroizing;

use super::PublicKey;

// Hold across a whole phase, including the repeatability check. Dropping a
// future drops its socket and guard; the next phase always opens a new socket.
pub(super) static SIGNING: Mutex<()> = Mutex::const_new(());
const FRAME_LIMIT: usize = 256 * 1024;
const KEY_LIMIT: usize = 1024;

/// Unix SSH-agent endpoint. No discovery, startup, retries, or fallback.
#[derive(Clone)]
pub struct Agent {
    path: PathBuf,
}

impl Agent {
    /// Prefer PSST_SSH_AUTH_SOCK when set, otherwise use SSH_AUTH_SOCK.
    /// An empty selected value is an error, never a request to fall back.
    pub fn from_env() -> Result<Self> {
        let (name, path) = match std::env::var_os("PSST_SSH_AUTH_SOCK") {
            Some(path) => ("PSST_SSH_AUTH_SOCK", path),
            None => (
                "SSH_AUTH_SOCK",
                std::env::var_os("SSH_AUTH_SOCK").context("SSH_AUTH_SOCK is not set")?,
            ),
        };
        ensure!(!path.is_empty(), "{name} is empty");
        Ok(Self::at(PathBuf::from(path)))
    }

    /// Explicit endpoint, useful for isolated fake-agent tests.
    pub fn at(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_owned(),
        }
    }

    pub(super) async fn connect(&self) -> Result<UnixStream> {
        UnixStream::connect(&self.path)
            .await
            .context("Cannot connect to SSH agent")
    }

    /// List eligible Ed25519 identities in one bounded agent-work phase.
    /// Unsupported types are ignored; malformed Ed25519 identities are errors.
    pub async fn identities(&self) -> Result<Vec<PublicKey>> {
        super::phase(async {
            let _guard = SIGNING.lock().await;
            let mut socket = self.connect().await?;
            let reply = exchange(&mut socket, &[11]).await?;
            let mut reader = Reader::new(&reply);
            ensure!(reader.byte()? == 12, "SSH agent refused identity listing");
            let count = reader.u32()? as usize;
            ensure!(count <= KEY_LIMIT, "Too many SSH agent identities");
            let mut keys = Vec::new();
            for _ in 0..count {
                let blob = reader.string()?;
                let _comment = reader.string()?;
                let mut key_reader = Reader::new(blob);
                if key_reader.string()? == b"ssh-ed25519" {
                    let key = PublicKey::from_blob(blob)?;
                    ensure!(!keys.contains(&key), "Duplicate SSH agent identity");
                    keys.push(key);
                }
            }
            reader.finish()?;
            Ok(keys)
        })
        .await
    }
}

pub(super) async fn sign(
    socket: &mut UnixStream,
    key: &PublicKey,
    challenge: &[u8],
) -> Result<Zeroizing<[u8; 64]>> {
    let mut request = vec![13];
    put_string(&mut request, &key.blob());
    put_string(&mut request, challenge);
    request.extend_from_slice(&0u32.to_be_bytes());
    let reply = exchange(socket, &request).await?;
    let mut reader = Reader::new(&reply);
    ensure!(reader.byte()? == 14, "SSH agent refused signing");
    let encoded = reader.string()?;
    reader.finish()?;
    let mut signature = Reader::new(encoded);
    ensure!(
        signature.string()? == b"ssh-ed25519",
        "Unsupported SSH signature type"
    );
    let bytes: [u8; 64] = signature
        .string()?
        .try_into()
        .context("Invalid SSH signature size")?;
    let bytes = Zeroizing::new(bytes);
    signature.finish()?;
    key.verifier()?
        .verify_strict(challenge, &Signature::from_bytes(&bytes))
        .context("Invalid SSH agent signature")?;
    Ok(bytes)
}

async fn exchange(socket: &mut UnixStream, request: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    ensure!(request.len() <= FRAME_LIMIT, "SSH request too large");
    socket.write_u32(request.len() as u32).await?;
    socket.write_all(request).await?;
    let size = socket.read_u32().await? as usize;
    ensure!(
        (1..=FRAME_LIMIT).contains(&size),
        "Invalid SSH agent reply size"
    );
    let mut reply = Zeroizing::new(vec![0; size]);
    socket.read_exact(&mut reply).await?;
    Ok(reply)
}

pub(super) fn put_string(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
    bytes.extend_from_slice(value);
}

pub(super) struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        if count > self.bytes.len() {
            bail!("Truncated SSH agent data");
        }
        let (value, rest) = self.bytes.split_at(count);
        self.bytes = rest;
        Ok(value)
    }
    pub(super) fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    pub(super) fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into()?))
    }
    pub(super) fn string(&mut self) -> Result<&'a [u8]> {
        let count = self.u32()? as usize;
        self.take(count)
    }
    pub(super) fn finish(&self) -> Result<()> {
        ensure!(self.bytes.is_empty(), "Trailing SSH agent data");
        Ok(())
    }
}
