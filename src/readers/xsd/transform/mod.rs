//! Transform layer that converts parsed XSD into the schemata IR.
//!
//! The main entry point is [`transform_schema`], which takes a namespace URI
//! and a [`TypeRegistry`] and produces one [`Schema`] (the IR root) for that
//! namespace. Writers (e.g. the proto lowering) consume the IR from here.

pub mod naming;
pub mod profile;

use self::profile::SchemaProfile;

use std::collections::BTreeSet;

use crate::ir::model::{
    Alias, Annotation, AnnotationValue, Cardinality, Choice, Decl, EnumDecl, EnumValue, Field,
    Member, Primitive, Record, Schema, TypeRef,
};
use crate::readers::xsd::model::*;
use crate::readers::xsd::names::{STRUCTURES_NAMESPACE, XS_NAMESPACE};
use crate::readers::xsd::resolver::TypeRegistry;

use self::naming::*;

/// The namespace URI for NIEM proxy types (niem-xs).
const NIEM_XS_NAMESPACE: &str = "http://release.niem.gov/niem/proxy/niem-xs/5.0/";

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Transform a single namespace from the XSD registry into an IR [`Schema`].
///
/// Returns `None` if the namespace has no schema in the registry.
pub fn transform_schema(
    ns: &NamespaceUri,
    registry: &TypeRegistry,
    profile: SchemaProfile,
) -> Option<Schema> {
    let schema = registry.schemas.get(ns)?;
    let package = namespace_to_package(ns.as_str());

    let mut ctx = TransformContext {
        registry,
        current_ns: ns.clone(),
        current_package: package.clone(),
        imports: BTreeSet::new(),
        profile,
    };

    let mut decls = Vec::new();

    // Transform simple types (enums, constraint aliases, etc.).
    for st in &schema.simple_types {
        if let Some(decl) = ctx.transform_simple_type(st) {
            decls.push(decl);
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

            if let Some(record) = ctx.transform_complex_type(ct, name) {
                decls.push(Decl::Record(record));
            }
        }
    }

    // Imports are package names of other IR schemas (deduplicated, sorted).
    let imports: Vec<String> = ctx.imports.into_iter().collect();

    let source_path = registry.source_paths.get(ns).cloned();

    Some(Schema {
        name: package,
        annotations: vec![Annotation::str("xml.namespace", ns.as_str())],
        imports,
        decls,
        source_path,
    })
}

/// Convert the transform's internal proto-style type string into an IR TypeRef.
fn type_ref_from_type_string(s: &str) -> TypeRef {
    match s {
        "double" => return TypeRef::Primitive(Primitive::Float64),
        "float" => return TypeRef::Primitive(Primitive::Float32),
        _ => {}
    }
    if let Some(p) = Primitive::from_name(s) {
        return TypeRef::Primitive(p);
    }
    match s.rsplit_once('.') {
        Some((pkg, name)) => TypeRef::named(Some(pkg), name),
        None => TypeRef::named(None, s),
    }
}

/// Convert a raw XSD range facet value into an annotation argument:
/// an integer when it parses as one, the raw string otherwise (e.g. "-273.15").
fn range_arg(v: &str) -> AnnotationValue {
    match v.parse::<i64>() {
        Ok(n) => AnnotationValue::Int(n),
        Err(_) => AnnotationValue::Str(v.to_string()),
    }
}

/// True if `s` is a valid identifier: `[A-Za-z_][A-Za-z0-9_]*`.
fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Sanitize a raw string into an identifier: non-alphanumeric characters
/// become `_`, and a leading digit gets a `_` prefix.
fn sanitize_identifier(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// Build an IR enum value from a raw XSD enumeration value.
///
/// The raw value is preserved exactly: as the value name when it is a valid
/// identifier, otherwise in an `@xml.value("...")` annotation next to a
/// sanitized name. Lowerings that need the original (e.g. proto enum constant
/// naming) read the annotation first, falling back to the name.
fn enum_value_from_raw(raw: &str, doc: Option<String>) -> EnumValue {
    if is_identifier(raw) {
        EnumValue {
            name: raw.to_string(),
            doc,
            annotations: vec![],
        }
    } else {
        EnumValue {
            name: sanitize_identifier(raw),
            doc,
            annotations: vec![Annotation::str("xml.value", raw)],
        }
    }
}

/// Count fields declared so far (both direct fields and choice fields).
/// Mirrors the sequential field numbering of the proto lowering, so
/// synthetic placeholder names stay stable.
fn count_fields(members: &[Member]) -> usize {
    members
        .iter()
        .map(|m| match m {
            Member::Field(_) => 1,
            Member::Choice(c) => c.fields.len(),
        })
        .sum()
}

// ---------------------------------------------------------------------------
// Transform context
// ---------------------------------------------------------------------------

/// Mutable context threaded through the transform.
struct TransformContext<'a> {
    registry: &'a TypeRegistry,
    current_ns: NamespaceUri,
    current_package: String,
    /// Package names of other schemas referenced by qualified type refs.
    imports: BTreeSet<String>,
    profile: SchemaProfile,
}

impl<'a> TransformContext<'a> {
    // -----------------------------------------------------------------------
    // Simple type -> IR enum
    // -----------------------------------------------------------------------

