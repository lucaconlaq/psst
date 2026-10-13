//! Deterministic private fixture; never reads SSH_AUTH_SOCK or private-key
//! files.
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ed25519_dalek::{Signer, SigningKey};
use psst::providers::local_storage::{Agent, PublicKey};
use sha2::{Digest, Sha512};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Notify, Semaphore};
use tokio::task::{JoinHandle, JoinSet};

#[derive(Clone, Copy)]
pub enum Fault {
    None = 0,
    Refuse,
    Disconnect,
    Malformed,
    Oversized,
    InvalidSignature,
    WrongAlgorithm,
    Truncated,
    Trailing,
    ShortSignature,
    EmptyFrame,
    Nonrepeatable,
    DuplicateIdentity,
    InvalidIdentity,
    ExcessIdentities,
}

pub struct FakeAgent {
    _dir: tempfile::TempDir,
    task: JoinHandle<()>,
    pub client: Agent,
    pub key: PublicKey,
    pub state: Arc<State>,
}

pub struct State {
    pub signs: AtomicUsize,
    pub connections: AtomicUsize,
    pub closed: AtomicUsize,
    pub fault: AtomicUsize,
    pub gate: Semaphore,
    pub requested: Notify,
    pub delay: AtomicUsize,
    pub identities: std::sync::Mutex<Vec<Vec<u8>>>,
    pub listing_gate: Semaphore,
    pub listing_requested: Notify,
    pub delay_listing: AtomicUsize,
    pub listing_fault: AtomicUsize,
}

impl FakeAgent {
    pub async fn new() -> Self {
        Self::with_seed(7).await
    }

    pub async fn with_seed(seed: u8) -> Self {
        Self::with_seeds(&[seed]).await
    }

    // Only subprocess fixtures need the endpoint path.
    #[allow(dead_code)]
    pub fn socket(&self) -> std::path::PathBuf {
        self._dir.path().join("agent.sock")
    }

    pub async fn with_seeds(seeds: &[u8]) -> Self {
        let seed = seeds[0];
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("agent.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let signing = SigningKey::from_bytes(&[seed; 32]);
        let mut blob = Vec::new();
        string(&mut blob, b"ssh-ed25519");
        string(&mut blob, signing.verifying_key().as_bytes());
        let key = PublicKey::from_blob(&blob).unwrap();
        let state = Arc::new(State {
            signs: AtomicUsize::new(0),
            connections: AtomicUsize::new(0),
            closed: AtomicUsize::new(0),
            fault: AtomicUsize::new(0),
            gate: Semaphore::new(0),
            requested: Notify::new(),
            delay: AtomicUsize::new(0),
            listing_gate: Semaphore::new(0),
            listing_requested: Notify::new(),
            delay_listing: AtomicUsize::new(0),
            listing_fault: AtomicUsize::new(0),
            identities: std::sync::Mutex::new(
                seeds
                    .iter()
                    .map(|seed| {
                        let signing = SigningKey::from_bytes(&[*seed; 32]);
                        let mut blob = Vec::new();
                        string(&mut blob, b"ssh-ed25519");
                        string(&mut blob, signing.verifying_key().as_bytes());
                        blob
                    })
                    .collect(),
            ),
        });
        let signers: Vec<_> = seeds
            .iter()
            .map(|seed| SigningKey::from_bytes(&[*seed; 32]))
            .collect();
        let shared = state.clone();
        let task = tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (stream, _) = accepted.unwrap();
                        shared.connections.fetch_add(1, Ordering::SeqCst);
                        let state = shared.clone();
                        let blob = blob.clone();
                        let signers = signers.clone();
                        tasks.spawn(async move {
                            let _ = serve(stream, &signers, &blob, &state).await;
                            state.closed.fetch_add(1, Ordering::SeqCst);
                        });
                    }
                    _ = tasks.join_next(), if !tasks.is_empty() => {}
                }
            }
        });
        Self {
            _dir: dir,
            task,
            client: Agent::at(socket),
            key,
            state,
        }
    }

    pub fn fault(&self, fault: Fault) {
        self.state.fault.store(fault as usize, Ordering::SeqCst);
    }
    pub fn signs(&self) -> usize {
        self.state.signs.load(Ordering::SeqCst)
    }
    pub fn delay(&self) {
        while let Ok(permit) = self.state.gate.try_acquire() {
            permit.forget();
        }
        self.state.delay.store(1, Ordering::SeqCst);
    }
    pub fn release(&self) {
        self.state.delay.store(0, Ordering::SeqCst);
        self.state.gate.add_permits(1);
    }
    pub async fn requested(&self) {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.state.requested.notified(),
        )
        .await
        .unwrap();
    }
}

impl Drop for FakeAgent {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn string(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
    bytes.extend_from_slice(value);
}

fn take_string<'a>(bytes: &mut &'a [u8]) -> &'a [u8] {
    let size = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
    let value = &bytes[4..4 + size];
    *bytes = &bytes[4 + size..];
    value
}

