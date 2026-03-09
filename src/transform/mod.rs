//! Transform layer that converts XSD IR to Proto IR.
//!
//! The main entry point is [`transform_schema`], which takes a namespace URI
//! and a [`TypeRegistry`] and produces a [`ProtoFile`] representing a single
//! `.proto` file for that namespace.

pub mod naming;
pub mod profile;

use self::profile::SchemaProfile;

use std::collections::BTreeSet;

use crate::proto::model::*;
use crate::resolver::TypeRegistry;
use crate::xsd::model::*;
use crate::xsd::names::{STRUCTURES_NAMESPACE, XS_NAMESPACE};

use self::naming::*;

/// The namespace URI for NIEM proxy types (niem-xs).
const NIEM_XS_NAMESPACE: &str = "http://release.niem.gov/niem/proxy/niem-xs/5.0/";

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Transform a single namespace from the XSD registry into a [`ProtoFile`].
///
/// Returns `None` if the namespace has no schema in the registry.
pub fn transform_schema(
    ns: &NamespaceUri,
    registry: &TypeRegistry,
    profile: SchemaProfile,
) -> Option<ProtoFile> {
    let schema = registry.schemas.get(ns)?;
    let package = namespace_to_package(ns.as_str());

    let mut ctx = TransformContext {
        registry,
        current_ns: ns.clone(),
        current_package: package.clone(),
        imports: BTreeSet::new(),
        profile,
    };

    let mut messages = Vec::new();
    let mut enums = Vec::new();

    // Transform simple types (enums, patterns, etc.).
    for st in &schema.simple_types {
        if let Some(proto_enum) = ctx.transform_simple_type(st) {
            enums.push(proto_enum);
        }
    }

    // Transform complex types.
    for ct in &schema.complex_types {
        if let Some(name) = &ct.name {
            // Skip NIEM wrapper types (Rule 3) - they get collapsed to the
            // underlying enum or primitive at usage sites.
            if ctx.profile.should_collapse_niem_wrappers() && ctx.is_niem_wrapper(ct) {
                continue;
            }

            // Skip structures namespace abstract base types - they are
            // handled specially when extending types reference them.
            if ctx.profile.should_skip_structures_namespace() && ns.as_str() == STRUCTURES_NAMESPACE
            {
                continue;
            }

            if let Some(msg) = ctx.transform_complex_type(ct, name) {
                messages.push(msg);
            }
        }
    }

    // Build import list.
    let imports: Vec<String> = ctx.imports.into_iter().collect();

    let source_xsd_path = registry.source_paths.get(ns).cloned();

    Some(ProtoFile {
        syntax: "proto3".to_string(),
        package,
        imports,
        options: Vec::new(),
        messages,
        enums,
        source_xsd_path,
    })
}

// ---------------------------------------------------------------------------
// Transform context
// ---------------------------------------------------------------------------

/// Mutable context threaded through the transform.
struct TransformContext<'a> {
    registry: &'a TypeRegistry,
    current_ns: NamespaceUri,
    current_package: String,
    imports: BTreeSet<String>,
    profile: SchemaProfile,
}

impl<'a> TransformContext<'a> {
    // -----------------------------------------------------------------------
    // Simple type -> Proto enum
    // -----------------------------------------------------------------------

    /// Transform a simple type into a proto enum if it has enumeration variants.
    /// For pattern-only or other restriction types, returns None (they map
    /// to their base primitive type at usage sites).
    fn transform_simple_type(&self, st: &XsdSimpleType) -> Option<ProtoEnum> {
        match &st.content {
            SimpleTypeContent::Enumeration { variants, .. } => {
                Some(self.build_enum(&st.name, variants, st.annotation.documentation.as_deref()))
            }
            _ => None,
        }
    }

    /// Build a ProtoEnum from enumeration variants.
    fn build_enum(
        &self,
        name: &str,
        variants: &[EnumVariant],
        documentation: Option<&str>,
    ) -> ProtoEnum {
        let mut values = Vec::new();

        // Check if there's already an UNKNOWN variant.
        let has_unknown = variants
            .iter()
            .any(|v| v.value.eq_ignore_ascii_case("unknown"));

        if !has_unknown {
            // Insert a synthetic UNKNOWN = 0 value.
            values.push(ProtoEnumValue {
                name: enum_unknown_name(name),
                number: 0,
                documentation: Some("Unspecified/unknown value.".to_string()),
            });
        }

        for (i, variant) in variants.iter().enumerate() {
            let number = if has_unknown && variant.value.eq_ignore_ascii_case("unknown") {
                0 // UNKNOWN gets number 0
            } else if has_unknown {
                // If UNKNOWN is in the original list, offset numbering to skip 0
                // for non-UNKNOWN values.
                let unknown_idx = variants
                    .iter()
                    .position(|v| v.value.eq_ignore_ascii_case("unknown"))
                    .unwrap();
                if i < unknown_idx {
                    (i + 1) as i32
                } else {
                    i as i32
                }
            } else {
                // UNKNOWN was synthetically added at 0, so real values start at 1.
                (i + 1) as i32
            };

            values.push(ProtoEnumValue {
                name: enum_value_name(name, &variant.value),
                number,
                documentation: variant.annotation.documentation.clone(),
            });
        }

        ProtoEnum {
            name: name.to_string(),
            values,
            documentation: documentation.map(|s| s.to_string()),
        }
    }

    // -----------------------------------------------------------------------
    // Complex type -> Proto message
    // -----------------------------------------------------------------------

