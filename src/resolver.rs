//! Resolver that builds a [`TypeRegistry`] from parsed XSD schemas.
//!
//! The registry indexes all global elements, complex types, simple types,
//! substitution groups, augmentation points, and namespace dependencies
//! so that downstream transforms can look up any component by its [`QName`].

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::xsd::model::*;

// ---------------------------------------------------------------------------
// TypeRegistry
// ---------------------------------------------------------------------------

/// A global index of all components across every parsed XSD schema.
///
/// Built by [`build_type_registry`], which processes a list of
/// `(XsdSchema, PathBuf)` pairs (one per parsed file).
#[derive(Debug)]
pub struct TypeRegistry {
    /// All parsed schemas indexed by their target namespace.
    pub schemas: HashMap<NamespaceUri, XsdSchema>,

    /// Global element index: QName → element declaration.
    pub elements: HashMap<QName, XsdElement>,

    /// Global complex type index: QName → complex type definition.
    pub complex_types: HashMap<QName, XsdComplexType>,

    /// Global simple type index: QName → simple type definition.
    pub simple_types: HashMap<QName, XsdSimpleType>,

    /// Substitution groups: abstract head element → concrete member elements.
    pub substitution_groups: HashMap<QName, Vec<QName>>,

    /// Augmentation points: augmentation-point element → augmenting element refs.
    ///
    /// An augmentation point is an abstract element whose name ends with
    /// `"AugmentationPoint"`. Concrete elements that declare
    /// `substitutionGroup` pointing to such an element are collected here.
    pub augmentation_points: HashMap<QName, Vec<QName>>,

    /// Dependency graph: namespace URI → set of imported namespace URIs.
    pub dependency_graph: HashMap<NamespaceUri, HashSet<NamespaceUri>>,

    /// Source file paths for each namespace (for diagnostics).
    pub source_paths: HashMap<NamespaceUri, PathBuf>,
}

impl TypeRegistry {
    /// Look up a global element by qualified name.
    pub fn resolve_element(&self, qname: &QName) -> Option<&XsdElement> {
        self.elements.get(qname)
    }

    /// Look up a global complex type by qualified name.
    pub fn resolve_complex_type(&self, qname: &QName) -> Option<&XsdComplexType> {
        self.complex_types.get(qname)
    }

    /// Look up a global simple type by qualified name.
    pub fn resolve_simple_type(&self, qname: &QName) -> Option<&XsdSimpleType> {
        self.simple_types.get(qname)
    }

    /// Get the concrete members of a substitution group headed by `head`.
    pub fn get_substitution_group(&self, head: &QName) -> Option<&[QName]> {
        self.substitution_groups.get(head).map(|v| v.as_slice())
    }