    /// Transform a simple type into an IR declaration.
    ///
    /// Enumeration variants become an [`EnumDecl`]. Pattern, range, and
    /// length restrictions become an [`Alias`] to the base type carrying the
    /// facets as annotations. Lists, unions, and plain restrictions map to
    /// their base primitive type at usage sites (no declaration).
    fn transform_simple_type(&mut self, st: &XsdSimpleType) -> Option<Decl> {
        match &st.content {
            SimpleTypeContent::Enumeration { variants, .. } => Some(Decl::Enum(self.build_enum(
                &st.name,
                variants,
                st.annotation.documentation.as_deref(),
            ))),
            SimpleTypeContent::Pattern { base, pattern } => {
                let annotations = vec![Annotation::str("pattern", pattern)];
                Some(Decl::Alias(self.build_alias(st, base, annotations)))
            }
            SimpleTypeContent::Range {
                base,
                min_inclusive,
                max_inclusive,
                min_exclusive,
                max_exclusive,
            } => {
                let mut annotations = Vec::new();
                let facets = [
                    ("minInclusive", min_inclusive),
                    ("maxInclusive", max_inclusive),
                    ("minExclusive", min_exclusive),
                    ("maxExclusive", max_exclusive),
                ];
                for (name, value) in facets {
                    if let Some(v) = value {
                        annotations.push(Annotation::new(name, vec![range_arg(v)]));
                    }
                }
                Some(Decl::Alias(self.build_alias(st, base, annotations)))
            }
            SimpleTypeContent::LengthRestriction {
                base,
                min_length,
                max_length,
                length,
            } => {
                let mut annotations = Vec::new();
                let facets = [
                    ("minLength", min_length),
                    ("maxLength", max_length),
                    ("length", length),
                ];
                for (name, value) in facets {
                    if let Some(n) = value {
                        annotations
                            .push(Annotation::new(name, vec![AnnotationValue::Int(*n as i64)]));
                    }
                }
                Some(Decl::Alias(self.build_alias(st, base, annotations)))
            }
            _ => None,
        }
    }

    /// Build an IR alias for a constrained simple type. The target is
    /// whatever the base type maps to (primitive or named type).
    fn build_alias(
        &mut self,
        st: &XsdSimpleType,
        base: &QName,
        annotations: Vec<Annotation>,
    ) -> Alias {
        Alias {
            name: st.name.clone(),
            doc: st.annotation.documentation.clone(),
            target: self.resolve_type_ref(base),
            annotations,
        }
    }

    /// Build an IR enum from enumeration variants.
    ///
    /// Values carry the raw XSD enumeration strings (no UNKNOWN injection,
    /// no numbering — writers add those).
    fn build_enum(
        &self,
        name: &str,
        variants: &[EnumVariant],
        documentation: Option<&str>,
    ) -> EnumDecl {
        let values = variants
            .iter()
            .map(|v| enum_value_from_raw(&v.value, v.annotation.documentation.clone()))
            .collect();

        EnumDecl {
            name: name.to_string(),
            doc: documentation.map(|s| s.to_string()),
            annotations: vec![],
            values,
        }
    }

    // -----------------------------------------------------------------------
    // Complex type -> IR record
    // -----------------------------------------------------------------------

