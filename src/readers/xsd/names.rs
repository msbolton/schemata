//! Utilities for working with XML qualified names, namespace prefixes,
//! and name transformations.

use crate::readers::xsd::model::{NamespaceUri, QName};
use std::collections::HashMap;

/// Well-known XML Schema namespace.
pub const XS_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema";

/// Well-known NIEM structures namespace.
pub const STRUCTURES_NAMESPACE: &str = "http://release.niem.gov/niem/structures/5.0/";

/// A mapping from namespace prefixes to namespace URIs, built from the
/// `xmlns:` declarations on an `xs:schema` element.
#[derive(Debug, Clone, Default)]
pub struct NamespaceMap {
    prefix_to_uri: HashMap<String, NamespaceUri>,
}

impl NamespaceMap {
    /// Create a new, empty namespace map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a prefix-to-URI binding.
    pub fn insert(&mut self, prefix: impl Into<String>, uri: impl Into<String>) {
        self.prefix_to_uri
            .insert(prefix.into(), NamespaceUri(uri.into()));
    }

    /// Resolve a prefixed name like `"structures:ObjectType"` into a [`QName`].
    ///
    /// If the name has no prefix (no `:`), the `default_namespace` is used.
    /// Returns `None` if the prefix is not found in the map.
    pub fn resolve(
        &self,
        prefixed_name: &str,
        default_namespace: Option<&NamespaceUri>,
    ) -> Option<QName> {
        if let Some((prefix, local)) = prefixed_name.split_once(':') {
            let ns = self.prefix_to_uri.get(prefix)?;
            Some(QName {
                namespace: ns.clone(),
                local_name: local.to_string(),
            })
        } else {
            // No prefix -- use default namespace (typically the target namespace
            // or the XSD namespace for built-in types).
            let ns = default_namespace?.clone();
            Some(QName {
                namespace: ns,
                local_name: prefixed_name.to_string(),
            })
        }
    }

    /// Look up the URI for a given prefix.
    pub fn get_uri(&self, prefix: &str) -> Option<&NamespaceUri> {
        self.prefix_to_uri.get(prefix)
    }

    /// Iterate over all (prefix, uri) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &NamespaceUri)> {
        self.prefix_to_uri.iter().map(|(k, v)| (k.as_str(), v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_prefixed_name() {
        let mut map = NamespaceMap::new();
        map.insert("structures", STRUCTURES_NAMESPACE);
        map.insert("xs", XS_NAMESPACE);

        let qname = map.resolve("structures:ObjectType", None).unwrap();
        assert_eq!(qname.namespace.as_str(), STRUCTURES_NAMESPACE);
        assert_eq!(qname.local_name, "ObjectType");
    }

    #[test]
    fn resolve_unprefixed_with_default() {
        let map = NamespaceMap::new();
        let default_ns = NamespaceUri("http://example.com/default".to_string());

        let qname = map.resolve("MyElement", Some(&default_ns)).unwrap();
        assert_eq!(qname.namespace.as_str(), "http://example.com/default");
        assert_eq!(qname.local_name, "MyElement");
    }

    #[test]
    fn resolve_unknown_prefix_returns_none() {
        let map = NamespaceMap::new();
        assert!(map.resolve("unknown:Foo", None).is_none());
    }

    #[test]
    fn resolve_unprefixed_without_default_returns_none() {
        let map = NamespaceMap::new();
        assert!(map.resolve("Foo", None).is_none());
    }
}