    /// Transform a complex type into a proto message.
    fn transform_complex_type(&mut self, ct: &XsdComplexType, name: &str) -> Option<ProtoMessage> {
        let mut fields = Vec::new();
        let mut oneofs = Vec::new();
        let mut field_number = 1u32;

        match &ct.content {
            // Rule 1: complexContent/extension
            ComplexTypeContent::ComplexExtension { base, compositor } => {
                self.handle_extension_base(base, &mut fields, &mut oneofs, &mut field_number);

                if let Some(comp) = compositor {
                    self.flatten_compositor(comp, &mut fields, &mut oneofs, &mut field_number);
                }
            }

            // complexContent/restriction - treat similarly but with the restricted content.
            ComplexTypeContent::ComplexRestriction { base, compositor } => {
                self.handle_extension_base(base, &mut fields, &mut oneofs, &mut field_number);
                if let Some(comp) = compositor {
                    self.flatten_compositor(comp, &mut fields, &mut oneofs, &mut field_number);
                }
            }

            // simpleContent/extension - this should have been caught as a wrapper,
            // but if it wasn't (e.g., has custom attributes beyond SimpleObjectAttributeGroup),
            // emit a message with a `value` field.
            ComplexTypeContent::SimpleExtension { base } => {
                let type_name = self.resolve_type_name(base);
                fields.push(ProtoField {
                    name: "value".to_string(),
                    number: field_number,
                    type_name,
                    cardinality: FieldCardinality::Singular,
                    documentation: None,
                });
                field_number += 1;
            }

            ComplexTypeContent::SimpleRestriction { base } => {
                let type_name = self.resolve_type_name(base);
                fields.push(ProtoField {
                    name: "value".to_string(),
                    number: field_number,
                    type_name,
                    cardinality: FieldCardinality::Singular,
                    documentation: None,
                });
                field_number += 1;
            }

            // Direct content (sequence/choice/all).
            ComplexTypeContent::Direct { compositor } => {
                if let Some(comp) = compositor {
                    self.flatten_compositor(comp, &mut fields, &mut oneofs, &mut field_number);
                }
            }

            ComplexTypeContent::Empty => {}
        }

        // Rule 11: attributes
        self.handle_attributes(
            &ct.attributes,
            &ct.attribute_group_refs,
            &mut fields,
            &mut field_number,
        );

        // Rule 13: anyAttribute -> map<string, string>
        if ct.any_attribute.is_some() {
            fields.push(ProtoField {
                name: "extra_attributes".to_string(),
                number: field_number,
                type_name: "map<string, string>".to_string(),
                cardinality: FieldCardinality::Singular,
                documentation: Some("Catch-all for ISM/NTK attributes.".to_string()),
            });
            // field_number += 1; // not needed as last field
        }

        Some(ProtoMessage {
            name: name.to_string(),
            fields,
            oneofs,
            nested_messages: Vec::new(),
            nested_enums: Vec::new(),
            documentation: ct.annotation.documentation.clone(),
        })
    }

    // -----------------------------------------------------------------------
    // Extension base handling
    // -----------------------------------------------------------------------

    /// Handle an extension base type.
    ///
    /// For NIEM structures base types (ObjectType, AssociationType, etc.),
    /// we inline the standard attributes (id, ref, uri, metadata) as fields
    /// rather than emitting a base field.
    ///
    /// For other base types, we emit a composition field.
    fn handle_extension_base(
        &mut self,
        base: &QName,
        fields: &mut Vec<ProtoField>,
        oneofs: &mut Vec<ProtoOneof>,
        field_number: &mut u32,
    ) {
        if self.profile.should_inline_structures_base()
            && base.namespace.as_str() == STRUCTURES_NAMESPACE
        {
            // Inline the structures attributes.
            self.emit_structures_attributes(fields, field_number);

            // Check for ObjectAugmentationPoint or AssociationAugmentationPoint.
            let aug_point_name = match base.local_name.as_str() {
                "ObjectType" => Some("ObjectAugmentationPoint"),
                "AssociationType" => Some("AssociationAugmentationPoint"),
                _ => None,
            };

            if let Some(aug_name) = aug_point_name {
                let aug_qname = QName {
                    namespace: NamespaceUri(STRUCTURES_NAMESPACE.to_string()),
                    local_name: aug_name.to_string(),
                };
                // Only emit a oneof if there are known augmentations.
                if let Some(augmentations) = self.registry.get_augmentations(&aug_qname) {
                    if !augmentations.is_empty() {
                        let oneof = self.build_augmentation_oneof(
                            &to_snake_case(aug_name),
                            augmentations,
                            field_number,
                        );
                        oneofs.push(oneof);
                    }
                }
            }
        } else {
            // Non-structures base: emit a composition field.
            let type_name = self.resolve_type_name(base);
            let field_name = to_snake_case(
                base.local_name
                    .strip_suffix("Type")
                    .unwrap_or(&base.local_name),
            );
            fields.push(ProtoField {
                name: field_name,
                number: *field_number,
                type_name,
                cardinality: FieldCardinality::Singular,
                documentation: Some(format!("Base type: {}", base.local_name)),
            });
            *field_number += 1;
        }
    }

    /// Emit the standard NIEM structures attributes as fields.
    fn emit_structures_attributes(&self, fields: &mut Vec<ProtoField>, field_number: &mut u32) {
        let attrs = [
            ("structures_id", "string", "A document-relative identifier."),
            ("structures_ref", "string", "A document-relative reference."),
            ("structures_uri", "string", "A URI for this object."),
            ("structures_metadata", "string", "Metadata references."),
        ];

        for (name, type_name, doc) in &attrs {
            fields.push(ProtoField {
                name: name.to_string(),
                number: *field_number,
                type_name: type_name.to_string(),
                cardinality: FieldCardinality::Optional,
                documentation: Some(doc.to_string()),
            });
            *field_number += 1;
        }
    }

    // -----------------------------------------------------------------------
    // Compositor flattening
    // -----------------------------------------------------------------------

    /// Flatten a compositor (sequence/choice/all) into fields and oneofs.
    fn flatten_compositor(
        &mut self,
        comp: &Compositor,
        fields: &mut Vec<ProtoField>,
        oneofs: &mut Vec<ProtoOneof>,
        field_number: &mut u32,
    ) {
        match comp.kind {
            // Rule 7: xs:choice -> oneof
            CompositorKind::Choice => {
                let oneof = self.build_choice_oneof(comp, field_number);
                oneofs.push(oneof);
            }

            // Rule 8: xs:sequence -> ordered fields
            CompositorKind::Sequence | CompositorKind::All => {
                for item in &comp.items {
                    match item {
                        CompositorItem::Element(elem) => {
                            if let Some(field_or_oneof) = self.transform_element(elem, field_number)
                            {
                                match field_or_oneof {
                                    FieldOrOneof::Field(f) => fields.push(f),
                                    FieldOrOneof::Oneof(o) => oneofs.push(o),
                                }
                            }
                        }
                        CompositorItem::Compositor(nested) => {
                            self.flatten_compositor(nested, fields, oneofs, field_number);
                        }
                    }
                }
            }
        }
    }