    /// Transform a complex type into an IR record.
    fn transform_complex_type(&mut self, ct: &XsdComplexType, name: &str) -> Option<Record> {
        let mut members = Vec::new();

        match &ct.content {
            // Rule 1: complexContent/extension.
            // complexContent/restriction is treated the same way but with the
            // restricted content.
            ComplexTypeContent::ComplexExtension { base, compositor }
            | ComplexTypeContent::ComplexRestriction { base, compositor } => {
                self.handle_extension_base(base, &mut members);

                if let Some(comp) = compositor {
                    self.flatten_compositor(comp, &mut members);
                }
            }

            // simpleContent/extension - this should have been caught as a wrapper,
            // but if it wasn't (e.g., has custom attributes beyond SimpleObjectAttributeGroup),
            // emit a record with a `value` field.
            ComplexTypeContent::SimpleExtension { base }
            | ComplexTypeContent::SimpleRestriction { base } => {
                let ty = self.resolve_type_ref(base);
                members.push(Member::Field(Field {
                    name: "value".to_string(),
                    ty,
                    cardinality: Cardinality::Required,
                    doc: None,
                    annotations: vec![],
                }));
            }

            // Direct content (sequence/choice/all).
            ComplexTypeContent::Direct { compositor } => {
                if let Some(comp) = compositor {
                    self.flatten_compositor(comp, &mut members);
                }
            }

            ComplexTypeContent::Empty => {}
        }

        // Rule 11: attributes
        self.handle_attributes(&ct.attributes, &ct.attribute_group_refs, &mut members);

        // Rule 13: anyAttribute -> map<string, string>
        if ct.any_attribute.is_some() {
            members.push(Member::Field(Field {
                name: "extra_attributes".to_string(),
                ty: TypeRef::named(None, "map<string, string>"),
                cardinality: Cardinality::Required,
                doc: Some("Catch-all for ISM/NTK attributes.".to_string()),
                annotations: vec![],
            }));
        }

        Some(Record {
            name: name.to_string(),
            doc: ct.annotation.documentation.clone(),
            annotations: vec![],
            members,
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
    fn handle_extension_base(&mut self, base: &QName, members: &mut Vec<Member>) {
        if self.profile.should_inline_structures_base()
            && base.namespace.as_str() == STRUCTURES_NAMESPACE
        {
            // Inline the structures attributes.
            self.emit_structures_attributes(members);

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
                // Only emit a choice if there are known augmentations.
                if let Some(augmentations) = self.registry.get_augmentations(&aug_qname) {
                    if !augmentations.is_empty() {
                        let choice =
                            self.build_augmentation_choice(&to_snake_case(aug_name), augmentations);
                        members.push(Member::Choice(choice));
                    }
                }
            }
        } else {
            // Non-structures base: emit a composition field.
            let ty = self.resolve_type_ref(base);
            let field_name = to_snake_case(
                base.local_name
                    .strip_suffix("Type")
                    .unwrap_or(&base.local_name),
            );
            members.push(Member::Field(Field {
                name: field_name,
                ty,
                cardinality: Cardinality::Required,
                doc: Some(format!("Base type: {}", base.local_name)),
                annotations: vec![],
            }));
        }
    }

    /// Emit the standard NIEM structures attributes as fields.
    fn emit_structures_attributes(&self, members: &mut Vec<Member>) {
        let attrs = [
            ("structures_id", "A document-relative identifier."),
            ("structures_ref", "A document-relative reference."),
            ("structures_uri", "A URI for this object."),
            ("structures_metadata", "Metadata references."),
        ];

        for (name, doc) in &attrs {
            members.push(Member::Field(Field {
                name: name.to_string(),
                ty: TypeRef::Primitive(Primitive::String),
                cardinality: Cardinality::Optional,
                doc: Some(doc.to_string()),
                annotations: vec![],
            }));
        }
    }

    // -----------------------------------------------------------------------
    // Compositor flattening
    // -----------------------------------------------------------------------

    /// Flatten a compositor (sequence/choice/all) into record members.
    fn flatten_compositor(&mut self, comp: &Compositor, members: &mut Vec<Member>) {
        match comp.kind {
            // Rule 7: xs:choice -> choice member
            CompositorKind::Choice => {
                let fields_before = count_fields(members);
                let choice = self.build_choice(comp, fields_before);
                members.push(Member::Choice(choice));
            }

            // Rule 8: xs:sequence -> ordered fields
            CompositorKind::Sequence | CompositorKind::All => {
                for item in &comp.items {
                    match item {
                        CompositorItem::Element(elem) => {
                            if let Some(member) = self.transform_element(elem) {
                                members.push(member);
                            }
                        }
                        CompositorItem::Compositor(nested) => {
                            self.flatten_compositor(nested, members);
                        }
                    }
                }
            }
        }
    }

    /// Build a choice member from a choice compositor.
    ///
    /// `fields_before` is the number of fields already declared in the record,
    /// used only to name placeholder fields for nested compositors.
    fn build_choice(&mut self, comp: &Compositor, fields_before: usize) -> Choice {
        let mut choice_fields = Vec::new();

        // Try to derive a meaningful name from the choice items.
        let choice_name = derive_choice_name(&comp.items);

        for item in &comp.items {
            match item {
                CompositorItem::Element(elem) => {
                    let (name, ty) = self.element_name_and_type(elem);
                    choice_fields.push(Field {
                        name,
                        ty,
                        cardinality: Cardinality::Required,
                        doc: elem.annotation.documentation.clone(),
                        annotations: vec![],
                    });
                }
                CompositorItem::Compositor(_nested) => {
                    // Nested compositor inside choice - rare, emit a placeholder.
                    choice_fields.push(Field {
                        name: format!("choice_option_{}", fields_before + choice_fields.len() + 1),
                        ty: TypeRef::Primitive(Primitive::String),
                        cardinality: Cardinality::Required,
                        doc: Some("Nested compositor in choice.".to_string()),
                        annotations: vec![],
                    });
                }
            }
        }

        Choice {
            name: choice_name,
            doc: None,
            fields: choice_fields,
        }
    }

    // -----------------------------------------------------------------------
    // Element transform
    // -----------------------------------------------------------------------

    /// Transform a single element (either a reference or a declaration) into
    /// a field or a choice (for substitution groups / augmentation points).
    fn transform_element(&mut self, elem: &XsdElement) -> Option<Member> {
        // Resolve the element — could be a ref or a declaration.
        let resolved = self.resolve_element(elem);

        // Check if this element is abstract and heads a substitution group.
        let elem_qname = self.element_qname(elem);

        if let Some(ref qname) = elem_qname {
            // Rule 6: augmentation points (NIEM-specific)
            if self.profile.is_augmentation_point(qname) {
                if let Some(augmentations) = self.registry.get_augmentations(qname) {
                    if !augmentations.is_empty() {
                        let choice = self.build_augmentation_choice(
                            &to_snake_case(&qname.local_name),
                            augmentations,
                        );
                        return Some(Member::Choice(choice));
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
                        let choice = self.build_substitution_choice(
                            &to_snake_case(&qname.local_name),
                            members,
                            elem,
                        );
                        return Some(Member::Choice(choice));
                    }
                }
            }
        }

        // Regular element -> field.
        let (name, ty) = self.element_name_and_type(elem);
        let cardinality = element_cardinality(elem);

        Some(Member::Field(Field {
            name,
            ty,
            cardinality,
            doc: elem.annotation.documentation.clone(),
            annotations: vec![],
        }))
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

    /// Get the field name and IR type for an element.
    fn element_name_and_type(&mut self, elem: &XsdElement) -> (String, TypeRef) {
        if let Some(ref qname) = elem.element_ref {
            // Element reference: use the ref'd element's name and type.
            let field_name = to_snake_case(&qname.local_name);
            let ty = if let Some(resolved) = self.registry.resolve_element(qname) {
                if let Some(type_ref) = resolved.type_ref.clone() {
                    self.resolve_type_ref(&type_ref)
                } else {
                    // Element with anonymous type or no type.
                    TypeRef::Primitive(Primitive::String)
                }
            } else {
                TypeRef::Primitive(Primitive::String)
            };
            (field_name, ty)
        } else {
            // Inline element declaration.
            let field_name = to_snake_case(elem.name.as_deref().unwrap_or("unknown"));
            let ty = if let Some(ref type_ref) = elem.type_ref {
                self.resolve_type_ref(type_ref)
            } else {
                TypeRef::Primitive(Primitive::String)
            };
            (field_name, ty)
        }
    }

    // -----------------------------------------------------------------------
    // Substitution group / augmentation choices
    // -----------------------------------------------------------------------

    /// Build a choice for a substitution group.
    fn build_substitution_choice(
        &mut self,
        choice_name: &str,
        members: &[QName],
        parent_elem: &XsdElement,
    ) -> Choice {
        let mut choice_fields = Vec::new();

        for member in members {
            let field_name = to_snake_case(&member.local_name);
            let ty = self.resolve_element_type(member);

            choice_fields.push(Field {
                name: field_name,
                ty,
                cardinality: Cardinality::Required,
                doc: None,
                annotations: vec![],
            });
        }

        // If the parent element allows multiple occurrences, note it in docs
        // (a choice itself can't be repeated).
        let _ = parent_elem;

        Choice {
            name: choice_name.to_string(),
            doc: None,
            fields: choice_fields,
        }
    }

    /// Build a choice for an augmentation point.
    fn build_augmentation_choice(&mut self, choice_name: &str, augmentations: &[QName]) -> Choice {
        let mut choice_fields = Vec::new();

        for aug in augmentations {
            let field_name = to_snake_case(&aug.local_name);
            let ty = self.resolve_element_type(aug);

            choice_fields.push(Field {
                name: field_name,
                ty,
                cardinality: Cardinality::Required,
                doc: None,
                annotations: vec![],
            });
        }

        Choice {
            name: choice_name.to_string(),
            doc: None,
            fields: choice_fields,
        }
    }

    /// Resolve a global element's declared type, defaulting to string.
    fn resolve_element_type(&mut self, qname: &QName) -> TypeRef {
        if let Some(resolved) = self.registry.resolve_element(qname) {
            if let Some(type_ref) = resolved.type_ref.clone() {
                return self.resolve_type_ref(&type_ref);
            }
        }
        TypeRef::Primitive(Primitive::String)
    }

    // -----------------------------------------------------------------------
    // Attribute handling
    // -----------------------------------------------------------------------

    /// Handle attributes on a complex type.
    fn handle_attributes(
        &mut self,
        attrs: &[XsdAttribute],
        attr_group_refs: &[QName],
        members: &mut Vec<Member>,
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
                    self.emit_attribute_field(attr, members);
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
            self.emit_attribute_field(attr, members);
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

    /// Emit a single attribute as a field.
    fn emit_attribute_field(&mut self, attr: &XsdAttribute, members: &mut Vec<Member>) {
        let (name, ty) = if let Some(ref attr_ref) = attr.attribute_ref {
            let field_name = to_snake_case(&attr_ref.local_name);
            let ty = if let Some(ref type_ref) = attr.type_ref {
                self.resolve_type_ref(type_ref)
            } else {
                // Look up the global attribute for its type.
                self.resolve_global_attribute_type(attr_ref)
            };
            (field_name, ty)
        } else {
            let field_name = to_snake_case(attr.name.as_deref().unwrap_or("attr"));
            let ty = if let Some(ref type_ref) = attr.type_ref {
                self.resolve_type_ref(type_ref)
            } else {
                TypeRef::Primitive(Primitive::String)
            };
            (field_name, ty)
        };

        let cardinality = if attr.use_required {
            Cardinality::Required
        } else {
            Cardinality::Optional
        };

        members.push(Member::Field(Field {
            name,
            ty,
            cardinality,
            doc: attr.annotation.documentation.clone(),
            annotations: vec![],
        }));
    }

    /// Resolve the type of a global attribute by looking it up in the registry.
    fn resolve_global_attribute_type(&mut self, qname: &QName) -> TypeRef {
        let schema = self.registry.schemas.get(&qname.namespace);
        if let Some(schema) = schema {
            for attr in &schema.attributes {
                if attr.name.as_deref() == Some(qname.local_name.as_str()) {
                    if let Some(type_ref) = attr.type_ref.clone() {
                        return self.resolve_type_ref(&type_ref);
                    }
                }
            }
        }
        TypeRef::Primitive(Primitive::String)
    }

    // -----------------------------------------------------------------------
    // Type resolution
    // -----------------------------------------------------------------------

    /// Resolve a QName type reference to an IR [`TypeRef`].
    fn resolve_type_ref(&mut self, qname: &QName) -> TypeRef {
        type_ref_from_type_string(&self.resolve_type_name(qname))
    }

    /// Resolve a QName type reference to a proto-style type string.
    ///
    /// This handles:
    /// - XSD built-in types (xs:string, xs:int, etc.)
    /// - NIEM proxy types (niem-xs:string, niem-xs:double, etc.) -> Rule 4
    /// - NIEM wrapper types (Rule 3) -> collapse to the underlying enum/primitive
    /// - Regular named types -> fully-qualified type name
    fn resolve_type_name(&mut self, qname: &QName) -> String {
        // Rule 4: XSD built-in types.
        if qname.namespace.as_str() == XS_NAMESPACE {
            return xsd_builtin_to_proto(&qname.local_name);
        }

        // Rule 4: NIEM proxy types -> unwrap to builtins.
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

        // Rule 12: Check if it's a simple type.
        if let Some(st) = self.registry.resolve_simple_type(qname) {
            match &st.content {
                SimpleTypeContent::Enumeration { .. } => {
                    // Reference to an enum type.
                    return self.qualified_type_name(qname, &st.name);
                }
                // Constrained simple types are declared as aliases in their
                // defining schema — reference them by name (unqualified when
                // local, qualified with import tracking otherwise).
                SimpleTypeContent::Pattern { .. }
                | SimpleTypeContent::Range { .. }
                | SimpleTypeContent::LengthRestriction { .. } => {
                    if qname.namespace == self.current_ns {
                        return st.name.clone();
                    }
                    return self.qualified_type_name(qname, &st.name);
                }
                SimpleTypeContent::Restriction { base, .. } => {
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

    /// Build a fully-qualified type name and track the import.
    fn qualified_type_name(&mut self, qname: &QName, type_name: &str) -> String {
        let target_package = namespace_to_package(qname.namespace.as_str());
        self.maybe_add_import(&target_package);
        format!("{}.{}", target_package, type_name)
    }

    /// Add an import for a target package if it differs from the current package.
    fn maybe_add_import(&mut self, target_package: &str) {
        if target_package != self.current_package {
            self.imports.insert(target_package.to_string());
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

    /// Unwrap a NIEM wrapper to get the underlying type string.
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

/// Determine the cardinality of an element.
fn element_cardinality(elem: &XsdElement) -> Cardinality {
    // Rule 9: maxOccurs="unbounded" -> repeated
    if elem.max_occurs == Occurs::Unbounded {
        return Cardinality::Many;
    }
    if let Occurs::Count(max) = elem.max_occurs {
        if max > 1 {
            return Cardinality::Many;
        }
    }

    // Rule 10: minOccurs="0" -> optional
    if elem.min_occurs == Occurs::Count(0) {
        return Cardinality::Optional;
    }

    Cardinality::Required
}

/// Derive a name for a choice from its items.
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
// XSD -> type-string mapping
// ---------------------------------------------------------------------------

/// Map an XSD built-in type to a proto-style type string.
///
/// The result is converted to an IR [`TypeRef`] via
/// [`type_ref_from_type_string`]: bare primitives become
/// [`Primitive`]s (e.g. `"date"` -> `Primitive::Date`, which writers map
/// out as they see fit), while dotted names such as
/// `"google.protobuf.Timestamp"` become qualified named refs.
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
        "date" => "date".to_string(),
        "time" => "time".to_string(),
        "ID" | "IDREF" | "IDREFS" => "string".to_string(),
        _ => "string".to_string(), // safe fallback
    }
}

/// Map a NIEM proxy type (niem-xs:*) to a built-in type string.
///
/// niem-xs proxy types are just wrappers around xs: types with
/// SimpleObjectAttributeGroup. We map them to the same type
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
    use crate::readers::xsd::transform::profile::SchemaProfile;
    use crate::test_fixtures::build_test_registry;

    // -- IR inspection helpers ----------------------------------------------

    fn records(schema: &Schema) -> Vec<&Record> {
        schema
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::Record(r) => Some(r),
                _ => None,
            })
            .collect()
    }

    fn enums(schema: &Schema) -> Vec<&EnumDecl> {
        schema
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::Enum(e) => Some(e),
                _ => None,
            })
            .collect()
    }

    fn find_record<'a>(schema: &'a Schema, name: &str) -> Option<&'a Record> {
        records(schema).into_iter().find(|r| r.name == name)
    }

    fn direct_fields(record: &Record) -> Vec<&Field> {
        record
            .members
            .iter()
            .filter_map(|m| match m {
                Member::Field(f) => Some(f),
                _ => None,
            })
            .collect()
    }

    fn choices(record: &Record) -> Vec<&Choice> {
        record
            .members
            .iter()
            .filter_map(|m| match m {
                Member::Choice(c) => Some(c),
                _ => None,
            })
            .collect()
    }

    // -- Integration: transform a namespace ---------------------------------

    #[test]
    fn transform_vehicle_namespace() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/vehicle".to_string());

        let schema =
            transform_schema(&ns, &reg, SchemaProfile::Niem).expect("should produce a Schema");

        assert_eq!(schema.name, "example_com.schemas.vehicle");
        assert!(
            !records(&schema).is_empty(),
            "should have at least one record"
        );
        assert_eq!(
            schema.annotations,
            vec![Annotation::str(
                "xml.namespace",
                "http://example.com/schemas/vehicle"
            )]
        );
    }

    #[test]
    fn vehicle_type_has_expected_fields() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/vehicle".to_string());

        let schema = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        let veh = find_record(&schema, "VehicleType").expect("should have VehicleType record");

        // Should have structures attributes (id, ref, uri, metadata).
        let field_names: Vec<&str> = direct_fields(veh).iter().map(|f| f.name.as_str()).collect();
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

        // Should have audit_record field (many).
        let audit_field = direct_fields(veh)
            .into_iter()
            .find(|f| f.name == "audit_record")
            .expect("should have audit_record field");
        assert_eq!(
            audit_field.cardinality,
            Cardinality::Many,
            "audit_record should be many"
        );
    }