    /// Get the augmenting elements for a given augmentation point.
    pub fn get_augmentations(&self, point: &QName) -> Option<&[QName]> {
        self.augmentation_points.get(point).map(|v| v.as_slice())
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Build a [`TypeRegistry`] from all parsed schemas.
///
/// Each entry in `schemas` is a `(XsdSchema, PathBuf)` pair — the parsed
/// schema and the file it came from. Schemas without a `target_namespace`
/// (e.g., aggregation schemas with no `targetNamespace`) are skipped.
pub fn build_type_registry(schemas: Vec<(XsdSchema, PathBuf)>) -> TypeRegistry {
    let mut registry = TypeRegistry {
        schemas: HashMap::new(),
        elements: HashMap::new(),
        complex_types: HashMap::new(),
        simple_types: HashMap::new(),
        substitution_groups: HashMap::new(),
        augmentation_points: HashMap::new(),
        dependency_graph: HashMap::new(),
        source_paths: HashMap::new(),
    };

    // ------------------------------------------------------------------
    // 1. Index all schemas by target namespace, skipping those without one.
    // ------------------------------------------------------------------
    let mut indexed: Vec<(NamespaceUri, XsdSchema, PathBuf)> = Vec::new();

    for (schema, path) in schemas {
        if let Some(ref ns) = schema.target_namespace {
            indexed.push((ns.clone(), schema, path));
        }
        // else: aggregation schema with no targetNamespace — skip
    }

    // ------------------------------------------------------------------
    // 2–3. Index global elements, complex types, and simple types.
    // ------------------------------------------------------------------
    for (ns, schema, path) in &indexed {
        // Elements
        for elem in &schema.elements {
            if let Some(ref name) = elem.name {
                let qname = QName {
                    namespace: ns.clone(),
                    local_name: name.clone(),
                };
                registry.elements.insert(qname, elem.clone());
            }
        }

        // Complex types
        for ct in &schema.complex_types {
            if let Some(ref name) = ct.name {
                let qname = QName {
                    namespace: ns.clone(),
                    local_name: name.clone(),
                };
                registry.complex_types.insert(qname, ct.clone());
            }
        }

        // Simple types
        for st in &schema.simple_types {
            let qname = QName {
                namespace: ns.clone(),
                local_name: st.name.clone(),
            };
            registry.simple_types.insert(qname, st.clone());
        }

        registry.source_paths.insert(ns.clone(), path.clone());
    }

    // ------------------------------------------------------------------
    // 4–5. Build substitution groups and augmentation points.
    //
    // For every global element that declares a substitutionGroup, add it
    // as a member of that head element's group. If the head element's name
    // ends with "AugmentationPoint", also record it in augmentation_points.
    // ------------------------------------------------------------------
    for (ns, schema, _path) in &indexed {
        for elem in &schema.elements {
            if let (Some(ref name), Some(ref head)) = (&elem.name, &elem.substitution_group) {
                let member_qname = QName {
                    namespace: ns.clone(),
                    local_name: name.clone(),
                };

                // Add to substitution_groups
                registry
                    .substitution_groups
                    .entry(head.clone())
                    .or_default()
                    .push(member_qname.clone());

                // If the head is an augmentation point, also record it there
                if head.local_name.ends_with("AugmentationPoint") {
                    registry
                        .augmentation_points
                        .entry(head.clone())
                        .or_default()
                        .push(member_qname);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // 6. Build dependency graph from imports.
    // ------------------------------------------------------------------
    for (ns, schema, _path) in &indexed {
        let mut deps = HashSet::new();
        for import in &schema.imports {
            if let Some(ref imported_ns) = import.namespace {
                deps.insert(imported_ns.clone());
            }
        }
        registry.dependency_graph.insert(ns.clone(), deps);
    }

    // ------------------------------------------------------------------
    // Store schemas last (consumes the vec entries).
    // ------------------------------------------------------------------
    for (ns, schema, _path) in indexed {
        registry.schemas.insert(ns, schema);
    }

    registry
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::build_test_registry;

    #[test]
    fn registry_has_key_namespaces() {
        let reg = build_test_registry();

        let expected_ns = [
            "http://release.niem.gov/niem/structures/5.0/",
            "http://release.niem.gov/niem/proxy/niem-xs/5.0/",
            "http://example.com/schemas/common-types",
            "http://example.com/schemas/vehicle",
            "http://example.com/schemas/capability",
            "http://release.niem.gov/niem/niem-core/5.0/",
        ];

        for ns_str in &expected_ns {
            let ns = NamespaceUri(ns_str.to_string());
            assert!(
                reg.schemas.contains_key(&ns),
                "registry should contain namespace {ns_str}"
            );
        }
    }

    #[test]
    fn registry_has_vehicle_type() {
        let reg = build_test_registry();

        let qname = QName {
            namespace: NamespaceUri("http://example.com/schemas/vehicle".to_string()),
            local_name: "VehicleType".to_string(),
        };
        assert!(
            reg.resolve_complex_type(&qname).is_some(),
            "VehicleType should exist in complex_types"
        );
    }

    #[test]
    fn feature_abstract_substitution_group() {
        let reg = build_test_registry();

        let head = QName {
            namespace: NamespaceUri("http://example.com/schemas/capability".to_string()),
            local_name: "FeatureAbstract".to_string(),
        };

        let members = reg
            .get_substitution_group(&head)
            .expect("FeatureAbstract should have a substitution group");

        let member_names: HashSet<&str> = members.iter().map(|q| q.local_name.as_str()).collect();

        assert!(
            member_names.contains("NetworkFeature"),
            "should contain NetworkFeature, got: {member_names:?}"
        );
        assert!(
            member_names.contains("SensorFeature"),
            "should contain SensorFeature, got: {member_names:?}"
        );
        assert!(
            member_names.contains("ActuatorFeature"),
            "should contain ActuatorFeature, got: {member_names:?}"
        );
    }

    #[test]
    fn augmentation_points_populated() {
        let reg = build_test_registry();

        // nc:LocationAugmentationPoint should have mo:LocationAugmentation
        let aug_point = QName {
            namespace: NamespaceUri("http://release.niem.gov/niem/niem-core/5.0/".to_string()),
            local_name: "LocationAugmentationPoint".to_string(),
        };

        let augmentations = reg
            .get_augmentations(&aug_point)
            .expect("LocationAugmentationPoint should have augmentations");

        let names: Vec<&str> = augmentations
            .iter()
            .map(|q| q.local_name.as_str())
            .collect();

        assert!(
            names.contains(&"LocationAugmentation"),
            "should contain LocationAugmentation, got: {names:?}"
        );
    }

    #[test]
    fn dependency_graph_has_imports() {
        let reg = build_test_registry();

        let veh_ns = NamespaceUri("http://example.com/schemas/vehicle".to_string());

        let deps = reg
            .dependency_graph
            .get(&veh_ns)
            .expect("vehicle should be in dependency graph");

        // vehicle imports structures and capability (among others)
        let structures_ns =
            NamespaceUri("http://release.niem.gov/niem/structures/5.0/".to_string());
        let capability_ns = NamespaceUri("http://example.com/schemas/capability".to_string());

        assert!(
            deps.contains(&structures_ns),
            "vehicle should import structures"
        );
        assert!(
            deps.contains(&capability_ns),
            "vehicle should import capability"
        );
    }

    #[test]
    fn resolve_element_works() {
        let reg = build_test_registry();

        let qname = QName {
            namespace: NamespaceUri("http://example.com/schemas/vehicle".to_string()),
            local_name: "Vehicle".to_string(),
        };

        let elem = reg
            .resolve_element(&qname)
            .expect("Vehicle element should be resolvable");

        assert_eq!(elem.name.as_deref(), Some("Vehicle"));
    }

    #[test]
    fn resolve_simple_type_works() {
        let reg = build_test_registry();

        let qname = QName {
            namespace: NamespaceUri("http://example.com/schemas/common-types".to_string()),
            local_name: "ConfidenceCodeSimpleType".to_string(),
        };

        let st = reg
            .resolve_simple_type(&qname)
            .expect("ConfidenceCodeSimpleType should be resolvable");

        assert_eq!(st.name, "ConfidenceCodeSimpleType");
    }

    #[test]
    fn schemas_without_target_namespace_are_skipped() {
        let reg = build_test_registry();

        // No schema should have a None-like namespace key; aggregation schemas
        // without a targetNamespace should be excluded.
        for ns in reg.schemas.keys() {
            assert!(
                !ns.as_str().is_empty(),
                "registry should not contain schemas with empty namespace"
            );
        }
    }

    #[test]
    fn substitution_group_members_have_correct_namespace() {
        let reg = build_test_registry();

        let head = QName {
            namespace: NamespaceUri("http://example.com/schemas/capability".to_string()),
            local_name: "FeatureAbstract".to_string(),
        };

        let members = reg
            .get_substitution_group(&head)
            .expect("FeatureAbstract should have a substitution group");

        // All members should be in the capability namespace
        let cap_ns = "http://example.com/schemas/capability";
        for member in members {
            assert_eq!(
                member.namespace.as_str(),
                cap_ns,
                "member {} should be in capability namespace",
                member.local_name
            );
        }
    }
}