    /// Build a oneof from a choice compositor.
    fn build_choice_oneof(&mut self, comp: &Compositor, field_number: &mut u32) -> ProtoOneof {
        let mut oneof_fields = Vec::new();

        // Try to derive a meaningful name from the choice items.
        let choice_name = derive_choice_name(&comp.items);

        for item in &comp.items {
            match item {
                CompositorItem::Element(elem) => {
                    let (name, type_name) = self.element_name_and_type(elem);
                    oneof_fields.push(ProtoField {
                        name,
                        number: *field_number,
                        type_name,
                        cardinality: FieldCardinality::Singular,
                        documentation: elem.annotation.documentation.clone(),
                    });
                    *field_number += 1;
                }
                CompositorItem::Compositor(_nested) => {
                    // Nested compositor inside choice - rare, emit a placeholder.
                    oneof_fields.push(ProtoField {
                        name: format!("choice_option_{}", field_number),
                        number: *field_number,
                        type_name: "string".to_string(),
                        cardinality: FieldCardinality::Singular,
                        documentation: Some("Nested compositor in choice.".to_string()),
                    });
                    *field_number += 1;
                }
            }
        }

        ProtoOneof {
            name: choice_name,
            fields: oneof_fields,
        }
    }

    // -----------------------------------------------------------------------
    // Element transform
    // -----------------------------------------------------------------------

    /// Transform a single element (either a reference or a declaration) into
    /// a field or a oneof (for substitution groups / augmentation points).
    fn transform_element(
        &mut self,
        elem: &XsdElement,
        field_number: &mut u32,
    ) -> Option<FieldOrOneof> {
        // Resolve the element — could be a ref or a declaration.
        let resolved = self.resolve_element(elem);

        // Check if this element is abstract and heads a substitution group.
        let elem_qname = self.element_qname(elem);

        if let Some(ref qname) = elem_qname {
            // Rule 6: augmentation points (NIEM-specific)
            if self.profile.is_augmentation_point(qname) {
                if let Some(augmentations) = self.registry.get_augmentations(qname) {
                    if !augmentations.is_empty() {
                        let oneof = self.build_augmentation_oneof(
                            &to_snake_case(&qname.local_name),
                            augmentations,
                            field_number,
                        );
                        return Some(FieldOrOneof::Oneof(oneof));
                    }
                }
                // No augmentations found — omit the field.
                return None;
            }

            // Rule 5: substitution groups (non-augmentation abstract elements)
            let is_abstract = resolved.map(|e| e.is_abstract).unwrap_or(false) || elem.is_abstract;
            if is_abstract {
                if let Some(members) = self.registry.get_substitution_group(qname) {
                    if !members.is_empty() {
                        let oneof = self.build_substitution_oneof(
                            &to_snake_case(&qname.local_name),
                            members,
                            field_number,
                            elem,
                        );
                        return Some(FieldOrOneof::Oneof(oneof));
                    }
                }
            }
        }

        // Regular element -> field.
        let (name, type_name) = self.element_name_and_type(elem);
        let cardinality = element_cardinality(elem);

        let field = ProtoField {
            name,
            number: *field_number,
            type_name,
            cardinality,
            documentation: elem.annotation.documentation.clone(),
        };
        *field_number += 1;

        Some(FieldOrOneof::Field(field))
    }

    /// Get the QName for an element (handles both refs and declarations).
    fn element_qname(&self, elem: &XsdElement) -> Option<QName> {
        if let Some(ref qname) = elem.element_ref {
            Some(qname.clone())
        } else {
            elem.name.as_ref().map(|name| QName {
                namespace: self.current_ns.clone(),
                local_name: name.clone(),
            })
        }
    }

    /// Resolve an element reference to the global element it points to.
    fn resolve_element(&self, elem: &XsdElement) -> Option<&XsdElement> {
        if let Some(ref qname) = elem.element_ref {
            self.registry.resolve_element(qname)
        } else {
            None
        }
    }

    /// Get the field name and proto type for an element.
    fn element_name_and_type(&mut self, elem: &XsdElement) -> (String, String) {
        if let Some(ref qname) = elem.element_ref {
            // Element reference: use the ref'd element's name and type.
            let field_name = to_snake_case(&qname.local_name);
            let type_name = if let Some(resolved) = self.registry.resolve_element(qname) {
                if let Some(ref type_ref) = resolved.type_ref {
                    self.resolve_type_name(type_ref)
                } else {
                    // Element with anonymous type or no type.
                    "string".to_string()
                }
            } else {
                "string".to_string()
            };
            (field_name, type_name)
        } else {
            // Inline element declaration.
            let field_name = to_snake_case(elem.name.as_deref().unwrap_or("unknown"));
            let type_name = if let Some(ref type_ref) = elem.type_ref {
                self.resolve_type_name(type_ref)
            } else {
                "string".to_string()
            };
            (field_name, type_name)
        }
    }

    // -----------------------------------------------------------------------
    // Substitution group / augmentation oneofs
    // -----------------------------------------------------------------------

    /// Build a oneof for a substitution group.
    fn build_substitution_oneof(
        &mut self,
        oneof_name: &str,
        members: &[QName],
        field_number: &mut u32,
        parent_elem: &XsdElement,
    ) -> ProtoOneof {
        let mut oneof_fields = Vec::new();

        for member in members {
            let field_name = to_snake_case(&member.local_name);
            let type_name = if let Some(resolved) = self.registry.resolve_element(member) {
                if let Some(ref type_ref) = resolved.type_ref {
                    self.resolve_type_name(type_ref)
                } else {
                    "string".to_string()
                }
            } else {
                "string".to_string()
            };

            oneof_fields.push(ProtoField {
                name: field_name,
                number: *field_number,
                type_name,
                cardinality: FieldCardinality::Singular,
                documentation: None,
            });
            *field_number += 1;
        }

        // If the parent element allows multiple occurrences, note it in docs
        // (oneof itself can't be repeated in proto3).
        let _ = parent_elem;

        ProtoOneof {
            name: oneof_name.to_string(),
            fields: oneof_fields,
        }
    }

    /// Build a oneof for an augmentation point.
    fn build_augmentation_oneof(
        &mut self,
        oneof_name: &str,
        augmentations: &[QName],
        field_number: &mut u32,
    ) -> ProtoOneof {
        let mut oneof_fields = Vec::new();

        for aug in augmentations {
            let field_name = to_snake_case(&aug.local_name);
            let type_name = if let Some(resolved) = self.registry.resolve_element(aug) {
                if let Some(ref type_ref) = resolved.type_ref {
                    self.resolve_type_name(type_ref)
                } else {
                    "string".to_string()
                }
            } else {
                "string".to_string()
            };

            oneof_fields.push(ProtoField {
                name: field_name,
                number: *field_number,
                type_name,
                cardinality: FieldCardinality::Singular,
                documentation: None,
            });
            *field_number += 1;
        }

        ProtoOneof {
            name: oneof_name.to_string(),
            fields: oneof_fields,
        }
    }

