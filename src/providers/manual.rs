use super::{FetchFuture, Provider};
use anyhow::bail;
use std::path::Path;
use zeroize::Zeroizing;

pub(super) struct Manual;
impl Provider for Manual {
    fn id(&self) -> &'static str {
        "manual"
    }
    fn reference_label(&self) -> &'static str {
        "Literal value"
    }
    fn sensitive_reference(&self) -> bool {
        true
    }
    fn fetch<'a>(&'a self, value: &'a str, _dir: &'a Path) -> FetchFuture<'a> {
        Box::pin(async move {
            if value.is_empty() {
                bail!("Manual secret is empty");
            }
            Ok(Zeroizing::new(value.to_owned()))
        })
    }
}
