use super::{FetchFuture, Provider, reference, scalar};
use crate::{
    browser::{BrowserLevel, Choice, Level, Target, json, text},
    process,
};
use anyhow::{Context, Result};
use futures::future::BoxFuture;
use std::path::Path;
use zeroize::Zeroizing;

pub(super) struct Vault;
impl Provider for Vault {
    fn id(&self) -> &'static str {
        "vault"
    }
    fn reference_label(&self) -> &'static str {
        "Vault (KV v2) · path#field · Ctrl+B browse"
    }
    fn browser(&self) -> Option<Level> {
        Some(Level::new(Location::Mounts))
    }
    fn fetch<'a>(&'a self, value: &'a str, dir: &'a Path) -> FetchFuture<'a> {
        Box::pin(async move {
            let (path, key) = reference(value)?;
            let data = process::output("vault", &["kv", "get", "-format=json", path], dir).await?;
            let json: serde_json::Value =
                serde_json::from_str(&data).context("Vault returned invalid JSON")?;
            let value = json
                .get("data")
                .and_then(|d| d.get("data"))
                .and_then(|d| d.get(key))
                .context("Vault KV v2 field not found")?;
            Ok(Zeroizing::new(scalar(value)))
        })
    }
}

enum Location {
    Mounts,
    Path(String),
}
impl BrowserLevel for Location {
    fn title(&self) -> String {
        match self {
            Self::Mounts => "Vault / KV mounts".into(),
            Self::Path(path) => format!("Vault / {path}"),
        }
    }
    fn list<'a>(&'a self, dir: &'a Path) -> BoxFuture<'a, Result<Vec<Choice>>> {
        Box::pin(async move {
            let mut choices = Vec::new();
            match self {
                Self::Mounts => {
                    let data = json(
                        "vault",
                        &["read", "-format=json", "sys/internal/ui/mounts"],
                        dir,
                    )
                    .await?;
                    for (path, v) in data
                        .pointer("/data/secret")
                        .and_then(|v| v.as_object())
                        .context("Expected Vault mounts")?
                    {
                        if text(v, "type") == "kv" {
                            choices.push(Choice {
                                label: path.clone(),
                                detail: "KV mount".into(),
                                target: Target::Level(Level::new(Self::Path(
                                    path.trim_matches('/').into(),
                                ))),
                            });
                        }
                    }
                }
                Self::Path(path) => {
                    let list_args = ["kv", "list", "-format=json", path];
                    let get_args = ["kv", "get", "-format=json", path];
                    let (folders, fields) = tokio::join!(
                        json("vault", &list_args, dir),
                        json("vault", &get_args, dir)
                    );
                    if folders.is_err() && fields.is_err() {
                        anyhow::bail!(
                            "Cannot list or read this Vault path; check authentication and permissions"
                        );
                    }
                    if let Ok(data) = folders
                        && let Some(array) = data.as_array()
                    {
                        for name in array.iter().filter_map(|v| v.as_str()) {
                            choices.push(Choice {
                                label: name.into(),
                                detail: "Folder".into(),
                                target: Target::Level(Level::new(Self::Path(format!(
                                    "{}/{}",
                                    path.trim_matches('/'),
                                    name.trim_matches('/')
                                )))),
                            });
                        }
                    }
                    if let Ok(data) = fields
                        && let Some(fields) = data.pointer("/data/data").and_then(|v| v.as_object())
                    {
                        for key in fields.keys() {
                            choices.push(Choice {
                                label: key.clone(),
                                detail: "Field".into(),
                                target: Target::Reference(format!(
                                    "/{}#{key}",
                                    path.trim_matches('/')
                                )),
                            });
                        }
                    }
                }
            }
            Ok(choices)
        })
    }
}