    // -----------------------------------------------------------------------
    // Attribute handling
    // -----------------------------------------------------------------------

    /// Handle attributes on a complex type.
    fn handle_attributes(
        &mut self,
        attrs: &[XsdAttribute],
        attr_group_refs: &[QName],
        fields: &mut Vec<ProtoField>,
        field_number: &mut u32,
    ) {
        for group_ref in attr_group_refs {
            // Skip structures:SimpleObjectAttributeGroup (already handled when
            // inlining structures base).
            if self.profile.should_skip_structures_attributes()
                && group_ref.namespace.as_str() == STRUCTURES_NAMESPACE
                && group_ref.local_name == "SimpleObjectAttributeGroup"
            {
                continue;
            }
            // Resolve the attribute group and inline its attributes.
            if let Some(group) = self.registry_lookup_attribute_group(group_ref) {
                let group_attrs = group.attributes.clone();
                for attr in &group_attrs {
                    self.emit_attribute_field(attr, fields, field_number);
                }
            }
        }

        for attr in attrs {
            // Skip structures: namespace attributes (already handled).
            if self.profile.should_skip_structures_attributes() {
                if let Some(ref attr_ref) = attr.attribute_ref {
                    if attr_ref.namespace.as_str() == STRUCTURES_NAMESPACE {
                        continue;
                    }
                }
            }
            self.emit_attribute_field(attr, fields, field_number);
        }
    }

    /// Look up an attribute group in the registry.
    fn registry_lookup_attribute_group(&self, qname: &QName) -> Option<&XsdAttributeGroup> {
        let schema = self.registry.schemas.get(&qname.namespace)?;
        schema
            .attribute_groups
            .iter()
            .find(|g| g.name == qname.local_name)
    }

    /// Emit a single attribute as a proto field.
    fn emit_attribute_field(
        &mut self,
        attr: &XsdAttribute,
        fields: &mut Vec<ProtoField>,
        field_number: &mut u32,
    ) {
        let (name, type_name) = if let Some(ref attr_ref) = attr.attribute_ref {
            let field_name = to_snake_case(&attr_ref.local_name);
            let tname = if let Some(ref type_ref) = attr.type_ref {
                self.resolve_type_name(type_ref)
            } else {
                // Look up the global attribute for its type.
                self.resolve_global_attribute_type(attr_ref)
            };
            (field_name, tname)
        } else {
            let field_name = to_snake_case(attr.name.as_deref().unwrap_or("attr"));
            let tname = if let Some(ref type_ref) = attr.type_ref {
                self.resolve_type_name(type_ref)
            } else {
                "string".to_string()
            };
            (field_name, tname)
        };

        let cardinality = if attr.use_required {
            FieldCardinality::Singular
        } else {
            FieldCardinality::Optional
        };

        fields.push(ProtoField {
            name,
            number: *field_number,
            type_name,
            cardinality,
            documentation: attr.annotation.documentation.clone(),
        });
        *field_number += 1;
    }

    /// Resolve the type of a global attribute by looking it up in the registry.
    fn resolve_global_attribute_type(&mut self, qname: &QName) -> String {
        let schema = self.registry.schemas.get(&qname.namespace);
        if let Some(schema) = schema {
            for attr in &schema.attributes {
                if attr.name.as_deref() == Some(qname.local_name.as_str()) {
                    if let Some(ref type_ref) = attr.type_ref {
                        return self.resolve_type_name(type_ref);
                    }
                }
            }
        }
        "string".to_string()
    }

    // -----------------------------------------------------------------------
    // Type resolution
    // -----------------------------------------------------------------------

    /// Resolve a QName type reference to a proto type string.
    ///
    /// This handles:
    /// - XSD built-in types (xs:string, xs:int, etc.)
    /// - NIEM proxy types (niem-xs:string, niem-xs:double, etc.) -> Rule 4
    /// - NIEM wrapper types (Rule 3) -> collapse to the underlying enum/primitive
    /// - Regular named types -> fully-qualified proto type name
    fn resolve_type_name(&mut self, qname: &QName) -> String {
        // Rule 4: XSD built-in types.
        if qname.namespace.as_str() == XS_NAMESPACE {
            return xsd_builtin_to_proto(&qname.local_name);
        }

        // Rule 4: NIEM proxy types -> unwrap to proto builtins.
        if self.profile.should_use_niem_proxy_mapping()
            && qname.namespace.as_str() == NIEM_XS_NAMESPACE
        {
            return niem_xs_to_proto(&qname.local_name);
        }

        // Rule 3: Check if the referenced type is a NIEM wrapper.
        if self.profile.should_collapse_niem_wrappers() {
            if let Some(ct) = self.registry.resolve_complex_type(qname) {
                let ct_clone = ct.clone();
                if self.is_niem_wrapper(&ct_clone) {
                    return self.unwrap_niem_wrapper(&ct_clone);
                }
            }
        }

        // Rule 12: Check if it's a simple type with pattern only -> string.
        if let Some(st) = self.registry.resolve_simple_type(qname) {
            match &st.content {
                SimpleTypeContent::Pattern { .. } => return "string".to_string(),
                SimpleTypeContent::Enumeration { .. } => {
                    // Reference to an enum type.
                    return self.qualified_proto_type_name(qname, &st.name);
                }
                SimpleTypeContent::LengthRestriction { base, .. }
                | SimpleTypeContent::Range { base, .. }
                | SimpleTypeContent::Restriction { base, .. } => {
                    // Collapse to the base primitive type.
                    return self.resolve_type_name(base);
                }
                SimpleTypeContent::List { .. } => return "string".to_string(),
                SimpleTypeContent::Union { .. } => return "string".to_string(),
            }
        }

        // Regular complex type -> qualified name.
        // If it's not in the registry at all, warn about the missing type.
        if self.registry.resolve_complex_type(qname).is_none()
            && self.registry.resolve_simple_type(qname).is_none()
        {
            tracing::warn!(
                qname = %qname,
                "type not found in registry; falling back to qualified name"
            );
        }

        let target_package = namespace_to_package(qname.namespace.as_str());
        self.maybe_add_import(&target_package);

        format!("{}.{}", target_package, qname.local_name)
    }

    /// Build a fully-qualified proto type name and track the import.
    fn qualified_proto_type_name(&mut self, qname: &QName, type_name: &str) -> String {
        let target_package = namespace_to_package(qname.namespace.as_str());
        self.maybe_add_import(&target_package);
        format!("{}.{}", target_package, type_name)
    }

