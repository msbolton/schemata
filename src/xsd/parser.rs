//! Parser that reads XSD files via `roxmltree` and produces [`XsdSchema`] values.
//!
//! The parser matches elements by the XSD namespace URI
//! (`http://www.w3.org/2001/XMLSchema`), so it works regardless of whether
//! the schema uses the `xs:` or `xsd:` prefix (or any other alias).

use std::path::Path;

use anyhow::{bail, Context, Result};
use roxmltree::{Document, Node};
use tracing::warn;

use crate::xsd::model::*;
use crate::xsd::names::{NamespaceMap, XS_NAMESPACE};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Parse an XSD schema from its XML text.
///
/// `source_path` is used only for error messages.
pub fn parse_schema(xml: &str, source_path: &Path) -> Result<XsdSchema> {
    let doc = Document::parse(xml)
        .with_context(|| format!("failed to parse XML in {}", source_path.display()))?;

    let root = doc.root_element();
    ensure_xsd_element(&root, "schema", source_path)?;

    let ns_map = build_namespace_map(&root);
    let target_namespace = root
        .attribute("targetNamespace")
        .map(|s| NamespaceUri(s.to_string()));

    let mut schema = XsdSchema {
        target_namespace,
        imports: Vec::new(),
        complex_types: Vec::new(),
        simple_types: Vec::new(),
        elements: Vec::new(),
        attributes: Vec::new(),
        attribute_groups: Vec::new(),
    };

    for child in xsd_children(&root) {
        let local = child.tag_name().name();
        match local {
            "import" => schema.imports.push(parse_import(&child)),
            "complexType" => {
                schema.complex_types.push(parse_complex_type(
                    &child,
                    &ns_map,
                    &schema.target_namespace,
                )?);
            }
            "simpleType" => {
                schema.simple_types.push(parse_simple_type(
                    &child,
                    &ns_map,
                    &schema.target_namespace,
                )?);
            }
            "element" => {
                schema
                    .elements
                    .push(parse_element(&child, &ns_map, &schema.target_namespace)?);
            }
            "attribute" => {
                schema
                    .attributes
                    .push(parse_attribute(&child, &ns_map, &schema.target_namespace));
            }
            "attributeGroup" => {
                schema.attribute_groups.push(parse_attribute_group(
                    &child,
                    &ns_map,
                    &schema.target_namespace,
                )?);
            }
            // xs:annotation at schema level, xs:include, xs:redefine, etc. — skip
            _ => {}
        }
    }

    Ok(schema)
}

// ---------------------------------------------------------------------------
// Helpers — namespace-aware child iteration
// ---------------------------------------------------------------------------

/// Iterate over child elements that are in the XSD namespace.
fn xsd_children<'a>(node: &'a Node<'a, 'a>) -> impl Iterator<Item = Node<'a, 'a>> {
    node.children()
        .filter(|n| n.is_element() && is_xsd_element(n))
}

/// Check whether a node is in the XSD namespace.
fn is_xsd_element(node: &Node) -> bool {
    node.tag_name().namespace() == Some(XS_NAMESPACE)
}

/// Assert that `node` is an XSD element with the expected local name.
fn ensure_xsd_element(node: &Node, expected: &str, source_path: &Path) -> Result<()> {
    if !is_xsd_element(node) || node.tag_name().name() != expected {
        anyhow::bail!(
            "{}: expected xs:{} element, found <{}>",
            source_path.display(),
            expected,
            node.tag_name().name()
        );
    }
    Ok(())
}

/// Find the first XSD child element with the given local name.
fn xsd_child<'a>(node: &'a Node<'a, 'a>, local_name: &str) -> Option<Node<'a, 'a>> {
    xsd_children(node).find(|c| c.tag_name().name() == local_name)
}

// ---------------------------------------------------------------------------
// Namespace map construction
// ---------------------------------------------------------------------------

/// Build a [`NamespaceMap`] from the namespace declarations on the root
/// `<xs:schema>` element (or any element, really).
fn build_namespace_map(schema_node: &Node) -> NamespaceMap {
    let mut map = NamespaceMap::new();
    for ns in schema_node.namespaces() {
        if let Some(prefix) = ns.name() {
            map.insert(prefix, ns.uri());
        }
    }
    map
}

