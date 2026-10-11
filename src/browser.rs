use crate::process;
use anyhow::{Context, Result};
use futures::future::BoxFuture;
use std::{path::Path, sync::Arc};

/// A browser row containing display metadata and a navigation/selection target.
#[derive(Clone)]
pub struct Choice {
    /// Provider label, never a retrieved secret field value.
    pub label: String,
    /// Additional non-secret display metadata.
    pub detail: String,
    /// Action taken when selecting the row.
    pub target: Target,
}
/// Selecting a row either descends into a level or returns a stored reference.
#[derive(Clone)]
pub enum Target {
    /// Descend into another provider listing.
    Level(Level),
    /// Complete browsing with this reference, without fetching its value.
    Reference(String),
}
/// Provider-owned browser behavior, independent of terminal rendering.
pub trait BrowserLevel: Send + Sync {
    /// Human-readable location title.
    fn title(&self) -> String;
    /// List metadata and references, never retrieved secret field values.
    fn list<'a>(&'a self, dir: &'a Path) -> BoxFuture<'a, Result<Vec<Choice>>>;
}
/// Cloneable provider location; authentication is inherited from the environment.
#[derive(Clone)]
pub struct Level(Arc<dyn BrowserLevel>);
impl Level {
    /// Wrap a provider-specific location for the generic browser UI.
    pub fn new(level: impl BrowserLevel + 'static) -> Self {
        Self(Arc::new(level))
    }
    /// Human-readable location title for the browser.
    pub fn title(&self) -> String {
        self.0.title()
    }
}
/// List display metadata and references using provider-owned browser behavior.
/// Retrieved field values are not included in choices; subprocess limits apply.
pub async fn list(level: &Level, dir: &Path) -> Result<Vec<Choice>> {
    level.0.list(dir).await
}
pub(crate) async fn json(program: &str, args: &[&str], dir: &Path) -> Result<serde_json::Value> {
    let text = process::output(program, args, dir).await?;
    serde_json::from_str(&text).context("Provider browser returned invalid JSON")
}
pub(crate) fn text(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ExampleLevel;
    impl BrowserLevel for ExampleLevel {
        fn title(&self) -> String {
            "Example provider".into()
        }
        fn list<'a>(&'a self, _dir: &'a Path) -> BoxFuture<'a, Result<Vec<Choice>>> {
            Box::pin(async {
                Ok(vec![Choice {
                    label: "Field name".into(),
                    detail: "Reference only".into(),
                    target: Target::Reference("example://item/field".into()),
                }])
            })
        }
    }
    #[tokio::test]
    async fn browser_accepts_provider_owned_levels_without_central_dispatch() {
        let level = Level::new(ExampleLevel).clone();
        assert_eq!(level.title(), "Example provider");
        let choices = list(&level, Path::new(".")).await.unwrap();
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].label, "Field name");
        assert!(matches!(
            &choices[0].target,
            Target::Reference(reference) if reference == "example://item/field"
        ));
    }
}