    /// Add an import for a target package if it differs from the current package.
    fn maybe_add_import(&mut self, target_package: &str) {
        if target_package != self.current_package {
            let import_path = package_to_import_path(target_package);
            self.imports.insert(import_path);
        }
    }

    // -----------------------------------------------------------------------
    // NIEM wrapper detection and unwrapping (Rule 3)
    // -----------------------------------------------------------------------

    /// Check if a complex type is a NIEM wrapper:
    /// - simpleContent/extension where the base resolves to a simple type with
    ///   enumerations and the extension adds only SimpleObjectAttributeGroup
    /// - Or: simpleContent/extension where the base is a primitive (xs:string, etc.)
    ///   with only SimpleObjectAttributeGroup
    fn is_niem_wrapper(&self, ct: &XsdComplexType) -> bool {
        if let ComplexTypeContent::SimpleExtension { ref base } = ct.content {
            // Check that attributes are only SimpleObjectAttributeGroup.
            let has_only_simple_obj_attrs = ct.attributes.is_empty()
                && ct.attribute_group_refs.iter().all(|r| {
                    r.namespace.as_str() == STRUCTURES_NAMESPACE
                        && r.local_name == "SimpleObjectAttributeGroup"
                });

            if !has_only_simple_obj_attrs {
                return false;
            }

            // Check if the base is a primitive XSD type.
            if base.namespace.as_str() == XS_NAMESPACE {
                return true;
            }

            // Check if the base is a simple type with enumerations.
            if let Some(st) = self.registry.resolve_simple_type(base) {
                return matches!(st.content, SimpleTypeContent::Enumeration { .. });
            }

            // Check if the base is a simple type with pattern/restriction.
            if let Some(st) = self.registry.resolve_simple_type(base) {
                return matches!(
                    st.content,
                    SimpleTypeContent::Pattern { .. }
                        | SimpleTypeContent::Restriction { .. }
                        | SimpleTypeContent::LengthRestriction { .. }
                        | SimpleTypeContent::Range { .. }
                );
            }

            return false;
        }
        false
    }

    /// Unwrap a NIEM wrapper to get the underlying proto type.
    fn unwrap_niem_wrapper(&mut self, ct: &XsdComplexType) -> String {
        if let ComplexTypeContent::SimpleExtension { ref base } = ct.content {
            return self.resolve_type_name(base);
        }
        "string".to_string()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Result of transforming a single element: either a field or a oneof.
enum FieldOrOneof {
    Field(ProtoField),
    Oneof(ProtoOneof),
}

/// Determine the cardinality of an element.
fn element_cardinality(elem: &XsdElement) -> FieldCardinality {
    // Rule 9: maxOccurs="unbounded" -> repeated
    if elem.max_occurs == Occurs::Unbounded {
        return FieldCardinality::Repeated;
    }
    if let Occurs::Count(max) = elem.max_occurs {
        if max > 1 {
            return FieldCardinality::Repeated;
        }
    }

    // Rule 10: minOccurs="0" -> optional
    if elem.min_occurs == Occurs::Count(0) {
        return FieldCardinality::Optional;
    }

    FieldCardinality::Singular
}

/// Derive a name for a choice oneof from its items.
fn derive_choice_name(items: &[CompositorItem]) -> String {
    // If all items are element refs/declarations, try to find a common suffix
    // or just use "choice".
    if items.len() <= 5 {
        // Try to build a name from the first element.
        if let Some(CompositorItem::Element(elem)) = items.first() {
            if let Some(ref qname) = elem.element_ref {
                // Use a generic "choice" name based on the first element
                // with "_or_" for small choices.
                return format!("{}_choice", to_snake_case(&qname.local_name));
            }
            if let Some(ref name) = elem.name {
                return format!("{}_choice", to_snake_case(name));
            }
        }
    }
    "choice".to_string()
}

// ---------------------------------------------------------------------------
// XSD -> Proto type mapping
// ---------------------------------------------------------------------------

/// Map an XSD built-in type to a proto type.
fn xsd_builtin_to_proto(local_name: &str) -> String {
    match local_name {
        "string" | "token" | "anyURI" | "NMTOKEN" | "normalizedString" => "string".to_string(),
        "boolean" => "bool".to_string(),
        "int" | "short" => "int32".to_string(),
        "integer" | "long" => "int64".to_string(),
        "nonNegativeInteger" | "positiveInteger" | "unsignedInt" | "unsignedLong" => {
            "uint64".to_string()
        }
        "double" | "decimal" => "double".to_string(),
        "float" => "float".to_string(),
        "dateTime" => "google.protobuf.Timestamp".to_string(),
        "duration" => "google.protobuf.Duration".to_string(),
        "base64Binary" | "hexBinary" => "bytes".to_string(),
        "date" => "string".to_string(),
        "ID" | "IDREF" | "IDREFS" => "string".to_string(),
        _ => "string".to_string(), // safe fallback
    }
}

/// Map a NIEM proxy type (niem-xs:*) to a proto built-in.
///
/// niem-xs proxy types are just wrappers around xs: types with
/// SimpleObjectAttributeGroup. We map them to the same proto type
/// as the corresponding xs: type.
fn niem_xs_to_proto(local_name: &str) -> String {
    // The niem-xs type names exactly match the xs: type names
    // (they are lower-camelCase, e.g., "string", "double", "boolean").
    xsd_builtin_to_proto(local_name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::build_test_registry;
    use crate::transform::profile::SchemaProfile;

    // -- Integration: transform a namespace ---------------------------------

    #[test]
    fn transform_vehicle_namespace() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/vehicle".to_string());

        let proto =
            transform_schema(&ns, &reg, SchemaProfile::Niem).expect("should produce a ProtoFile");

        assert_eq!(proto.syntax, "proto3");
        assert_eq!(proto.package, "example_com.schemas.vehicle");
        assert!(
            !proto.messages.is_empty(),
            "should have at least one message"
        );
    }

    #[test]
    fn vehicle_type_has_expected_fields() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/vehicle".to_string());

        let proto = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        let veh_msg = proto
            .messages
            .iter()
            .find(|m| m.name == "VehicleType")
            .expect("should have VehicleType message");

        // Should have structures attributes (id, ref, uri, metadata).
        let field_names: Vec<&str> = veh_msg.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(
            field_names.contains(&"structures_id"),
            "should have structures_id field, got: {field_names:?}"
        );

        // Should have Identification field.
        assert!(
            field_names.contains(&"identification"),
            "should have identification field, got: {field_names:?}"
        );

        // Should have entity_details field.
        assert!(
            field_names.contains(&"entity_details"),
            "should have entity_details field, got: {field_names:?}"
        );

        // Should have audit_record field (repeated).
        let audit_field = veh_msg
            .fields
            .iter()
            .find(|f| f.name == "audit_record")
            .expect("should have audit_record field");
        assert_eq!(
            audit_field.cardinality,
            FieldCardinality::Repeated,
            "audit_record should be repeated"
        );
    }

    #[test]
    fn inspection_report_type_has_substitution_oneof() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/vehicle".to_string());

        let proto = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        // InspectionReportType references StatusCodeAbstract which has
        // a substitution group (StatusCode). This should produce a oneof.
        let ir_msg = proto
            .messages
            .iter()
            .find(|m| m.name == "InspectionReportType")
            .expect("should have InspectionReportType message");

        let oneof_names: Vec<&str> = ir_msg.oneofs.iter().map(|o| o.name.as_str()).collect();
        assert!(
            !oneof_names.is_empty(),
            "InspectionReportType should have a oneof for StatusCodeAbstract substitution group, got: {oneof_names:?}"
        );

        // The oneof should contain "status_code" as one of its members.
        let status_oneof = ir_msg
            .oneofs
            .iter()
            .find(|o| o.name.contains("status_code"))
            .expect("should have a oneof related to status_code");

        let field_names: Vec<&str> = status_oneof
            .fields
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert!(
            field_names.contains(&"status_code"),
            "status oneof should contain status_code, got: {field_names:?}"
        );
    }