    #[test]
    fn inspection_report_type_has_substitution_choice() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/vehicle".to_string());

        let schema = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        // InspectionReportType references StatusCodeAbstract which has
        // a substitution group (StatusCode). This should produce a choice.
        let ir_rec = find_record(&schema, "InspectionReportType")
            .expect("should have InspectionReportType record");

        let choice_names: Vec<&str> = choices(ir_rec).iter().map(|c| c.name.as_str()).collect();
        assert!(
            !choice_names.is_empty(),
            "InspectionReportType should have a choice for StatusCodeAbstract substitution group, got: {choice_names:?}"
        );

        // The choice should contain "status_code" as one of its members.
        let status_choice = choices(ir_rec)
            .into_iter()
            .find(|c| c.name.contains("status_code"))
            .expect("should have a choice related to status_code");

        let field_names: Vec<&str> = status_choice
            .fields
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert!(
            field_names.contains(&"status_code"),
            "status choice should contain status_code, got: {field_names:?}"
        );
    }

    #[test]
    fn vehicle_type_augmentation_point_omitted_when_no_augmentations() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/vehicle".to_string());

        let schema = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        let veh = find_record(&schema, "VehicleType").expect("should have VehicleType record");

        // VehicleAugmentationPoint has no concrete augmentations
        // in the test dataset, so it should NOT appear as a field or choice.
        let has_aug_field = direct_fields(veh)
            .iter()
            .any(|f| f.name.contains("augmentation"));
        let has_aug_choice = choices(veh).iter().any(|c| c.name.contains("augmentation"));

        assert!(
            !has_aug_field && !has_aug_choice,
            "VehicleAugmentationPoint should be omitted when no augmentations exist"
        );
    }

    #[test]
    fn confidence_code_simple_type_becomes_enum() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/common-types".to_string());

        let schema = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        let confidence_enum = enums(&schema)
            .into_iter()
            .find(|e| e.name == "ConfidenceCodeSimpleType")
            .expect("should have ConfidenceCodeSimpleType enum");

        // Values carry the raw XSD enumeration strings: no synthetic value,
        // no prefixing (writers add those).
        let value_names: Vec<&str> = confidence_enum
            .values
            .iter()
            .map(|v| v.name.as_str())
            .collect();
        assert!(
            value_names.contains(&"HIGH"),
            "should have raw HIGH value, got: {value_names:?}"
        );
        assert!(
            confidence_enum
                .values
                .iter()
                .all(|v| v.annotations.is_empty()),
            "identifier-safe values need no xml.value annotation"
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

        let schema = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        // CapabilityConfidenceCodeType is a NIEM wrapper around
        // ConfidenceCodeSimpleType. It should NOT appear as a record.
        assert!(
            find_record(&schema, "CapabilityConfidenceCodeType").is_none(),
            "CapabilityConfidenceCodeType should be collapsed (not emitted as record)"
        );
    }

    #[test]
    fn composite_object_type_has_choice() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/core".to_string());

        let schema = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        let co_rec = find_record(&schema, "CompositeObjectType")
            .expect("should have CompositeObjectType record");

        // Should have a choice for the xs:choice.
        let co_choices = choices(co_rec);
        assert!(
            !co_choices.is_empty(),
            "CompositeObjectType should have at least one choice for the xs:choice"
        );

        // The choice should contain vehicle as one of the options.
        let choice_field_names: Vec<&str> = co_choices[0]
            .fields
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert!(
            choice_field_names.contains(&"vehicle"),
            "choice should contain vehicle, got: {choice_field_names:?}"
        );
    }

    #[test]
    fn structures_namespace_produces_no_records() {
        let reg = build_test_registry();
        let ns = NamespaceUri(STRUCTURES_NAMESPACE.to_string());

        let schema = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        // We skip structures types (they are inlined into extending types).
        assert!(
            records(&schema).is_empty(),
            "structures namespace should produce no records"
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
    fn imports_are_tracked_as_package_names() {
        let reg = build_test_registry();
        let ns = NamespaceUri("http://example.com/schemas/core".to_string());

        let schema = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();

        // core references veh:Vehicle, so it should import that package.
        assert!(
            schema
                .imports
                .contains(&"example_com.schemas.vehicle".to_string()),
            "should import vehicle package, got: {:?}",
            schema.imports
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
        assert_eq!(xsd_builtin_to_proto("date"), "date");
        assert_eq!(xsd_builtin_to_proto("time"), "time");
        assert_eq!(xsd_builtin_to_proto("ID"), "string");
        assert_eq!(xsd_builtin_to_proto("IDREF"), "string");
        assert_eq!(xsd_builtin_to_proto("IDREFS"), "string");
    }

    #[test]
    fn type_ref_from_type_string_maps_primitives_and_names() {
        assert_eq!(
            type_ref_from_type_string("double"),
            TypeRef::Primitive(Primitive::Float64)
        );
        assert_eq!(
            type_ref_from_type_string("float"),
            TypeRef::Primitive(Primitive::Float32)
        );
        assert_eq!(
            type_ref_from_type_string("string"),
            TypeRef::Primitive(Primitive::String)
        );
        assert_eq!(
            type_ref_from_type_string("date"),
            TypeRef::Primitive(Primitive::Date)
        );
        assert_eq!(
            type_ref_from_type_string("Person"),
            TypeRef::named(None, "Person")
        );
        assert_eq!(
            type_ref_from_type_string("a.b.C"),
            TypeRef::named(Some("a.b"), "C")
        );
    }

    #[test]
    fn enum_value_from_raw_preserves_identifier_values() {
        let v = enum_value_from_raw("HIGH", None);
        assert_eq!(v.name, "HIGH");
        assert!(v.annotations.is_empty());
    }

    #[test]
    fn enum_value_from_raw_sanitizes_and_annotates_non_identifiers() {
        let v = enum_value_from_raw("5.56 mm", None);
        assert_eq!(v.name, "_5_56_mm");
        assert_eq!(v.annotations, vec![Annotation::str("xml.value", "5.56 mm")]);
    }

    // -- Generic profile tests -----------------------------------------------
    //
    // These use small, self-contained inline XSD fixtures with synthetic
    // namespaces so they are independent of any real-world schema set.

    use crate::readers::xsd::parser::parse_schema;
    use crate::readers::xsd::resolver::build_type_registry as build_reg;
    use crate::test_fixtures::{NIEM_XS_XSD, STRUCTURES_XSD};
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

    fn aliases(schema: &Schema) -> Vec<&crate::ir::model::Alias> {
        schema
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::Alias(a) => Some(a),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn pattern_simple_type_becomes_alias_with_annotation() {
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema targetNamespace="http://example.com/pat"
            xmlns:p="http://example.com/pat"
            xmlns:xs="http://www.w3.org/2001/XMLSchema">
          <xs:simpleType name="SSNType">
            <xs:restriction base="xs:string">
              <xs:pattern value="\d{3}-\d{2}-\d{4}"/>
            </xs:restriction>
          </xs:simpleType>
          <xs:complexType name="PersonType">
            <xs:sequence>
              <xs:element name="Ssn" type="p:SSNType"/>
            </xs:sequence>
          </xs:complexType>
        </xs:schema>"#;

        let reg = generic_registry(&[xsd]);
        let ns = NamespaceUri("http://example.com/pat".to_string());
        let schema = transform_schema(&ns, &reg, SchemaProfile::Generic).unwrap();

        let alias = aliases(&schema)
            .into_iter()
            .find(|a| a.name == "SSNType")
            .expect("should have SSNType alias");
        assert_eq!(alias.target, TypeRef::Primitive(Primitive::String));
        assert_eq!(
            alias.annotations,
            vec![Annotation::str("pattern", r"\d{3}-\d{2}-\d{4}")]
        );

        let rec = find_record(&schema, "PersonType").expect("should have PersonType record");
        let ssn = direct_fields(rec)
            .into_iter()
            .find(|f| f.name == "ssn")
            .expect("should have ssn field");
        assert_eq!(ssn.ty, TypeRef::named(None, "SSNType"));
    }

    #[test]
    fn range_simple_type_becomes_alias_with_int_annotations() {
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema targetNamespace="http://example.com/rng"
            xmlns:r="http://example.com/rng"
            xmlns:xs="http://www.w3.org/2001/XMLSchema">
          <xs:simpleType name="PercentType">
            <xs:restriction base="xs:int">
              <xs:minInclusive value="0"/>
              <xs:maxInclusive value="100"/>
            </xs:restriction>
          </xs:simpleType>
          <xs:complexType name="ScoreType">
            <xs:sequence>
              <xs:element name="Percent" type="r:PercentType"/>
            </xs:sequence>
          </xs:complexType>
        </xs:schema>"#;

        let reg = generic_registry(&[xsd]);
        let ns = NamespaceUri("http://example.com/rng".to_string());
        let schema = transform_schema(&ns, &reg, SchemaProfile::Generic).unwrap();

        let alias = aliases(&schema)
            .into_iter()
            .find(|a| a.name == "PercentType")
            .expect("should have PercentType alias");
        assert_eq!(alias.target, TypeRef::Primitive(Primitive::Int32));
        assert_eq!(
            alias.annotations,
            vec![
                Annotation::new("minInclusive", vec![AnnotationValue::Int(0)]),
                Annotation::new("maxInclusive", vec![AnnotationValue::Int(100)]),
            ]
        );

        let rec = find_record(&schema, "ScoreType").expect("should have ScoreType record");
        let pct = direct_fields(rec)
            .into_iter()
            .find(|f| f.name == "percent")
            .expect("should have percent field");
        assert_eq!(pct.ty, TypeRef::named(None, "PercentType"));
    }

    #[test]
    fn length_restriction_simple_type_becomes_alias_with_length_annotations() {
        let xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema targetNamespace="http://example.com/len"
            xmlns:l="http://example.com/len"
            xmlns:xs="http://www.w3.org/2001/XMLSchema">
          <xs:simpleType name="CodeType">
            <xs:restriction base="xs:string">
              <xs:minLength value="2"/>
              <xs:maxLength value="8"/>
            </xs:restriction>
          </xs:simpleType>
          <xs:complexType name="ItemType">
            <xs:sequence>
              <xs:element name="Code" type="l:CodeType"/>
            </xs:sequence>
          </xs:complexType>
        </xs:schema>"#;

        let reg = generic_registry(&[xsd]);
        let ns = NamespaceUri("http://example.com/len".to_string());
        let schema = transform_schema(&ns, &reg, SchemaProfile::Generic).unwrap();

        let alias = aliases(&schema)
            .into_iter()
            .find(|a| a.name == "CodeType")
            .expect("should have CodeType alias");
        assert_eq!(alias.target, TypeRef::Primitive(Primitive::String));
        assert_eq!(
            alias.annotations,
            vec![
                Annotation::new("minLength", vec![AnnotationValue::Int(2)]),
                Annotation::new("maxLength", vec![AnnotationValue::Int(8)]),
            ]
        );

        let rec = find_record(&schema, "ItemType").expect("should have ItemType record");
        let code = direct_fields(rec)
            .into_iter()
            .find(|f| f.name == "code")
            .expect("should have code field");
        assert_eq!(code.ty, TypeRef::named(None, "CodeType"));
    }

    #[test]
    fn cross_namespace_facet_simple_type_is_qualified_and_imported() {
        let common_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema targetNamespace="http://example.com/xcommon"
            xmlns:c="http://example.com/xcommon"
            xmlns:xs="http://www.w3.org/2001/XMLSchema">
          <xs:simpleType name="UuidType">
            <xs:restriction base="xs:token">
              <xs:pattern value="[0-9a-f]{8}"/>
            </xs:restriction>
          </xs:simpleType>
        </xs:schema>"#;
        let user_xsd = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xs:schema targetNamespace="http://example.com/xuser"
            xmlns:c="http://example.com/xcommon"
            xmlns:xs="http://www.w3.org/2001/XMLSchema">
          <xs:import namespace="http://example.com/xcommon"/>
          <xs:complexType name="RefType">
            <xs:sequence>
              <xs:element name="Id" type="c:UuidType"/>
            </xs:sequence>
          </xs:complexType>
        </xs:schema>"#;

        let reg = generic_registry(&[common_xsd, user_xsd]);

        // The alias lives in the defining schema.
        let common_ns = NamespaceUri("http://example.com/xcommon".to_string());
        let common = transform_schema(&common_ns, &reg, SchemaProfile::Generic).unwrap();
        assert!(
            aliases(&common).iter().any(|a| a.name == "UuidType"),
            "common schema should declare the UuidType alias"
        );

        // The referencing schema uses a qualified ref and tracks the import.
        let user_ns = NamespaceUri("http://example.com/xuser".to_string());
        let user = transform_schema(&user_ns, &reg, SchemaProfile::Generic).unwrap();
        let rec = find_record(&user, "RefType").expect("should have RefType record");
        let id = direct_fields(rec)
            .into_iter()
            .find(|f| f.name == "id")
            .expect("should have id field");
        assert_eq!(
            id.ty,
            TypeRef::named(Some("example_com.xcommon"), "UuidType")
        );
        assert!(
            user.imports.contains(&"example_com.xcommon".to_string()),
            "should import the alias's package, got: {:?}",
            user.imports
        );
    }

    #[test]
    fn generic_structures_namespace_produces_records() {
        let reg = generic_registry(&[STRUCTURES_XSD]);
        let ns = NamespaceUri(STRUCTURES_NAMESPACE.to_string());

        let schema = transform_schema(&ns, &reg, SchemaProfile::Generic).unwrap();

        assert!(
            !records(&schema).is_empty(),
            "generic profile should emit records for structures namespace"
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
        let schema = transform_schema(&ns, &reg, SchemaProfile::Generic).unwrap();

        let wrapper_rec = find_record(&schema, "ColorCodeType")
            .expect("generic profile should emit ColorCodeType as a record");

        let field_names: Vec<&str> = direct_fields(wrapper_rec)
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert!(
            field_names.contains(&"value"),
            "wrapper record should have a 'value' field, got: {field_names:?}"
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
        let schema = transform_schema(&ns, &reg, SchemaProfile::Generic).unwrap();

        let rec = find_record(&schema, "VehicleType").expect("should have VehicleType record");

        let field_names: Vec<&str> = direct_fields(rec).iter().map(|f| f.name.as_str()).collect();
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

        let schema_niem = transform_schema(&ns, &reg, SchemaProfile::Niem).unwrap();
        let schema_generic = transform_schema(&ns, &reg, SchemaProfile::Generic).unwrap();

        // In NIEM mode the augmentation point produces a special choice.
        let niem_rec = find_record(&schema_niem, "ThingType").unwrap();
        let niem_has_aug_choice = choices(niem_rec)
            .iter()
            .any(|c| c.name.contains("augmentation"));
        assert!(
            niem_has_aug_choice,
            "NIEM profile should produce augmentation choice"
        );

        // In generic mode the same element is treated as a regular
        // substitution group (since it is abstract).
        let generic_rec = find_record(&schema_generic, "ThingType").unwrap();
        let generic_choice = choices(generic_rec)
            .into_iter()
            .find(|c| c.name.contains("thing_augmentation_point"));
        assert!(
            generic_choice.is_some(),
            "generic profile should handle augmentation point as a regular substitution group choice"
        );
    }

    #[test]
    fn generic_niem_proxy_types_not_collapsed() {
        // A type referencing niem-xs:string — in generic mode it should NOT be
        // collapsed to a plain string.
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
        let schema = transform_schema(&ns, &reg, SchemaProfile::Generic).unwrap();

        let rec = find_record(&schema, "ItemType").unwrap();

        let label_field = direct_fields(rec)
            .into_iter()
            .find(|f| f.name == "label")
            .expect("should have label field");

        assert_ne!(
            label_field.ty,
            TypeRef::Primitive(Primitive::String),
            "generic profile should not collapse niem-xs:string to plain string"
        );
        assert!(
            label_field.ty.to_string().contains("niem"),
            "generic profile should reference niem proxy type: {}",
            label_field.ty
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
        let schema = transform_schema(&ns, &reg, SchemaProfile::Generic).unwrap();

        let rec = find_record(&schema, "WidgetType").expect("should have WidgetType record");

        let field_names: Vec<&str> = direct_fields(rec).iter().map(|f| f.name.as_str()).collect();
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
