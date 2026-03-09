//! Schema profile selection for NIEM vs generic XSD transforms.

use crate::xsd::model::QName;

/// Controls which NIEM-specific transformations are applied.
///
/// - `Niem`: applies all NIEM-specific collapsing, inlining, and skipping rules.
/// - `Generic`: treats every XSD construct literally — no NIEM shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum SchemaProfile {
    /// NIEM profile: collapses wrappers, inlines structures attributes, etc.
    Niem,
    /// Generic XSD profile: no NIEM-specific transforms.
    Generic,
}

impl SchemaProfile {
    /// Skip emitting messages for the structures namespace entirely.
    pub fn should_skip_structures_namespace(self) -> bool {
        self == SchemaProfile::Niem
    }

    /// Collapse NIEM wrapper types (simpleContent/extension with only
    /// `SimpleObjectAttributeGroup`) to their underlying primitive/enum.
    pub fn should_collapse_niem_wrappers(self) -> bool {
        self == SchemaProfile::Niem
    }

    /// Inline `structures:id/ref/uri/metadata` fields when extending a
    /// structures base type, instead of emitting a composition field.
    pub fn should_inline_structures_base(self) -> bool {
        self == SchemaProfile::Niem
    }

    /// Skip `SimpleObjectAttributeGroup` and individual structures-namespace
    /// attribute references (they are inlined by `should_inline_structures_base`).
    pub fn should_skip_structures_attributes(self) -> bool {
        self == SchemaProfile::Niem
    }

    /// Map `niem-xs:*` proxy types directly to proto builtins.
    pub fn should_use_niem_proxy_mapping(self) -> bool {
        self == SchemaProfile::Niem
    }

    /// Treat elements whose local name ends with `AugmentationPoint` as
    /// special augmentation points (NIEM oneof handling).
    pub fn is_augmentation_point(self, qname: &QName) -> bool {
        self == SchemaProfile::Niem && qname.local_name.ends_with("AugmentationPoint")
    }
}
