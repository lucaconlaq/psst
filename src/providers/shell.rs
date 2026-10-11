use super::{FetchFuture, Provider};
use crate::process;
use std::path::Path;

pub(super) struct Shell;
impl Provider for Shell {
    fn id(&self) -> &'static str {
        "shell"
    }
    fn reference_label(&self) -> &'static str {
        "Shell command (trusted code)"
    }
    fn fetch<'a>(&'a self, value: &'a str, dir: &'a Path) -> FetchFuture<'a> {
        Box::pin(async move { process::output("/bin/sh", &["-c", value], dir).await })
    }
}
