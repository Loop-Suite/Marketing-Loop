use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A review lens (one of 7 marketing personas, selected to fit the content type's nature).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Lens {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub guide: String,
    /// If true, always force-included at the lens-selection stage.
    #[serde(default)]
    pub always: bool,
    /// The signal that causes this lens to be picked (inserted as-is into the selection prompt).
    #[serde(default)]
    pub signal: String,
    /// Characterized persona name (empty = no persona). Purpose: suppress sycophancy.
    #[serde(default)]
    pub persona_name: String,
    /// One-line summary of the persona's perspective/principles.
    #[serde(default)]
    pub persona_voice: String,
    /// Display-only string (e.g. 1/2 or core/support). Not involved in selection logic.
    #[serde(default)]
    pub tier: String,
}

/// A deterministic (locally computed) checklist item. The LLM doesn't judge it — the checks.rs result is shown as-is.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeterministicCheck {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub tool: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Spec {
    pub name: String,
    /// Campaign/brand context. Inserted as-is into the prompt.
    #[serde(default)]
    pub context: String,
    pub lenses: Vec<Lens>,
    #[serde(default)]
    pub deterministic_checks: Vec<DeterministicCheck>,
    /// List of labels allowed on findings.
    pub labels: Vec<String>,
    /// Content length (character count) upper limit. 0 = not configured (N/A).
    #[serde(default)]
    pub content_length_limit: usize,
    /// List of content types where a disclaimer (ad label, opt-out link, etc.) is required.
    #[serde(default)]
    pub disclaimer_required_types: Vec<String>,
    /// Brand/product name keywords that must be present in the content.
    #[serde(default)]
    pub required_brand_terms: Vec<String>,
}

impl Spec {
    pub fn load(path: &Path) -> Result<Spec> {
        let s = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read spec file: {}", path.display()))?;
        let spec: Spec = toml::from_str(&s)
            .with_context(|| format!("Failed to parse spec TOML: {}", path.display()))?;
        anyhow::ensure!(!spec.lenses.is_empty(), "lenses is empty");
        anyhow::ensure!(!spec.labels.is_empty(), "labels is empty");
        Ok(spec)
    }

    pub fn lens_by_id(&self, id: &str) -> Option<&Lens> {
        self.lenses.iter().find(|l| l.id == id)
    }

    pub fn always_lenses(&self) -> Vec<&Lens> {
        self.lenses.iter().filter(|l| l.always).collect()
    }

    pub fn optional_lenses(&self) -> Vec<&Lens> {
        self.lenses.iter().filter(|l| !l.always).collect()
    }

    pub fn labels_prompt(&self) -> String {
        self.labels
            .iter()
            .map(|l| format!("\"{l}\""))
            .collect::<Vec<_>>()
            .join(", ")
    }
}
