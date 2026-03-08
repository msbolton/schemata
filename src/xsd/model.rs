//! XSD model types representing the structure of XML Schema documents.
//!
//! These types are the internal representation produced by parsing XSD files.
//! They capture enough detail to drive the transform to Protocol Buffers,
//! without trying to model every corner of the XSD specification.

use std::fmt;

// ---------------------------------------------------------------------------
// Qualified names and namespaces
// ---------------------------------------------------------------------------

/// A namespace URI, e.g. `"http://release.niem.gov/niem/structures/5.0/"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NamespaceUri(pub String);

impl NamespaceUri {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NamespaceUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A namespace-qualified name.
///
/// In XSD, a QName like `structures:ObjectType` resolves to a namespace URI
/// plus a local name. We store the resolved form.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QName {
    pub namespace: NamespaceUri,
    pub local_name: String,
}

impl fmt::Display for QName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{}}}:{}", self.namespace, self.local_name)
    }
}

// ---------------------------------------------------------------------------
// Annotation
// ---------------------------------------------------------------------------

/// Documentation and appinfo attached to an XSD component.
#[derive(Debug, Clone, Default)]
pub struct XsdAnnotation {
    /// Human-readable documentation text (from `xs:documentation` elements).
    pub documentation: Option<String>,
}

// ---------------------------------------------------------------------------
// Schema (top-level container)
// ---------------------------------------------------------------------------

/// A parsed XSD schema document.
///
/// `target_namespace` is `None` only for chameleon/aggregation schemas (e.g.
/// `uc2-sos-all.xsd`). The pipeline skips schemas without a target namespace
/// during transform, so downstream code can safely require `Some`.
#[derive(Debug, Clone)]
pub struct XsdSchema {
    /// The target namespace declared by this schema (`None` for chameleon schemas).
    pub target_namespace: Option<NamespaceUri>,

    /// Imports of other namespaces.
    pub imports: Vec<XsdImport>,

    /// Top-level `xs:complexType` definitions.
    pub complex_types: Vec<XsdComplexType>,

    /// Top-level `xs:simpleType` definitions.
    pub simple_types: Vec<XsdSimpleType>,

    /// Top-level `xs:element` declarations.
    pub elements: Vec<XsdElement>,

    /// Top-level `xs:attribute` declarations.
    pub attributes: Vec<XsdAttribute>,

    /// Top-level `xs:attributeGroup` definitions.
    pub attribute_groups: Vec<XsdAttributeGroup>,
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

/// An `xs:import` directive.
#[derive(Debug, Clone)]
pub struct XsdImport {
    pub namespace: Option<NamespaceUri>,
    pub schema_location: Option<String>,
}

// ---------------------------------------------------------------------------
// Complex type
// ---------------------------------------------------------------------------

/// A top-level or anonymous `xs:complexType`.
#[derive(Debug, Clone)]
pub struct XsdComplexType {
    /// Name of the type (None for anonymous types).
    pub name: Option<String>,

    /// Whether this type is declared `abstract="true"`.
    pub is_abstract: bool,

    /// The content model of this complex type.
    pub content: ComplexTypeContent,

    /// Attributes declared directly on this complex type.
    pub attributes: Vec<XsdAttribute>,

    /// Attribute group references on this complex type.
    pub attribute_group_refs: Vec<QName>,

    /// Whether this type has `xs:anyAttribute`.
    pub any_attribute: Option<AnyAttribute>,

    /// Documentation annotation.
    pub annotation: XsdAnnotation,
}

/// The content model of a complex type.
#[derive(Debug, Clone)]
pub enum ComplexTypeContent {
    /// `xs:complexContent/xs:extension base="..."` -- extends another complex type.
    ComplexExtension {
        base: QName,
        compositor: Option<Compositor>,
    },

    /// `xs:complexContent/xs:restriction base="..."` -- restricts another complex type.
    ComplexRestriction {
        base: QName,
        compositor: Option<Compositor>,
    },

    /// `xs:simpleContent/xs:extension base="..."` -- wraps a simple type with attributes.
    SimpleExtension { base: QName },

    /// `xs:simpleContent/xs:restriction base="..."` -- restricts a simple content type.
    SimpleRestriction { base: QName },

    /// Direct content (sequence/choice/all at the top level, no extension/restriction).
    Direct { compositor: Option<Compositor> },

    /// Empty complex type (no content model at all, just attributes).
    Empty,
}

// ---------------------------------------------------------------------------
// Compositor (sequence / choice / all)
// ---------------------------------------------------------------------------

/// A compositor groups child elements and/or nested compositors.
#[derive(Debug, Clone)]
pub struct Compositor {
    pub kind: CompositorKind,
    pub min_occurs: Occurs,
    pub max_occurs: Occurs,
    pub items: Vec<CompositorItem>,
}

/// The kind of compositor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositorKind {
    Sequence,
    Choice,
    All,
}

/// An item inside a compositor.
#[derive(Debug, Clone)]
pub enum CompositorItem {
    /// A child element declaration or reference.
    Element(XsdElement),
    /// A nested compositor (e.g., `xs:choice` inside `xs:sequence`).
    Compositor(Compositor),
}

