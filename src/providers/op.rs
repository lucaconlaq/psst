use super::{FetchFuture, Provider};
use crate::{
    browser::{BrowserLevel, Choice, Level, Target, json, text},
    process,
};
use anyhow::{Context, Result, bail};
use futures::future::BoxFuture;
use hmac::{Hmac, Mac};
use sha1::Sha1;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

// Share one queue across op and op 2FA so reads cannot race authentication.
static READ_GATE: Mutex<()> = Mutex::const_new(());

pub(super) struct Op {
    pub(super) otp: bool,
}
impl Provider for Op {
    fn id(&self) -> &'static str {
        if self.otp { "op 2FA" } else { "op" }
    }
    fn reference_label(&self) -> &'static str {
        "1Password reference · Ctrl+B browse"
    }
    fn browser(&self) -> Option<Level> {
        Some(Level::new(Location::Vaults))
    }
    fn fetch<'a>(&'a self, value: &'a str, dir: &'a Path) -> FetchFuture<'a> {
        Box::pin(async move {
            if !value.starts_with("op://") {
                bail!("1Password reference must start with op://");
            }
            let value = {
                let _guard = READ_GATE.lock().await;
                process::output("op", &["read", value], dir).await?
            };
            if value.is_empty() {
                bail!("1Password secret is empty");
            }
            if self.otp {
                Ok(Zeroizing::new(totp(
                    &value,
                    SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
                )?))
            } else {
                Ok(value)
            }
        })
    }
}

enum Location {
    Vaults,
    Items(String),
    Fields { vault: String, item: String },
}
impl BrowserLevel for Location {
    fn title(&self) -> String {
        match self {
            Self::Vaults => "1Password / Vaults".into(),
            Self::Items(_) => "1Password / Items".into(),
            Self::Fields { .. } => "1Password / Fields".into(),
        }
    }
    fn list<'a>(&'a self, dir: &'a Path) -> BoxFuture<'a, Result<Vec<Choice>>> {
        Box::pin(async move {
            let mut choices = Vec::new();
            match self {
                Self::Vaults => {
                    let data = json("op", &["vault", "list", "--format", "json"], dir).await?;
                    for v in data.as_array().context("Expected vault list")? {
                        choices.push(Choice {
                            label: text(v, "name"),
                            detail: "Vault".into(),
                            target: Target::Level(Level::new(Self::Items(text(v, "id")))),
                        });
                    }
                }
                Self::Items(vault) => {
                    let data = json(
                        "op",
                        &["item", "list", "--vault", vault, "--format", "json"],
                        dir,
                    )
                    .await?;
                    for v in data.as_array().context("Expected item list")? {
                        choices.push(Choice {
                            label: text(v, "title"),
                            detail: "Item".into(),
                            target: Target::Level(Level::new(Self::Fields {
                                vault: vault.clone(),
                                item: text(v, "id"),
                            })),
                        });
                    }
                }
                Self::Fields { vault, item } => {
                    let data = json(
                        "op",
                        &["item", "get", item, "--vault", vault, "--format", "json"],
                        dir,
                    )
                    .await?;
                    for v in data
                        .get("fields")
                        .and_then(|v| v.as_array())
                        .context("Expected item fields")?
                    {
                        let reference = text(v, "reference");
                        if reference.is_empty() {
                            continue;
                        }
                        let mut label = text(v, "label");
                        if label.is_empty() {
                            label = text(v, "id");
                        }
                        choices.push(Choice {
                            label,
                            detail: text(v, "type"),
                            target: Target::Reference(reference),
                        });
                    }
                }
            }
            Ok(choices)
        })
    }
}

/// Generate a SHA-1, six-digit, 30-second OTP at Unix `seconds` from an otpauth URI.
/// URL algorithm/digits/period options are ignored for legacy compatibility.
pub fn totp(uri: &str, seconds: u64) -> Result<String> {
    let url = url::Url::parse(uri).context("Invalid otpauth URL")?;
    if url.scheme() != "otpauth" {
        bail!("Invalid otpauth URL");
    }
    let seed = Zeroizing::new(
        url.query_pairs()
            .find(|(k, _)| k == "secret")
            .map(|(_, v)| v.into_owned())
            .context("Missing secret in otpauth URL")?,
    );
    if seed.is_empty() {
        bail!("Missing secret in otpauth URL");
    }
    let normalized = Zeroizing::new(seed.trim_end_matches('=').to_ascii_uppercase());
    let bytes = Zeroizing::new(
        data_encoding::BASE32_NOPAD
            .decode(normalized.as_bytes())
            .map_err(|_| anyhow::anyhow!("Invalid base32 OTP seed"))?,
    );
    let mut mac =
        Hmac::<Sha1>::new_from_slice(&bytes).map_err(|_| anyhow::anyhow!("Invalid OTP seed"))?;
    mac.update(&(seconds / 30).to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[19] & 15) as usize;
    let value = u32::from_be_bytes(digest[offset..offset + 4].try_into().unwrap()) & 0x7fff_ffff;
    Ok(format!("{:06}", value % 1_000_000))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn otp_vectors() {
        let uri = "otpauth://totp/test?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=8";
        assert_eq!(totp(uri, 59).unwrap(), "287082");
        assert_eq!(totp(uri, 1111111109).unwrap(), "081804");
        assert!(totp("https://example.com", 59).is_err());
        assert!(totp("otpauth://totp/test", 59).is_err());
    }
}