// ---------------------------------------------------------------------------
// QName resolution
// ---------------------------------------------------------------------------

/// Resolve a prefixed attribute value (like `"structures:ObjectType"`) to a
/// [`QName`], using the namespace declarations from the document.
///
/// If the name has no prefix, `default_ns` is used (typically the target
/// namespace of the schema).
fn resolve_qname(
    prefixed: &str,
    ns_map: &NamespaceMap,
    default_ns: &Option<NamespaceUri>,
) -> Option<QName> {
    ns_map.resolve(prefixed, default_ns.as_ref())
}

/// Convenience: resolve a QName from an attribute value, returning `None` if
/// the attribute is absent.
fn resolve_attr_qname(
    node: &Node,
    attr_name: &str,
    ns_map: &NamespaceMap,
    default_ns: &Option<NamespaceUri>,
) -> Option<QName> {
    node.attribute(attr_name)
        .and_then(|v| resolve_qname(v, ns_map, default_ns))
}

// ---------------------------------------------------------------------------
// Annotation / documentation
// ---------------------------------------------------------------------------

/// Extract `xs:annotation/xs:documentation` text from a node's children.
fn parse_annotation(node: &Node) -> XsdAnnotation {
    if let Some(ann) = xsd_child(node, "annotation") {
        if let Some(doc) = xsd_child(&ann, "documentation") {
            let text = doc.text().unwrap_or("").trim();
            if text.is_empty() {
                return XsdAnnotation {
                    documentation: None,
                };
            }
            return XsdAnnotation {
                documentation: Some(text.to_string()),
            };
        }
    }
    XsdAnnotation {
        documentation: None,
    }
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

fn parse_import(node: &Node) -> XsdImport {
    XsdImport {
        namespace: node
            .attribute("namespace")
            .map(|s| NamespaceUri(s.to_string())),
        schema_location: node.attribute("schemaLocation").map(|s| s.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Complex type
// ---------------------------------------------------------------------------

fn parse_complex_type(
    node: &Node,
    ns_map: &NamespaceMap,
    default_ns: &Option<NamespaceUri>,
) -> Result<XsdComplexType> {
    let name = node.attribute("name").map(|s| s.to_string());
    let is_abstract = node.attribute("abstract") == Some("true");
    let annotation = parse_annotation(node);

    // Collect attributes, attributeGroup refs, and anyAttribute that appear
    // as direct children of the complexType — these apply when there is no
    // complexContent/simpleContent wrapper, or when there is direct content.
    let (mut attrs, mut ag_refs, mut any_attr) = collect_attributes(node, ns_map, default_ns);

    // Determine the content model.
    let content = if let Some(cc) = xsd_child(node, "complexContent") {
        parse_complex_content(
            &cc,
            ns_map,
            default_ns,
            &mut attrs,
            &mut ag_refs,
            &mut any_attr,
        )?
    } else if let Some(sc) = xsd_child(node, "simpleContent") {
        parse_simple_content(
            &sc,
            ns_map,
            default_ns,
            &mut attrs,
            &mut ag_refs,
            &mut any_attr,
        )?
    } else if let Some(compositor) = find_compositor(node, ns_map, default_ns)? {
        // Direct content: sequence/choice/all at top level.
        ComplexTypeContent::Direct {
            compositor: Some(compositor),
        }
    } else {
        // No compositor — empty content (possibly with attributes only).
        ComplexTypeContent::Empty
    };

    Ok(XsdComplexType {
        name,
        is_abstract,
        content,
        attributes: attrs,
        attribute_group_refs: ag_refs,
        any_attribute: any_attr,
        annotation,
    })
}

/// Parse `xs:complexContent` — either extension or restriction.
fn parse_complex_content(
    node: &Node,
    ns_map: &NamespaceMap,
    default_ns: &Option<NamespaceUri>,
    attrs: &mut Vec<XsdAttribute>,
    ag_refs: &mut Vec<QName>,
    any_attr: &mut Option<AnyAttribute>,
) -> Result<ComplexTypeContent> {
    if let Some(ext) = xsd_child(node, "extension") {
        let base = resolve_attr_qname(&ext, "base", ns_map, default_ns)
            .context("complexContent/extension must have a resolvable 'base' attribute")?;
        let compositor = find_compositor(&ext, ns_map, default_ns)?;

        // Collect attributes from the extension element.
        let (ext_attrs, ext_ag_refs, ext_any_attr) = collect_attributes(&ext, ns_map, default_ns);
        attrs.extend(ext_attrs);
        ag_refs.extend(ext_ag_refs);
        if ext_any_attr.is_some() {
            *any_attr = ext_any_attr;
        }

        Ok(ComplexTypeContent::ComplexExtension { base, compositor })
    } else if let Some(res) = xsd_child(node, "restriction") {
        let base = resolve_attr_qname(&res, "base", ns_map, default_ns)
            .context("complexContent/restriction must have a resolvable 'base' attribute")?;
        let compositor = find_compositor(&res, ns_map, default_ns)?;

        let (res_attrs, res_ag_refs, res_any_attr) = collect_attributes(&res, ns_map, default_ns);
        attrs.extend(res_attrs);
        ag_refs.extend(res_ag_refs);
        if res_any_attr.is_some() {
            *any_attr = res_any_attr;
        }

        Ok(ComplexTypeContent::ComplexRestriction { base, compositor })
    } else {
        Ok(ComplexTypeContent::Empty)
    }
}

/// Parse `xs:simpleContent` — either extension or restriction.
fn parse_simple_content(
    node: &Node,
    ns_map: &NamespaceMap,
    default_ns: &Option<NamespaceUri>,
    attrs: &mut Vec<XsdAttribute>,
    ag_refs: &mut Vec<QName>,
    any_attr: &mut Option<AnyAttribute>,
) -> Result<ComplexTypeContent> {
    if let Some(ext) = xsd_child(node, "extension") {
        let base = resolve_attr_qname(&ext, "base", ns_map, default_ns)
            .context("simpleContent/extension must have a resolvable 'base' attribute")?;

        let (ext_attrs, ext_ag_refs, ext_any_attr) = collect_attributes(&ext, ns_map, default_ns);
        attrs.extend(ext_attrs);
        ag_refs.extend(ext_ag_refs);
        if ext_any_attr.is_some() {
            *any_attr = ext_any_attr;
        }

        Ok(ComplexTypeContent::SimpleExtension { base })
    } else if let Some(res) = xsd_child(node, "restriction") {
        let base = resolve_attr_qname(&res, "base", ns_map, default_ns)
            .context("simpleContent/restriction must have a resolvable 'base' attribute")?;

        let (res_attrs, res_ag_refs, res_any_attr) = collect_attributes(&res, ns_map, default_ns);
        attrs.extend(res_attrs);
        ag_refs.extend(res_ag_refs);
        if res_any_attr.is_some() {
            *any_attr = res_any_attr;
        }

        Ok(ComplexTypeContent::SimpleRestriction { base })
    } else {
        Ok(ComplexTypeContent::Empty)
    }
}

// ---------------------------------------------------------------------------
// Compositor (sequence / choice / all)
// ---------------------------------------------------------------------------

/// Find the first compositor (sequence/choice/all) child of `node`.
fn find_compositor(
    node: &Node,
    ns_map: &NamespaceMap,
    default_ns: &Option<NamespaceUri>,
) -> Result<Option<Compositor>> {
    for child in xsd_children(node) {
        match child.tag_name().name() {
            "sequence" | "choice" | "all" => {
                return Ok(Some(parse_compositor(&child, ns_map, default_ns)?));
            }
            _ => {}
        }
    }
    Ok(None)
}

/// Parse a compositor element (sequence, choice, or all) into our IR.
fn parse_compositor(
    node: &Node,
    ns_map: &NamespaceMap,
    default_ns: &Option<NamespaceUri>,
) -> Result<Compositor> {
    let kind = match node.tag_name().name() {
        "sequence" => CompositorKind::Sequence,
        "choice" => CompositorKind::Choice,
        "all" => CompositorKind::All,
        other => bail!("unexpected compositor kind: {other}"),
    };

    let min_occurs = parse_min_occurs(node);
    let max_occurs = parse_max_occurs(node);

    let mut items = Vec::new();
    for child in xsd_children(node) {
        match child.tag_name().name() {
            "element" => {
                items.push(CompositorItem::Element(parse_element(
                    &child, ns_map, default_ns,
                )?));
            }
            "sequence" | "choice" | "all" => {
                items.push(CompositorItem::Compositor(parse_compositor(
                    &child, ns_map, default_ns,
                )?));
            }
            // xs:any, xs:group, xs:annotation — skip for now
            _ => {}
        }
    }

    Ok(Compositor {
        kind,
        min_occurs,
        max_occurs,
        items,
    })
}

// ---------------------------------------------------------------------------
// Occurrence helpers
// ---------------------------------------------------------------------------

fn parse_min_occurs(node: &Node) -> Occurs {
    match node.attribute("minOccurs") {
        Some("unbounded") => Occurs::Unbounded,
        Some(s) => Occurs::Count(s.parse().unwrap_or(1)),
        None => Occurs::Count(1),
    }
}

fn parse_max_occurs(node: &Node) -> Occurs {
    match node.attribute("maxOccurs") {
        Some("unbounded") => Occurs::Unbounded,
        Some(s) => Occurs::Count(s.parse().unwrap_or(1)),
        None => Occurs::Count(1),
    }
}

// ---------------------------------------------------------------------------
// Element
// ---------------------------------------------------------------------------

fn parse_element(
    node: &Node,
    ns_map: &NamespaceMap,
    default_ns: &Option<NamespaceUri>,
) -> Result<XsdElement> {
    let element_ref = resolve_attr_qname(node, "ref", ns_map, default_ns);
    let name = node.attribute("name").map(|s| s.to_string());
    let type_ref = resolve_attr_qname(node, "type", ns_map, default_ns);
    let substitution_group = resolve_attr_qname(node, "substitutionGroup", ns_map, default_ns);
    let is_abstract = node.attribute("abstract") == Some("true");

    let min_occurs = parse_min_occurs(node);
    let max_occurs = parse_max_occurs(node);

    let annotation = parse_annotation(node);

    // Check for an inline anonymous complexType.
    let anonymous_type = match xsd_child(node, "complexType") {
        Some(ct_node) => Some(Box::new(parse_complex_type(&ct_node, ns_map, default_ns)?)),
        None => None,
    };

    Ok(XsdElement {
        name,
        element_ref,
        type_ref,
        anonymous_type,
        substitution_group,
        is_abstract,
        min_occurs,
        max_occurs,
        annotation,
    })
}

// ---------------------------------------------------------------------------
// Attribute
// ---------------------------------------------------------------------------

fn parse_attribute(
    node: &Node,
    ns_map: &NamespaceMap,
    default_ns: &Option<NamespaceUri>,
) -> XsdAttribute {
    let name = node.attribute("name").map(|s| s.to_string());
    let attribute_ref = resolve_attr_qname(node, "ref", ns_map, default_ns);
    let type_ref = resolve_attr_qname(node, "type", ns_map, default_ns);
    let use_required = node.attribute("use") == Some("required");
    let annotation = parse_annotation(node);

    XsdAttribute {
        name,
        attribute_ref,
        type_ref,
        use_required,
        annotation,
    }
}

// ---------------------------------------------------------------------------
// Attribute group
// ---------------------------------------------------------------------------

fn parse_attribute_group(
    node: &Node,
    ns_map: &NamespaceMap,
    default_ns: &Option<NamespaceUri>,
) -> Result<XsdAttributeGroup> {
    let name = node
        .attribute("name")
        .context("top-level attributeGroup must have a 'name' attribute")?
        .to_string();
    let annotation = parse_annotation(node);

    let (attrs, ag_refs, any_attr) = collect_attributes(node, ns_map, default_ns);

    Ok(XsdAttributeGroup {
        name,
        attributes: attrs,
        attribute_group_refs: ag_refs,
        any_attribute: any_attr,
        annotation,
    })
}

/// Collect `xs:attribute`, `xs:attributeGroup` refs, and `xs:anyAttribute`
/// from the direct children of `node`.
fn collect_attributes(
    node: &Node,
    ns_map: &NamespaceMap,
    default_ns: &Option<NamespaceUri>,
) -> (Vec<XsdAttribute>, Vec<QName>, Option<AnyAttribute>) {
    let mut attrs = Vec::new();
    let mut ag_refs = Vec::new();
    let mut any_attr = None;

    for child in xsd_children(node) {
        match child.tag_name().name() {
            "attribute" => {
                attrs.push(parse_attribute(&child, ns_map, default_ns));
            }
            "attributeGroup" => {
                // An attributeGroup child with a `ref` attribute is a reference.
                if let Some(qn) = resolve_attr_qname(&child, "ref", ns_map, default_ns) {
                    ag_refs.push(qn);
                }
            }
            "anyAttribute" => {
                any_attr = Some(parse_any_attribute(&child));
            }
            _ => {}
        }
    }

    (attrs, ag_refs, any_attr)
}

/// Parse an `xs:anyAttribute` element.
fn parse_any_attribute(node: &Node) -> AnyAttribute {
    let namespace = node.attribute("namespace").map(|s| s.to_string());
    let process_contents = match node.attribute("processContents") {
        Some("strict") => ProcessContents::Strict,
        Some("lax") => ProcessContents::Lax,
        Some("skip") => ProcessContents::Skip,
        _ => ProcessContents::default(),
    };
    AnyAttribute {
        namespace,
        process_contents,
    }
}

// ---------------------------------------------------------------------------
// Simple type
// ---------------------------------------------------------------------------

fn parse_simple_type(
    node: &Node,
    ns_map: &NamespaceMap,
    default_ns: &Option<NamespaceUri>,
) -> Result<XsdSimpleType> {
    let name = node
        .attribute("name")
        .context("top-level simpleType must have a 'name' attribute")?
        .to_string();
    let annotation = parse_annotation(node);

    let content = if let Some(restriction) = xsd_child(node, "restriction") {
        parse_simple_type_restriction(&restriction, ns_map, default_ns)?
    } else if let Some(list) = xsd_child(node, "list") {
        parse_simple_type_list(&list, ns_map, default_ns)?
    } else if let Some(union) = xsd_child(node, "union") {
        parse_simple_type_union(&union, ns_map, default_ns)
    } else {
        // Fallback — shouldn't happen in well-formed schemas.
        warn!(
            simple_type = %name,
            "simpleType has no restriction/list/union child; defaulting to xs:string restriction"
        );
        SimpleTypeContent::Restriction {
            base: QName {
                namespace: NamespaceUri(XS_NAMESPACE.to_string()),
                local_name: "string".to_string(),
            },
        }
    };

    Ok(XsdSimpleType {
        name,
        content,
        annotation,
    })
}

/// Parse `xs:restriction` inside a `xs:simpleType`.
///
/// Examines the facet children to decide which [`SimpleTypeContent`] variant
/// to produce (enumeration, pattern, range, length, or plain restriction).
fn parse_simple_type_restriction(
    node: &Node,
    ns_map: &NamespaceMap,
    default_ns: &Option<NamespaceUri>,
) -> Result<SimpleTypeContent> {
    let base = resolve_attr_qname(node, "base", ns_map, default_ns)
        .context("simpleType restriction must have a resolvable 'base' attribute")?;

    // Collect facets.
    let mut enumerations: Vec<EnumVariant> = Vec::new();
    let mut pattern: Option<String> = None;
    let mut min_inclusive: Option<String> = None;
    let mut max_inclusive: Option<String> = None;
    let mut min_exclusive: Option<String> = None;
    let mut max_exclusive: Option<String> = None;
    let mut min_length: Option<u64> = None;
    let mut max_length: Option<u64> = None;
    let mut length: Option<u64> = None;

    for child in xsd_children(node) {
        let value = child.attribute("value").unwrap_or("");
        match child.tag_name().name() {
            "enumeration" => {
                let annotation = parse_annotation(&child);
                enumerations.push(EnumVariant {
                    value: value.to_string(),
                    annotation,
                });
            }
            "pattern" => {
                pattern = Some(value.to_string());
            }
            "minInclusive" => {
                min_inclusive = Some(value.to_string());
            }
            "maxInclusive" => {
                max_inclusive = Some(value.to_string());
            }
            "minExclusive" => {
                min_exclusive = Some(value.to_string());
            }
            "maxExclusive" => {
                max_exclusive = Some(value.to_string());
            }
            "minLength" => {
                min_length = value.parse().ok();
            }
            "maxLength" => {
                max_length = value.parse().ok();
            }
            "length" => {
                length = value.parse().ok();
            }
            _ => {} // totalDigits, fractionDigits, whiteSpace, etc.
        }
    }

    // Decide which variant to produce. Priority: enumerations > pattern >
    // range > length > plain restriction.
    Ok(if !enumerations.is_empty() {
        SimpleTypeContent::Enumeration {
            base,
            variants: enumerations,
        }
    } else if let Some(pat) = pattern {
        SimpleTypeContent::Pattern { base, pattern: pat }
    } else if min_inclusive.is_some()
        || max_inclusive.is_some()
        || min_exclusive.is_some()
        || max_exclusive.is_some()
    {
        SimpleTypeContent::Range {
            base,
            min_inclusive,
            max_inclusive,
            min_exclusive,
            max_exclusive,
        }
    } else if min_length.is_some() || max_length.is_some() || length.is_some() {
        SimpleTypeContent::LengthRestriction {
            base,
            min_length,
            max_length,
            length,
        }
    } else {
        SimpleTypeContent::Restriction { base }
    })
}

/// Parse `xs:list` inside a `xs:simpleType`.
fn parse_simple_type_list(
    node: &Node,
    ns_map: &NamespaceMap,
    default_ns: &Option<NamespaceUri>,
) -> Result<SimpleTypeContent> {
    let item_type = resolve_attr_qname(node, "itemType", ns_map, default_ns)
        .context("xs:list must have a resolvable 'itemType' attribute")?;
    Ok(SimpleTypeContent::List { item_type })
}

/// Parse `xs:union` inside a `xs:simpleType`.
fn parse_simple_type_union(
    node: &Node,
    ns_map: &NamespaceMap,
    default_ns: &Option<NamespaceUri>,
) -> SimpleTypeContent {
    let member_types = node
        .attribute("memberTypes")
        .unwrap_or("")
        .split_whitespace()
        .filter_map(|mt| {
            let resolved = resolve_qname(mt, ns_map, default_ns);
            if resolved.is_none() {
                warn!(member_type = %mt, "could not resolve union memberType QName; skipping");
            }
            resolved
        })
        .collect();
    SimpleTypeContent::Union { member_types }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Helper: read a schema file and parse it.
    fn parse_file(relative_path: &str) -> XsdSchema {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let path = base.join(relative_path);
        let xml = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        parse_schema(&xml, &path)
            .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()))
    }

    // -----------------------------------------------------------------------
    // structures.xsd
    // -----------------------------------------------------------------------

    #[test]
    fn structures_object_type_is_abstract() {
        let schema = parse_file("schema/core-xsd/niem/xsd/utility/structures.xsd");
        let ot = schema
            .complex_types
            .iter()
            .find(|ct| ct.name.as_deref() == Some("ObjectType"))
            .expect("ObjectType not found");
        assert!(ot.is_abstract, "ObjectType should be abstract");
    }

    #[test]
    fn structures_object_type_has_direct_sequence() {
        let schema = parse_file("schema/core-xsd/niem/xsd/utility/structures.xsd");
        let ot = schema
            .complex_types
            .iter()
            .find(|ct| ct.name.as_deref() == Some("ObjectType"))
            .expect("ObjectType not found");
        match &ot.content {
            ComplexTypeContent::Direct { compositor } => {
                let c = compositor.as_ref().expect("should have a compositor");
                assert_eq!(c.kind, CompositorKind::Sequence);
                assert_eq!(c.items.len(), 1, "should have one element ref");
            }
            other => panic!("expected Direct content, got {other:?}"),
        }
    }

    #[test]
    fn structures_object_type_has_any_attribute() {
        let schema = parse_file("schema/core-xsd/niem/xsd/utility/structures.xsd");
        let ot = schema
            .complex_types
            .iter()
            .find(|ct| ct.name.as_deref() == Some("ObjectType"))
            .expect("ObjectType not found");
        let aa = ot
            .any_attribute
            .as_ref()
            .expect("ObjectType should have anyAttribute");
        assert_eq!(
            aa.namespace.as_deref(),
            Some("urn:us:gov:ic:ism urn:us:gov:ic:ntk")
        );
        assert_eq!(aa.process_contents, ProcessContents::Lax);
    }

    #[test]
    fn structures_simple_object_attribute_group() {
        let schema = parse_file("schema/core-xsd/niem/xsd/utility/structures.xsd");
        let ag = schema
            .attribute_groups
            .iter()
            .find(|g| g.name == "SimpleObjectAttributeGroup")
            .expect("SimpleObjectAttributeGroup not found");

        // Should have 6 attribute refs + anyAttribute.
        assert_eq!(ag.attributes.len(), 6);
        assert!(
            ag.any_attribute.is_some(),
            "SimpleObjectAttributeGroup should have anyAttribute"
        );
    }

    #[test]
    fn structures_has_six_top_level_attributes() {
        let schema = parse_file("schema/core-xsd/niem/xsd/utility/structures.xsd");
        assert_eq!(
            schema.attributes.len(),
            6,
            "structures.xsd should have 6 top-level attributes"
        );
    }

    #[test]
    fn structures_abstract_elements() {
        let schema = parse_file("schema/core-xsd/niem/xsd/utility/structures.xsd");
        let abstract_elements: Vec<_> = schema.elements.iter().filter(|e| e.is_abstract).collect();
        assert_eq!(
            abstract_elements.len(),
            2,
            "should have ObjectAugmentationPoint and AssociationAugmentationPoint"
        );
    }

    // -----------------------------------------------------------------------
    // niem-xs.xsd
    // -----------------------------------------------------------------------

    #[test]
    fn niem_xs_has_14_complex_types() {
        let schema = parse_file("schema/core-xsd/niem/xsd/adapters/niem-xs.xsd");
        assert_eq!(
            schema.complex_types.len(),
            14,
            "niem-xs.xsd should define 14 proxy complex types"
        );
    }

    #[test]
    fn niem_xs_all_simple_extension() {
        let schema = parse_file("schema/core-xsd/niem/xsd/adapters/niem-xs.xsd");
        for ct in &schema.complex_types {
            match &ct.content {
                ComplexTypeContent::SimpleExtension { .. } => {} // good
                other => panic!(
                    "expected SimpleExtension for {}, got {other:?}",
                    ct.name.as_deref().unwrap_or("<anon>")
                ),
            }
        }
    }

    #[test]
    fn niem_xs_imports_structures() {
        let schema = parse_file("schema/core-xsd/niem/xsd/adapters/niem-xs.xsd");
        assert_eq!(schema.imports.len(), 1);
        assert_eq!(
            schema.imports[0].namespace.as_ref().unwrap().as_str(),
            "http://release.niem.gov/niem/structures/5.0/"
        );
    }

    #[test]
    fn niem_xs_string_type_base_is_xs_string() {
        let schema = parse_file("schema/core-xsd/niem/xsd/adapters/niem-xs.xsd");
        let string_type = schema
            .complex_types
            .iter()
            .find(|ct| ct.name.as_deref() == Some("string"))
            .expect("string type not found");
        match &string_type.content {
            ComplexTypeContent::SimpleExtension { base } => {
                assert_eq!(base.namespace.as_str(), XS_NAMESPACE);
                assert_eq!(base.local_name, "string");
            }
            other => panic!("expected SimpleExtension, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // uc2-core-types.xsd
    // -----------------------------------------------------------------------

    #[test]
    fn uc2_core_types_confidence_code_simple_type_has_6_variants() {
        let schema = parse_file("schema/core-xsd/extension/uc2-core-types.xsd");
        let st = schema
            .simple_types
            .iter()
            .find(|s| s.name == "ConfidenceCodeSimpleType")
            .expect("ConfidenceCodeSimpleType not found");
        match &st.content {
            SimpleTypeContent::Enumeration { variants, .. } => {
                assert_eq!(variants.len(), 6, "should have 6 enum variants");
                let values: Vec<&str> = variants.iter().map(|v| v.value.as_str()).collect();
                assert!(values.contains(&"HIGH"));
                assert!(values.contains(&"VERY_LOW"));
            }
            other => panic!("expected Enumeration, got {other:?}"),
        }
    }

    #[test]
    fn uc2_core_types_wgs84_extends_object_type() {
        let schema = parse_file("schema/core-xsd/extension/uc2-core-types.xsd");
        let ct = schema
            .complex_types
            .iter()
            .find(|c| c.name.as_deref() == Some("WGS84LocationType"))
            .expect("WGS84LocationType not found");
        match &ct.content {
            ComplexTypeContent::ComplexExtension { base, compositor } => {
                assert_eq!(base.local_name, "ObjectType");
                let comp = compositor.as_ref().expect("should have a compositor");
                assert_eq!(comp.kind, CompositorKind::Sequence);
                assert_eq!(comp.items.len(), 4, "sequence should have 4 element refs");
            }
            other => panic!("expected ComplexExtension, got {other:?}"),
        }
    }

    #[test]
    fn uc2_core_types_uuid_pattern() {
        let schema = parse_file("schema/core-xsd/extension/uc2-core-types.xsd");
        let st = schema
            .simple_types
            .iter()
            .find(|s| s.name == "UuidIdentificationIDSimpleType")
            .expect("UuidIdentificationIDSimpleType not found");
        match &st.content {
            SimpleTypeContent::Pattern { pattern, base } => {
                assert!(pattern.contains("[0-9a-fA-F]"));
                assert_eq!(base.local_name, "token");
            }
            other => panic!("expected Pattern, got {other:?}"),
        }
    }

    #[test]
    fn uc2_core_types_string64_length() {
        let schema = parse_file("schema/core-xsd/extension/uc2-core-types.xsd");
        let st = schema
            .simple_types
            .iter()
            .find(|s| s.name == "String64SimpleType")
            .expect("String64SimpleType not found");
        match &st.content {
            SimpleTypeContent::LengthRestriction { max_length, .. } => {
                assert_eq!(*max_length, Some(64));
            }
            other => panic!("expected LengthRestriction, got {other:?}"),
        }
    }

    #[test]
    fn uc2_core_types_substitution_group() {
        let schema = parse_file("schema/core-xsd/extension/uc2-core-types.xsd");
        let el = schema
            .elements
            .iter()
            .find(|e| e.name.as_deref() == Some("GlobalIdentifierCategoryCode"))
            .expect("GlobalIdentifierCategoryCode not found");
        let sg = el
            .substitution_group
            .as_ref()
            .expect("should have substitutionGroup");
        assert_eq!(sg.local_name, "IdentificationCategoryAbstract");
    }

    #[test]
    fn uc2_core_types_imports() {
        let schema = parse_file("schema/core-xsd/extension/uc2-core-types.xsd");
        assert_eq!(schema.imports.len(), 3);
    }

    // -----------------------------------------------------------------------
    // uc2-core.xsd
    // -----------------------------------------------------------------------

    #[test]
    fn uc2_core_info_object_has_choice_with_14_elements() {
        let schema = parse_file("schema/core-xsd/extension/uc2-core.xsd");
        let ct = schema
            .complex_types
            .iter()
            .find(|c| c.name.as_deref() == Some("CoreInformationObjectType"))
            .expect("CoreInformationObjectType not found");
        match &ct.content {
            ComplexTypeContent::ComplexExtension { base, compositor } => {
                assert_eq!(base.local_name, "ObjectType");
                let seq = compositor.as_ref().expect("should have a compositor");
                assert_eq!(seq.kind, CompositorKind::Sequence);
                // The sequence contains a single choice compositor.
                assert_eq!(seq.items.len(), 1);
                match &seq.items[0] {
                    CompositorItem::Compositor(choice) => {
                        assert_eq!(choice.kind, CompositorKind::Choice);
                        assert_eq!(choice.items.len(), 14, "choice should have 14 element refs");
                    }
                    other => panic!("expected nested Compositor, got {other:?}"),
                }
            }
            other => panic!("expected ComplexExtension, got {other:?}"),
        }
    }

    #[test]
    fn uc2_core_substitution_group() {
        let schema = parse_file("schema/core-xsd/extension/uc2-core.xsd");
        let el = schema
            .elements
            .iter()
            .find(|e| e.name.as_deref() == Some("CoreInformationObject"))
            .expect("CoreInformationObject not found");
        let sg = el
            .substitution_group
            .as_ref()
            .expect("should have substitutionGroup");
        assert_eq!(sg.local_name, "InformationObjectAbstract");
    }

    #[test]
    fn uc2_core_imports() {
        let schema = parse_file("schema/core-xsd/extension/uc2-core.xsd");
        // 10 import directives
        assert_eq!(schema.imports.len(), 10);
    }
}