async fn serve(
    mut stream: UnixStream,
    signers: &[SigningKey],
    blob: &[u8],
    state: &State,
) -> std::io::Result<()> {
    loop {
        let size = stream.read_u32().await? as usize;
        assert!(size < 256 * 1024);
        let mut request = vec![0; size];
        stream.read_exact(&mut request).await?;
        let mut reply = Vec::new();
        match request[0] {
            11 => {
                if state.delay_listing.load(Ordering::SeqCst) != 0 {
                    state.listing_requested.notify_one();
                    let mut byte = [0];
                    tokio::select! {
                        permit = state.listing_gate.acquire() => { permit.unwrap().forget(); }
                        read = stream.read(&mut byte) => { assert_eq!(read?, 0); return Ok(()); }
                    }
                }
                let listing_fault = state.listing_fault.load(Ordering::SeqCst);
                let fault = if listing_fault == 0 {
                    state.fault.load(Ordering::SeqCst)
                } else {
                    listing_fault
                };
                if fault == Fault::Disconnect as usize {
                    return Ok(());
                }
                if fault == Fault::Oversized as usize {
                    stream.write_u32(256 * 1024 + 1).await?;
                    return Ok(());
                }
                if fault == Fault::Refuse as usize {
                    reply.push(5);
                } else {
                    reply.push(12);
                    reply.extend_from_slice(
                        &if fault == Fault::ExcessIdentities as usize {
                            1025u32
                        } else {
                            state.identities.lock().unwrap().len() as u32 + 1
                        }
                        .to_be_bytes(),
                    );
                    for identity in state.identities.lock().unwrap().iter() {
                        string(
                            &mut reply,
                            if fault == Fault::InvalidIdentity as usize {
                                &identity[..50]
                            } else {
                                identity
                            },
                        );
                        string(&mut reply, b"test Ed25519");
                    }
                    let mut unsupported = Vec::new();
                    string(&mut unsupported, b"ssh-rsa");
                    string(
                        &mut reply,
                        if fault == Fault::DuplicateIdentity as usize {
                            blob
                        } else {
                            &unsupported
                        },
                    );
                    string(&mut reply, b"unsupported");
                    if fault == Fault::Trailing as usize {
                        reply.push(0);
                    }
                }
            }
            13 => {
                state.signs.fetch_add(1, Ordering::SeqCst);
                state.requested.notify_one();
                let mut fields = &request[1..];
                let requested = PublicKey::from_blob(take_string(&mut fields)).unwrap();
                let signing = signers
                    .iter()
                    .find(|signing| {
                        let mut blob = Vec::new();
                        string(&mut blob, b"ssh-ed25519");
                        string(&mut blob, signing.verifying_key().as_bytes());
                        PublicKey::from_blob(&blob).unwrap() == requested
                    })
                    .unwrap();
                let challenge = take_string(&mut fields);
                assert_eq!(fields, [0; 4]);
                if state.delay.load(Ordering::SeqCst) != 0 {
                    let mut byte = [0];
                    tokio::select! {
                        permit = state.gate.acquire() => { permit.unwrap().forget(); }
                        read = stream.read(&mut byte) => { assert_eq!(read?, 0); return Ok(()); }
                    }
                }
                let fault = state.fault.load(Ordering::SeqCst);
                if fault == Fault::Disconnect as usize {
                    return Ok(());
                }
                if fault == Fault::Oversized as usize {
                    stream.write_u32(256 * 1024 + 1).await?;
                    return Ok(());
                }
                if fault == Fault::EmptyFrame as usize {
                    stream.write_u32(0).await?;
                    return Ok(());
                }
                if fault == Fault::Truncated as usize {
                    stream.write_u32(99).await?;
                    stream.write_all(&[14]).await?;
                    return Ok(());
                }
                if fault == Fault::Refuse as usize {
                    reply.push(5);
                } else if fault == Fault::Malformed as usize {
                    reply.extend_from_slice(&[14, 0, 0, 0, 99]);
                } else {
                    reply.push(14);
                    let mut sig = signing.sign(challenge).to_bytes();
                    if fault == Fault::Nonrepeatable as usize {
                        // Only this public test seed uses the library's hazmat API.
                        // Vary its nonce prefix to model valid non-deterministic
                        // Ed25519 signing, while retaining the same public key.
                        let mut expanded = ed25519_dalek::hazmat::ExpandedSecretKey::from_bytes(
                            &Sha512::digest([7; 32]).into(),
                        );
                        expanded.hash_prefix[0] ^= state.signs.load(Ordering::SeqCst) as u8;
                        let signature = ed25519_dalek::hazmat::raw_sign::<Sha512>(
                            &expanded,
                            challenge,
                            &signing.verifying_key(),
                        );
                        signing
                            .verifying_key()
                            .verify_strict(challenge, &signature)
                            .unwrap();
                        sig = signature.to_bytes();
                    }
                    if fault == Fault::InvalidSignature as usize {
                        sig[0] ^= 1;
                    }
                    let mut encoded = Vec::new();
                    string(
                        &mut encoded,
                        if fault == Fault::WrongAlgorithm as usize {
                            b"ssh-rsa"
                        } else {
                            b"ssh-ed25519"
                        },
                    );
                    string(
                        &mut encoded,
                        if fault == Fault::ShortSignature as usize {
                            &sig[..63]
                        } else {
                            &sig
                        },
                    );
                    string(&mut reply, &encoded);
                    if fault == Fault::Trailing as usize {
                        reply.push(0);
                    }
                }
            }
            _ => panic!("Unexpected agent request"),
        }
        stream.write_u32(reply.len() as u32).await?;
        stream.write_all(&reply).await?;
    }
}