// ---------------------------------------------------------------------------
// Occurrence constraints
// ---------------------------------------------------------------------------

/// An occurrence value: a concrete count or unbounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Occurs {
    /// A specific count (0, 1, 2, ...).
    Count(u32),
    /// `maxOccurs="unbounded"`.
    Unbounded,
}

impl Default for Occurs {
    fn default() -> Self {
        Occurs::Count(1)
    }
}

// ---------------------------------------------------------------------------
// Element
// ---------------------------------------------------------------------------

/// An `xs:element` declaration (top-level or within a compositor).
///
/// **Invariant:** An element is either a *reference* (`element_ref` is `Some`) or a
/// *declaration* (`name` is `Some`, with optional `type_ref` or `anonymous_type`).
/// These two forms are mutually exclusive — the parser enforces this.
#[derive(Debug, Clone)]
pub struct XsdElement {
    /// The local name of the element. Present for named elements; absent for
    /// anonymous element declarations (which shouldn't appear at top level).
    pub name: Option<String>,

    /// A reference to a global element (`ref="..."`). Mutually exclusive with `name`.
    pub element_ref: Option<QName>,

    /// The type of this element (`type="..."`).
    pub type_ref: Option<QName>,

    /// An anonymous complex type defined inline.
    pub anonymous_type: Option<Box<XsdComplexType>>,

    /// Substitution group head (`substitutionGroup="..."`).
    pub substitution_group: Option<QName>,

    /// Whether this element is declared `abstract="true"`.
    pub is_abstract: bool,

    pub min_occurs: Occurs,
    pub max_occurs: Occurs,

    /// Documentation annotation.
    pub annotation: XsdAnnotation,
}

// ---------------------------------------------------------------------------
// Attribute
// ---------------------------------------------------------------------------

/// An `xs:attribute` declaration.
#[derive(Debug, Clone)]
pub struct XsdAttribute {
    /// The local name (for a declaration).
    pub name: Option<String>,

    /// A reference to a global attribute (`ref="..."`).
    pub attribute_ref: Option<QName>,

    /// The type of this attribute.
    pub type_ref: Option<QName>,

    /// Whether `use="required"`.
    pub use_required: bool,

    /// Documentation annotation.
    pub annotation: XsdAnnotation,
}

// ---------------------------------------------------------------------------
// Attribute group
// ---------------------------------------------------------------------------

/// An `xs:attributeGroup` definition.
#[derive(Debug, Clone)]
pub struct XsdAttributeGroup {
    /// The name of this attribute group.
    pub name: String,

    /// Attributes declared in this group.
    pub attributes: Vec<XsdAttribute>,

    /// References to other attribute groups.
    pub attribute_group_refs: Vec<QName>,

    /// Whether this group includes `xs:anyAttribute`.
    pub any_attribute: Option<AnyAttribute>,

    /// Documentation annotation.
    pub annotation: XsdAnnotation,
}

/// Represents `xs:anyAttribute`.
#[derive(Debug, Clone)]
pub struct AnyAttribute {
    /// The namespace constraint (e.g., `"urn:us:gov:ic:ism urn:us:gov:ic:ntk"`).
    pub namespace: Option<String>,
    /// The `processContents` value.
    pub process_contents: ProcessContents,
}

/// The `processContents` attribute on `xs:any` / `xs:anyAttribute`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProcessContents {
    Strict,
    #[default]
    Lax,
    Skip,
}

// ---------------------------------------------------------------------------
// Simple type
// ---------------------------------------------------------------------------

/// A top-level `xs:simpleType` definition.
#[derive(Debug, Clone)]
pub struct XsdSimpleType {
    /// The name of the simple type.
    pub name: String,

    /// The content/restriction of this simple type.
    pub content: SimpleTypeContent,

    /// Documentation annotation.
    pub annotation: XsdAnnotation,
}

/// The restriction or derivation of a simple type.
#[derive(Debug, Clone)]
pub enum SimpleTypeContent {
    /// `xs:restriction` with `xs:enumeration` facets.
    Enumeration {
        base: QName,
        variants: Vec<EnumVariant>,
    },

    /// `xs:restriction` with a `xs:pattern` facet.
    Pattern { base: QName, pattern: String },

    /// `xs:restriction` with range facets (minInclusive, maxInclusive, etc.).
    Range {
        base: QName,
        min_inclusive: Option<String>,
        max_inclusive: Option<String>,
        min_exclusive: Option<String>,
        max_exclusive: Option<String>,
    },

    /// `xs:restriction` with length facets (minLength, maxLength, length).
    LengthRestriction {
        base: QName,
        min_length: Option<u64>,
        max_length: Option<u64>,
        length: Option<u64>,
    },

    /// `xs:list` with an item type.
    List { item_type: QName },

    /// `xs:union` of member types.
    Union { member_types: Vec<QName> },

    /// A plain restriction with no recognized facets (just a base type).
    Restriction { base: QName },
}

/// A single variant in an enumeration simple type.
#[derive(Debug, Clone)]
pub struct EnumVariant {
    /// The enumeration value string.
    pub value: String,

    /// Documentation for this variant.
    pub annotation: XsdAnnotation,
}
