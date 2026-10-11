use super::{FetchFuture, Provider, reference, scalar};
use crate::process;
use anyhow::Context;
use std::path::Path;
use zeroize::Zeroizing;

pub(super) struct Terraform;
impl Provider for Terraform {
    fn id(&self) -> &'static str {
        "terraform"
    }
    fn reference_label(&self) -> &'static str {
        "Directory#output"
    }
    fn fetch<'a>(&'a self, value: &'a str, dir: &'a Path) -> FetchFuture<'a> {
        Box::pin(async move {
            let (directory, key) = reference(value)?;
            // Node path.join retains the config directory for absolute-looking segments.
            let cwd = dir.join(directory.trim_start_matches('/'));
            let data = process::output("terraform", &["output", "-json"], &cwd).await?;
            let json: serde_json::Value =
                serde_json::from_str(&data).context("Terraform returned invalid JSON")?;
            let value = json
                .get(key)
                .and_then(|d| d.get("value"))
                .context("Terraform output not found")?;
            Ok(Zeroizing::new(scalar(value)))
        })
    }
}