    #[test]
    fn vehicle_type_augmentation_point_omitted_when_no_augmentations() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/vehicle".to_string());

        let proto = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        let veh_msg = proto
            .messages
            .iter()
            .find(|m| m.name == "VehicleType")
            .expect("should have VehicleType message");

        // VehicleAugmentationPoint has no concrete augmentations
        // in the test dataset, so it should NOT appear as a field or oneof.
        let has_aug_field = veh_msg
            .fields
            .iter()
            .any(|f| f.name.contains("augmentation"));
        let has_aug_oneof = veh_msg
            .oneofs
            .iter()
            .any(|o| o.name.contains("augmentation"));

        assert!(
            !has_aug_field && !has_aug_oneof,
            "VehicleAugmentationPoint should be omitted when no augmentations exist"
        );
    }

    #[test]
    fn confidence_code_simple_type_becomes_enum() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/common-types".to_string());

        let proto = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        let confidence_enum = proto
            .enums
            .iter()
            .find(|e| e.name == "ConfidenceCodeSimpleType")
            .expect("should have ConfidenceCodeSimpleType enum");

        // Should have UNKNOWN at number 0.
        let unknown_val = confidence_enum
            .values
            .iter()
            .find(|v| v.name.contains("UNKNOWN"))
            .expect("should have an UNKNOWN value");
        assert_eq!(unknown_val.number, 0, "UNKNOWN should be at number 0");

        // Should have HIGH value.
        let high_val = confidence_enum
            .values
            .iter()
            .find(|v| v.name.contains("HIGH") && !v.name.contains("VERY"))
            .expect("should have a HIGH value");
        assert!(
            high_val.name.starts_with("CONFIDENCE_CODE_"),
            "HIGH value should be prefixed: {}",
            high_val.name
        );
    }

    #[test]
    fn niem_xs_types_map_to_builtins() {
        // niem-xs:string -> "string", niem-xs:double -> "double", etc.
        assert_eq!(niem_xs_to_proto("string"), "string");
        assert_eq!(niem_xs_to_proto("double"), "double");
        assert_eq!(niem_xs_to_proto("boolean"), "bool");
        assert_eq!(niem_xs_to_proto("integer"), "int64");
        assert_eq!(niem_xs_to_proto("dateTime"), "google.protobuf.Timestamp");
        assert_eq!(niem_xs_to_proto("duration"), "google.protobuf.Duration");
        assert_eq!(niem_xs_to_proto("base64Binary"), "bytes");
        assert_eq!(niem_xs_to_proto("nonNegativeInteger"), "uint64");
    }

    #[test]
    fn niem_wrapper_types_are_collapsed() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/common-types".to_string());

        let proto = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        // CapabilityConfidenceCodeType is a NIEM wrapper around
        // ConfidenceCodeSimpleType. It should NOT appear as a message.
        let has_wrapper_msg = proto
            .messages
            .iter()
            .any(|m| m.name == "CapabilityConfidenceCodeType");
        assert!(
            !has_wrapper_msg,
            "CapabilityConfidenceCodeType should be collapsed (not emitted as message)"
        );
    }

    #[test]
    fn composite_object_type_has_choice_oneof() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/core".to_string());

        let proto = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        let co_msg = proto
            .messages
            .iter()
            .find(|m| m.name == "CompositeObjectType")
            .expect("should have CompositeObjectType message");

        // Should have a oneof for the choice.
        assert!(
            !co_msg.oneofs.is_empty(),
            "CompositeObjectType should have at least one oneof for the xs:choice"
        );

        // The choice should contain vehicle as one of the options.
        let choice_oneof = &co_msg.oneofs[0];
        let choice_field_names: Vec<&str> = choice_oneof
            .fields
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert!(
            choice_field_names.contains(&"vehicle"),
            "choice oneof should contain vehicle, got: {choice_field_names:?}"
        );
    }

    #[test]
    fn structures_namespace_produces_no_messages() {
        let reg = build_test_registry();
        let ns = NamespaceUri(STRUCTURES_NAMESPACE.to_string());

        let proto = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        // We skip structures types (they are inlined into extending types).
        assert!(
            proto.messages.is_empty(),
            "structures namespace should produce no messages"
        );
    }

    #[test]
    fn transform_returns_none_for_unknown_namespace() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/nonexistent".to_string());

        assert!(
            transform_schema(&ns, &reg, SchemaProfile::Niem).is_none(),
            "should return None for unknown namespace"
        );
    }

    #[test]
    fn field_numbers_are_sequential_starting_from_1() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/common-types".to_string());

        let proto = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        for msg in &proto.messages {
            // Collect all field numbers (from both regular fields and oneofs).
            let mut all_numbers: Vec<u32> = msg.fields.iter().map(|f| f.number).collect();
            for oneof in &msg.oneofs {
                for f in &oneof.fields {
                    all_numbers.push(f.number);
                }
            }

            if all_numbers.is_empty() {
                continue;
            }

            all_numbers.sort();
            assert_eq!(
                all_numbers[0], 1,
                "field numbers in {} should start at 1",
                msg.name
            );

            // Check they are sequential.
            for window in all_numbers.windows(2) {
                assert_eq!(
                    window[1],
                    window[0] + 1,
                    "field numbers in {} should be sequential: {:?}",
                    msg.name,
                    all_numbers
                );
            }
        }
    }

    #[test]
    fn imports_are_tracked() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/core".to_string());

        let proto = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        // core references veh:Vehicle, so it should import that package.
        assert!(
            proto.imports.iter().any(|i| i.contains("vehicle")),
            "should import vehicle package, got: {:?}",
            proto.imports
        );
    }

    #[test]
    fn xsd_builtin_type_mapping() {
        assert_eq!(xsd_builtin_to_proto("string"), "string");
        assert_eq!(xsd_builtin_to_proto("token"), "string");
        assert_eq!(xsd_builtin_to_proto("anyURI"), "string");
        assert_eq!(xsd_builtin_to_proto("boolean"), "bool");
        assert_eq!(xsd_builtin_to_proto("int"), "int32");
        assert_eq!(xsd_builtin_to_proto("short"), "int32");
        assert_eq!(xsd_builtin_to_proto("integer"), "int64");
        assert_eq!(xsd_builtin_to_proto("long"), "int64");
        assert_eq!(xsd_builtin_to_proto("nonNegativeInteger"), "uint64");
        assert_eq!(xsd_builtin_to_proto("positiveInteger"), "uint64");
        assert_eq!(xsd_builtin_to_proto("unsignedInt"), "uint64");
        assert_eq!(xsd_builtin_to_proto("double"), "double");
        assert_eq!(xsd_builtin_to_proto("decimal"), "double");
        assert_eq!(xsd_builtin_to_proto("float"), "float");
        assert_eq!(
            xsd_builtin_to_proto("dateTime"),
            "google.protobuf.Timestamp"
        );
        assert_eq!(xsd_builtin_to_proto("duration"), "google.protobuf.Duration");
        assert_eq!(xsd_builtin_to_proto("base64Binary"), "bytes");
        assert_eq!(xsd_builtin_to_proto("hexBinary"), "bytes");
        assert_eq!(xsd_builtin_to_proto("date"), "string");
        assert_eq!(xsd_builtin_to_proto("ID"), "string");
        assert_eq!(xsd_builtin_to_proto("IDREF"), "string");
        assert_eq!(xsd_builtin_to_proto("IDREFS"), "string");
    }

    // -- Generic profile tests -----------------------------------------------
    //
    // These use small, self-contained inline XSD fixtures with synthetic
    // namespaces so they are independent of any real-world schema set.

    use crate::resolver::build_type_registry as build_reg;
    use crate::test_fixtures::{NIEM_XS_XSD, STRUCTURES_XSD};
    use crate::xsd::parser::parse_schema;
    use std::path::{Path, PathBuf};

    /// Build a minimal registry from inline XSD strings.
    fn generic_registry(xsds: &[&str]) -> TypeRegistry {
        let mut schemas = Vec::new();
        for xml in xsds {
            let schema =
                parse_schema(xml, Path::new("test.xsd")).expect("failed to parse inline XSD");
            schemas.push((schema, PathBuf::from("test.xsd")));
        }
        build_reg(schemas)
    }

    #[test]
    fn generic_structures_namespace_produces_messages() {
        let reg = generic_registry(&[STRUCTURES_XSD]);
        let ns = NamespaceUri(STRUCTURES_NAMESPACE.to_string());

        let proto = transform_schema(&ns, &reg, SchemaProfile::Generic).unwrap();

        assert!(
            !proto.messages.is_empty(),
            "generic profile should emit messages for structures namespace"
        );
    }

    #[test]
    fn generic_niem_wrapper_types_not_collapsed() {
        // A simpleContent/extension wrapping an enum with only
        // SimpleObjectAttributeGroup — a classic NIEM wrapper pattern.
        let wrapper_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema targetNamespace="http://example.com/wrapper"
            xmlns:w="http://example.com/wrapper"
            xmlns:structures="http://release.niem.gov/niem/structures/5.0/"
            xmlns:xs="http://www.w3.org/2001/XMLSchema">
          <xs:import namespace="http://release.niem.gov/niem/structures/5.0/"/>
          <xs:simpleType name="ColorSimpleType">
            <xs:restriction base="xs:token">
              <xs:enumeration value="RED"/>
              <xs:enumeration value="GREEN"/>
            </xs:restriction>
          </xs:simpleType>
          <xs:complexType name="ColorCodeType">
            <xs:simpleContent>
              <xs:extension base="w:ColorSimpleType">
                <xs:attributeGroup ref="structures:SimpleObjectAttributeGroup"/>
              </xs:extension>
            </xs:simpleContent>
          </xs:complexType>
        </xs:schema>"#;

        let reg = generic_registry(&[STRUCTURES_XSD, wrapper_xsd]);
        let ns = NamespaceUri("http://example.com/wrapper".to_string());
        let proto = transform_schema(&ns, &reg, SchemaProfile::Generic).unwrap();

        let wrapper_msg = proto
            .messages
            .iter()
            .find(|m| m.name == "ColorCodeType")
            .expect("generic profile should emit ColorCodeType as a message");

        let field_names: Vec<&str> = wrapper_msg.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(
            field_names.contains(&"value"),
            "wrapper message should have a 'value' field, got: {field_names:?}"
        );
    }

    #[test]
    fn generic_extension_base_is_composition_field() {
        // A type extending structures:ObjectType — in generic mode the base
        // should become a composition field, not inlined attributes.
        let ext_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema targetNamespace="http://example.com/ext"
            xmlns:e="http://example.com/ext"
            xmlns:structures="http://release.niem.gov/niem/structures/5.0/"
            xmlns:xs="http://www.w3.org/2001/XMLSchema">
          <xs:import namespace="http://release.niem.gov/niem/structures/5.0/"/>
          <xs:complexType name="VehicleType">
            <xs:complexContent>
              <xs:extension base="structures:ObjectType">
                <xs:sequence>
                  <xs:element name="Make" type="xs:string"/>
                </xs:sequence>
              </xs:extension>
            </xs:complexContent>
          </xs:complexType>
        </xs:schema>"#;

        let reg = generic_registry(&[STRUCTURES_XSD, ext_xsd]);
        let ns = NamespaceUri("http://example.com/ext".to_string());
        let proto = transform_schema(&ns, &reg, SchemaProfile::Generic).unwrap();

        let msg = proto
            .messages
            .iter()
            .find(|m| m.name == "VehicleType")
            .expect("should have VehicleType message");

        let field_names: Vec<&str> = msg.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(
            !field_names.contains(&"structures_id"),
            "generic profile should NOT inline structures_id, got: {field_names:?}"
        );
        assert!(
            field_names.contains(&"object"),
            "generic profile should have 'object' composition field for ObjectType base, got: {field_names:?}"
        );
    }

    #[test]
    fn generic_augmentation_point_not_special() {
        // An element named *AugmentationPoint — in generic mode it should NOT
        // receive special NIEM augmentation handling.
        let aug_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema targetNamespace="http://example.com/aug"
            xmlns:a="http://example.com/aug"
            xmlns:structures="http://release.niem.gov/niem/structures/5.0/"
            xmlns:xs="http://www.w3.org/2001/XMLSchema">
          <xs:import namespace="http://release.niem.gov/niem/structures/5.0/"/>
          <xs:element name="ThingAugmentationPoint" abstract="true"/>
          <xs:element name="ExtraField" type="xs:string" substitutionGroup="a:ThingAugmentationPoint"/>
          <xs:complexType name="ThingType">
            <xs:complexContent>
              <xs:extension base="structures:ObjectType">
                <xs:sequence>
                  <xs:element ref="a:ThingAugmentationPoint" minOccurs="0" maxOccurs="unbounded"/>
                </xs:sequence>
              </xs:extension>
            </xs:complexContent>
          </xs:complexType>
        </xs:schema>"#;

        let reg = generic_registry(&[STRUCTURES_XSD, aug_xsd]);
        let ns = NamespaceUri("http://example.com/aug".to_string());

        let proto_niem = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();
        let proto_generic = transform_schema(&ns, &reg, SchemaProfile::Generic).unwrap();

        // In NIEM mode the augmentation point produces a special oneof.
        let niem_msg = proto_niem
            .messages
            .iter()
            .find(|m| m.name == "ThingType")
            .unwrap();
        let niem_has_aug_oneof = niem_msg
            .oneofs
            .iter()
            .any(|o| o.name.contains("augmentation"));
        assert!(
            niem_has_aug_oneof,
            "NIEM profile should produce augmentation oneof"
        );

        // In generic mode the same element is treated as a regular
        // substitution group (since it is abstract).
        let generic_msg = proto_generic
            .messages
            .iter()
            .find(|m| m.name == "ThingType")
            .unwrap();
        let generic_oneof = generic_msg
            .oneofs
            .iter()
            .find(|o| o.name.contains("thing_augmentation_point"));
        assert!(
            generic_oneof.is_some(),
            "generic profile should handle augmentation point as a regular substitution group oneof"
        );
    }

    #[test]
    fn generic_niem_proxy_types_not_collapsed() {
        // A type referencing niem-xs:string — in generic mode it should NOT be
        // collapsed to proto `string`.
        let proxy_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema targetNamespace="http://example.com/proxy"
            xmlns:p="http://example.com/proxy"
            xmlns:structures="http://release.niem.gov/niem/structures/5.0/"
            xmlns:niem-xs="http://release.niem.gov/niem/proxy/niem-xs/5.0/"
            xmlns:xs="http://www.w3.org/2001/XMLSchema">
          <xs:import namespace="http://release.niem.gov/niem/structures/5.0/"/>
          <xs:import namespace="http://release.niem.gov/niem/proxy/niem-xs/5.0/"/>
          <xs:element name="Label" type="niem-xs:string"/>
          <xs:complexType name="ItemType">
            <xs:complexContent>
              <xs:extension base="structures:ObjectType">
                <xs:sequence>
                  <xs:element ref="p:Label"/>
                </xs:sequence>
              </xs:extension>
            </xs:complexContent>
          </xs:complexType>
        </xs:schema>"#;

        let reg = generic_registry(&[STRUCTURES_XSD, NIEM_XS_XSD, proxy_xsd]);
        let ns = NamespaceUri("http://example.com/proxy".to_string());
        let proto = transform_schema(&ns, &reg, SchemaProfile::Generic).unwrap();

        let msg = proto
            .messages
            .iter()
            .find(|m| m.name == "ItemType")
            .unwrap();

        let label_field = msg
            .fields
            .iter()
            .find(|f| f.name == "label")
            .expect("should have label field");

        assert_ne!(
            label_field.type_name, "string",
            "generic profile should not collapse niem-xs:string to proto string"
        );
        assert!(
            label_field.type_name.contains("niem"),
            "generic profile should reference niem proxy type: {}",
            label_field.type_name
        );
    }

    #[test]
    fn generic_structures_attributes_not_skipped() {
        // A type with structures:SimpleObjectAttributeGroup — in generic mode
        // those attributes should be emitted, not silently dropped.
        let attr_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema targetNamespace="http://example.com/attrs"
            xmlns:at="http://example.com/attrs"
            xmlns:structures="http://release.niem.gov/niem/structures/5.0/"
            xmlns:xs="http://www.w3.org/2001/XMLSchema">
          <xs:import namespace="http://release.niem.gov/niem/structures/5.0/"/>
          <xs:complexType name="WidgetType">
            <xs:complexContent>
              <xs:extension base="structures:ObjectType">
                <xs:sequence>
                  <xs:element name="Name" type="xs:string"/>
                </xs:sequence>
              </xs:extension>
            </xs:complexContent>
          </xs:complexType>
        </xs:schema>"#;

        let reg = generic_registry(&[STRUCTURES_XSD, attr_xsd]);
        let ns = NamespaceUri("http://example.com/attrs".to_string());
        let proto = transform_schema(&ns, &reg, SchemaProfile::Generic).unwrap();

        let msg = proto
            .messages
            .iter()
            .find(|m| m.name == "WidgetType")
            .expect("should have WidgetType message");

        let field_names: Vec<&str> = msg.fields.iter().map(|f| f.name.as_str()).collect();
        // In generic mode, structures attributes are NOT inlined via the
        // special NIEM path. The ObjectType base becomes a composition field.
        assert!(
            !field_names.contains(&"structures_id"),
            "generic profile should not inline structures attributes, got: {field_names:?}"
        );
        assert!(
            field_names.contains(&"object"),
            "generic profile should have composition field for base, got: {field_names:?}"
        );
    }
}
