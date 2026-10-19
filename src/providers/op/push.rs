//! Deliberately narrow JSON round trips: Secure Notes only, not Login items.
//! Login JSON can omit passkeys; absence of a passkey property is not evidence.
use std::{collections::HashSet, io::Write, path::Path};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroizing;

use super::READ_GATE;
use crate::process::{self, TransferState};

// Conservative psst limits, not claims about the provider's maximum capacity.
pub(crate) const VALUE_LIMIT: usize = 65_536;
const ITEM_LIMIT: usize = 1_048_576;
const LIST_LIMIT: usize = 4096;

// Every parsed string has a zeroizing owner, including partial parse failures.
// Serde's internal scratch space and compiler/OS copies remain best effort.
#[derive(Default)]
struct Text(Zeroizing<String>);
impl Serialize for Text {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Text {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(|text| Self(Zeroizing::new(text)))
    }
}
impl Text {
    fn as_str(&self) -> &str {
        &self.0
    }
}

// Distinguish absent properties from explicit null: do not silently drop or
// normalize metadata whose round-trip meaning has not been established.
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Deserialize)]
struct Listed {
    id: Text,
    #[serde(default)]
    name: Text,
    #[serde(default)]
    title: Text,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Vault {
    id: Text,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    name: Option<Text>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Section {
    id: Text,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    label: Option<Text>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Field {
    id: Text,
    r#type: Text,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    label: Option<Text>,
    // op omits value for an existing empty field. Eligibility still requires
    // a known field identity and STRING/CONCEALED type; explicit null is invalid.
    #[serde(default)]
    value: Text,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    purpose: Option<Text>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    reference: Option<Text>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    section: Option<Section>,
}

/// The allowlist is intentionally closed. In particular files, passkeys,
/// concealed unsupported field types and future properties cannot pass through.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Item {
    id: Text,
    title: Text,
    version: u64,
    vault: Vault,
    category: Text,
    // CLI-generated display summary. Preserve the latest string in the payload;
    // op regenerates it when the underlying note changes (verified on 2.30.3).
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    additional_information: Option<Text>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    last_edited_by: Option<Text>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    created_at: Option<Text>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    updated_at: Option<Text>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    favorite: Option<bool>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    tags: Option<Vec<Text>>,
    fields: Vec<Field>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    sections: Option<Vec<Section>>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Target {
    vault: String,
    item: String,
    field: String,
    section: Option<String>,
}

fn id(value: &str) -> bool {
    value.len() == 26
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

async fn read(args: &[&str], dir: &Path) -> Result<Zeroizing<String>> {
    let _guard = READ_GATE.lock().await;
    process::exact("op", args, dir, None, &mut TransferState::NotStarted).await
}

async fn select(args: &[&str], selector: &str, dir: &Path) -> Result<String> {
    let raw = read(args, dir).await?;
    let rows: Vec<Listed> = serde_json::from_str(&raw)?;
    ensure!(rows.len() <= LIST_LIMIT, "Provider list limit exceeded");
    let mut ids = HashSet::new();
    let mut matches = Vec::new();
    for row in &rows {
        ensure!(
            id(row.id.as_str()) && ids.insert(row.id.as_str()),
            "Invalid provider identity"
        );
        if row.id.as_str() == selector
            || row.name.as_str() == selector
            || row.title.as_str() == selector
        {
            matches.push(row.id.as_str());
        }
    }
    ensure!(matches.len() == 1, "Missing or ambiguous provider identity");
    Ok(matches[0].to_owned())
}

impl Item {
    async fn load(vault: &str, item: &str, dir: &Path) -> Result<Self> {
        let raw = read(
            &["item", "get", item, "--vault", vault, "--format", "json"],
            dir,
        )
        .await?;
        ensure!(raw.len() <= ITEM_LIMIT, "Provider item exceeds 1 MiB");
        let parsed: Self = serde_json::from_str(&raw)?;
        ensure!(
            parsed.vault.id.as_str() == vault && parsed.id.as_str() == item,
            "Provider identity changed"
        );
        parsed.eligible()?;
        Ok(parsed)
    }

    fn eligible(&self) -> Result<()> {
        ensure!(
            self.category.as_str() == "SECURE_NOTE",
            "Only Secure Notes are eligible"
        );
        ensure!(self.fields.len() <= 128, "Too many fields");
        let mut ids = HashSet::new();
        let sections = self.sections.as_deref().unwrap_or_default();
        ensure!(sections.len() <= 128, "Too many sections");
        let mut section_ids = HashSet::new();
        for section in sections {
            ensure!(
                !section.id.as_str().is_empty() && section_ids.insert(section.id.as_str()),
                "Ambiguous section"
            );
        }
        for field in &self.fields {
            ensure!(
                !field.id.as_str().is_empty() && ids.insert(field.id.as_str()),
                "Ambiguous field"
            );
            ensure!(
                matches!(field.r#type.as_str(), "STRING" | "CONCEALED"),
                "Unsupported field type"
            );
            ensure!(
                field
                    .purpose
                    .as_ref()
                    .is_none_or(|purpose| purpose.as_str() == "NOTES"
                        && field.id.as_str() == "notesPlain"
                        && field.r#type.as_str() == "STRING"),
                "Unsupported field purpose"
            );
            ensure!(!field.value.as_str().contains('\0'), "Invalid field value");
            if let Some(section) = &field.section {
                ensure!(section_ids.contains(section.id.as_str()), "Missing section");
            }
        }
        Ok(())
    }

    fn field(&self, target: &Target) -> Result<&Field> {
        self.fields
            .iter()
            .find(|field| {
                field.id.as_str() == target.field
                    && field.section.as_ref().map(|section| section.id.as_str())
                        == target.section.as_deref()
            })
            .context("Field removed or retargeted")
    }

    pub(crate) fn set(&mut self, target: &Target, text: &[u8]) -> Result<()> {
        ensure!(text.len() <= VALUE_LIMIT, "Provider field exceeds 64 KiB");
        self.field(target)?;
        let field = self
            .fields
            .iter_mut()
            .find(|field| field.id.as_str() == target.field)
            .unwrap();
        let value = std::str::from_utf8(text)?;
        ensure!(!value.contains('\0'), "Invalid field value");
        field.value = Text(Zeroizing::new(value.to_owned()));
        Ok(())
    }

    pub(crate) fn payload(&self) -> Result<Zeroizing<Vec<u8>>> {
        struct Bounded(Zeroizing<Vec<u8>>);
        impl Write for Bounded {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.0.len().saturating_add(bytes.len()) > ITEM_LIMIT {
                    return Err(std::io::Error::other("Provider payload exceeds 1 MiB"));
                }
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut buffer = Bounded(Zeroizing::new(Vec::with_capacity(ITEM_LIMIT)));
        serde_json::to_writer(&mut buffer, self)?;
        Ok(buffer.0)
    }
}

impl Target {
    pub(crate) async fn resolve(reference: &str, dir: &Path) -> Result<Self> {
        let reference = reference
            .strip_prefix("op://")
            .context("Invalid reference")?;
        // This subset deliberately refuses encoding and query/attribute syntax
        // rather than guessing how op normalizes potentially aliased selectors.
        ensure!(
            !reference
                .chars()
                .any(|c| c.is_control() || "%?#\\".contains(c)),
            "Unsupported reference syntax"
        );
        let parts: Vec<_> = reference.split('/').collect();
        ensure!(
            (parts.len() == 3 || parts.len() == 4) && parts.iter().all(|part| !part.is_empty()),
            "Invalid reference"
        );
        let vault = select(&["vault", "list", "--format", "json"], parts[0], dir).await?;
        let item = select(
            &["item", "list", "--vault", &vault, "--format", "json"],
            parts[1],
            dir,
        )
        .await?;
        let raw = Item::load(&vault, &item, dir).await?;
        let mut sections = HashSet::new();
        if parts.len() == 4 {
            for section in raw.sections.as_deref().unwrap_or_default() {
                if section.id.as_str() == parts[2]
                    || section
                        .label
                        .as_ref()
                        .is_some_and(|label| label.as_str() == parts[2])
                {
                    sections.insert(section.id.as_str());
                }
            }
            ensure!(sections.len() == 1, "Missing or ambiguous section");
        }
        let fields: Vec<_> = raw
            .fields
            .iter()
            .filter(|field| {
                (field.id.as_str() == parts[parts.len() - 1]
                    || field
                        .label
                        .as_ref()
                        .is_some_and(|label| label.as_str() == parts[parts.len() - 1]))
                    && (parts.len() == 3
                        || field
                            .section
                            .as_ref()
                            .is_some_and(|section| sections.contains(section.id.as_str())))
            })
            .collect();
        ensure!(fields.len() == 1, "Missing or ambiguous field");
        let field = fields[0];
        Ok(Self {
            vault,
            item,
            field: field.id.as_str().to_owned(),
            section: field
                .section
                .as_ref()
                .map(|section| section.id.as_str().to_owned()),
        })
    }

    pub(crate) fn same_item(&self, other: &Self) -> bool {
        self.vault == other.vault && self.item == other.item
    }

    pub(crate) async fn load(&self, dir: &Path) -> Result<Item> {
        let item = Item::load(&self.vault, &self.item, dir).await?;
        item.field(self)?;
        Ok(item)
    }

    pub(crate) async fn unchanged(&self, dir: &Path, input: &[u8]) -> Result<bool> {
        let item = self.load(dir).await?;
        Ok(item.field(self)?.value.as_str().as_bytes() == input)
    }

    pub(crate) async fn edit(
        &self,
        dir: &Path,
        payload: Zeroizing<Vec<u8>>,
        state: &mut TransferState,
    ) -> Result<()> {
        let _guard = READ_GATE.lock().await;
        process::exact(
            "op",
            &[
                "item",
                "edit",
                &self.item,
                "--vault",
                &self.vault,
                "--format",
                "json",
            ],
            dir,
            Some(payload),
            state,
        )
        .await?;
        Ok(())
    }
}
