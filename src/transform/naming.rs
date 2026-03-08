//! Naming conventions for transforming XSD names into Protocol Buffer names.
//!
//! Covers:
//! - Namespace URI to proto package name
//! - PascalCase / kebab-case to snake_case field names
//! - Enum value naming with prefix
//! - Proto package to import path

// ---------------------------------------------------------------------------
// Namespace URI -> proto package
// ---------------------------------------------------------------------------

/// Convert a namespace URI to a proto package name.
///
/// # Examples
/// ```text
/// "http://www.cto.mil/FNC3/UC2/Language/4/battlefieldEntity" -> "uc2.battlefield_entity.v4"
/// "http://release.niem.gov/niem/structures/5.0/"             -> "niem.structures.v5"
/// "http://release.niem.gov/niem/proxy/niem-xs/5.0/"          -> "niem.proxy.niem_xs.v5"
/// "http://release.niem.gov/niem/niem-core/5.0/"              -> "niem.niem_core.v5"
/// "http://release.niem.gov/niem/domains/militaryOperations/5.1/" -> "niem.domains.military_operations.v5"
/// ```
pub fn namespace_to_package(uri: &str) -> String {
    // Strip trailing slash for uniform handling.
    let uri_trimmed = uri.trim_end_matches('/');

    if let Some(pkg) = try_uc2_namespace(uri_trimmed) {
        return pkg;
    }
    if let Some(pkg) = try_niem_namespace(uri_trimmed) {
        return pkg;
    }

    // Fallback: use the host + path, converting to dotted snake_case.
    fallback_package(uri_trimmed)
}

/// Try to parse a UC2 namespace (cto.mil).
///
/// Pattern: `http://www.cto.mil/FNC3/UC2/Language/{version}/{name}`
fn try_uc2_namespace(uri: &str) -> Option<String> {
    // Look for the marker "/UC2/Language/" in the path.
    let marker = "/UC2/Language/";
    let idx = uri.find(marker)?;
    let after = &uri[idx + marker.len()..]; // e.g. "4/battlefieldEntity"

    let mut parts: Vec<&str> = after.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }

    // First segment is the version number.
    let version_str = parts.remove(0);
    let version = extract_major_version(version_str);

    // Remaining segments are the package path.
    let pkg_segments: Vec<String> = parts.iter().map(|s| to_snake_case(s)).collect();

    let mut result = String::from("uc2");
    for seg in &pkg_segments {
        result.push('.');
        result.push_str(seg);
    }
    result.push('.');
    result.push('v');
    result.push_str(&version);

    Some(result)
}

/// Try to parse a NIEM namespace (release.niem.gov).
///
/// Pattern: `http://release.niem.gov/niem/{path...}/{version}/`
fn try_niem_namespace(uri: &str) -> Option<String> {
    let marker = "release.niem.gov/niem/";
    let idx = uri.find(marker)?;
    let after = &uri[idx + marker.len()..]; // e.g. "structures/5.0" or "proxy/niem-xs/5.0"

    let parts: Vec<&str> = after.split('/').filter(|s| !s.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }

    // The last segment that looks like a version (digits.digits or just digits) is the version.
    // Everything before it is the package path.
    let mut version_idx = None;
    for (i, part) in parts.iter().enumerate().rev() {
        if looks_like_version(part) {
            version_idx = Some(i);
            break;
        }
    }

    let version_idx = version_idx?;
    let version = extract_major_version(parts[version_idx]);
    let path_parts = &parts[..version_idx];

    let pkg_segments: Vec<String> = path_parts.iter().map(|s| to_snake_case(s)).collect();

    let mut result = String::from("niem");
    for seg in &pkg_segments {
        result.push('.');
        result.push_str(seg);
    }
    result.push('.');
    result.push('v');
    result.push_str(&version);

    Some(result)
}

/// Check if a string segment looks like a version (e.g., "5.0", "5.1", "3.0", "5").
fn looks_like_version(s: &str) -> bool {
    let first_char = s.chars().next();
    matches!(first_char, Some('0'..='9')) && s.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Extract the major version from a version string like "5.0" -> "5".
fn extract_major_version(s: &str) -> String {
    s.split('.').next().unwrap_or(s).to_string()
}

/// Fallback package name for unknown namespace URIs.
fn fallback_package(uri: &str) -> String {
    // Strip the scheme.
    let without_scheme = uri
        .strip_prefix("http://")
        .or_else(|| uri.strip_prefix("https://"))
        .unwrap_or(uri);

    let parts: Vec<String> = without_scheme
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| {
            // Also split on dots for the host portion
            s.replace('.', "_")
        })
        .map(|s| to_snake_case(&s))
        .collect();

    parts.join(".")
}

// ---------------------------------------------------------------------------
// Name transformations
// ---------------------------------------------------------------------------

/// Convert a PascalCase, camelCase, or kebab-case string to snake_case.
///
/// # Examples
/// ```text
/// "BattlefieldEntity"        -> "battlefield_entity"
/// "niem-xs"                  -> "niem_xs"
/// "niem-core"                -> "niem_core"
/// "militaryOperations"       -> "military_operations"
/// "IFFData"                  -> "iff_data"
/// "WGS84LocationType"        -> "wgs84_location_type"
/// "LINK16SourceTrackNumber"  -> "link16_source_track_number"
/// ```
pub fn to_snake_case(s: &str) -> String {
    // First, replace hyphens with underscores.
    let s = s.replace('-', "_");

    let mut result = String::with_capacity(s.len() + 4);
    let chars: Vec<char> = s.chars().collect();

    for i in 0..chars.len() {
        let c = chars[i];

        if c == '_' {
            result.push('_');
            continue;
        }

        if c.is_ascii_uppercase() {
            if i > 0 {
                let prev = chars[i - 1];
                // Don't insert underscore after an existing underscore.
                if prev == '_' {
                    result.push(c.to_ascii_lowercase());
                    continue;
                }
                // Transition from lowercase/digit to uppercase -> insert underscore.
                if prev.is_ascii_lowercase() || prev.is_ascii_digit() {
                    result.push('_');
                }
                // Transition within an uppercase run: insert underscore before the
                // last uppercase if the next char is lowercase.
                // e.g., "IFFData" -> i=3 (D), prev=F, next=a => "iff_data"
                else if prev.is_ascii_uppercase() {
                    if let Some(&next) = chars.get(i + 1) {
                        if next.is_ascii_lowercase() {
                            result.push('_');
                        }
                    }
                }
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }

    result
}

/// Convert a type name (PascalCase) to a proto field name (snake_case).
///
/// This is the same as [`to_snake_case`] but is provided as a semantic alias.
pub fn type_name_to_field_name(name: &str) -> String {
    to_snake_case(name)
}

/// Convert an enum value to a proto enum constant name.
///
/// Proto3 requires enum constant names to be prefixed with the enum name in
/// UPPER_SNAKE_CASE. For example, enum `ConfidenceCode` with value `HIGH`
/// becomes `CONFIDENCE_CODE_HIGH`.
///
/// The `enum_name` should be the simple type name (e.g., `ConfidenceCodeSimpleType`),
/// from which we strip the `SimpleType` suffix before converting.
pub fn enum_value_name(enum_name: &str, value: &str) -> String {
    // Strip "SimpleType" suffix if present.
    let base = enum_name.strip_suffix("SimpleType").unwrap_or(enum_name);

    let prefix = to_snake_case(base).to_ascii_uppercase();
    let val_upper = sanitize_enum_value(value).to_ascii_uppercase();

    format!("{prefix}_{val_upper}")
}

/// Sanitize an enum value string so it's a valid proto identifier.
///
/// Replaces non-alphanumeric characters with underscores and ensures
/// it doesn't start with a digit.
fn sanitize_enum_value(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for c in value.chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c);
        } else {
            result.push('_');
        }
    }
    // If starts with a digit, prefix with underscore.
    if result.starts_with(|c: char| c.is_ascii_digit()) {
        result.insert(0, '_');
    }
    result
}

/// Convert an enum name to the "unknown" zero-value constant name.
///
/// For enum `ConfidenceCodeSimpleType`, the unknown value is
/// `CONFIDENCE_CODE_UNKNOWN`.
pub fn enum_unknown_name(enum_name: &str) -> String {
    enum_value_name(enum_name, "UNKNOWN")
}

// ---------------------------------------------------------------------------
// Proto package -> import path
// ---------------------------------------------------------------------------

/// Convert a proto package name to an import path.
///
/// Package `niem.structures.v5` -> `niem/structures/v5.proto`
pub fn package_to_import_path(package: &str) -> String {
    let path = package.replace('.', "/");
    format!("{path}.proto")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- namespace_to_package -----------------------------------------------

    #[test]
    fn uc2_battlefield_entity_namespace() {
        assert_eq!(
            namespace_to_package("http://www.cto.mil/FNC3/UC2/Language/4/battlefieldEntity"),
            "uc2.battlefield_entity.v4"
        );
    }

    #[test]
    fn uc2_types_namespace() {
        assert_eq!(
            namespace_to_package("http://www.cto.mil/FNC3/UC2/Language/4/uc2-types"),
            "uc2.uc2_types.v4"
        );
    }

    #[test]
    fn niem_structures_namespace() {
        assert_eq!(
            namespace_to_package("http://release.niem.gov/niem/structures/5.0/"),
            "niem.structures.v5"
        );
    }

    #[test]
    fn niem_proxy_namespace() {
        assert_eq!(
            namespace_to_package("http://release.niem.gov/niem/proxy/niem-xs/5.0/"),
            "niem.proxy.niem_xs.v5"
        );
    }

    #[test]
    fn niem_core_namespace() {
        assert_eq!(
            namespace_to_package("http://release.niem.gov/niem/niem-core/5.0/"),
            "niem.niem_core.v5"
        );
    }

    #[test]
    fn niem_military_operations_namespace() {
        assert_eq!(
            namespace_to_package("http://release.niem.gov/niem/domains/militaryOperations/5.1/"),
            "niem.domains.military_operations.v5"
        );
    }

    #[test]
    fn niem_codes_nga_namespace() {
        assert_eq!(
            namespace_to_package("http://release.niem.gov/niem/codes/nga/5.0/"),
            "niem.codes.nga.v5"
        );
    }

    // -- to_snake_case ------------------------------------------------------

    #[test]
    fn snake_case_pascal() {
        assert_eq!(to_snake_case("BattlefieldEntity"), "battlefield_entity");
    }

    #[test]
    fn snake_case_kebab() {
        assert_eq!(to_snake_case("niem-xs"), "niem_xs");
    }

    #[test]
    fn snake_case_camel() {
        assert_eq!(to_snake_case("militaryOperations"), "military_operations");
    }

    #[test]
    fn snake_case_acronym_then_word() {
        assert_eq!(to_snake_case("IFFData"), "iff_data");
    }

    #[test]
    fn snake_case_mixed_acronym_digits() {
        assert_eq!(to_snake_case("WGS84LocationType"), "wgs84_location_type");
    }

    #[test]
    fn snake_case_already_snake() {
        assert_eq!(to_snake_case("already_snake"), "already_snake");
    }

    // -- enum_value_name ----------------------------------------------------

    #[test]
    fn enum_value_high() {
        assert_eq!(
            enum_value_name("ConfidenceCodeSimpleType", "HIGH"),
            "CONFIDENCE_CODE_HIGH"
        );
    }

    #[test]
    fn enum_value_unknown() {
        assert_eq!(
            enum_value_name("ConfidenceCodeSimpleType", "UNKNOWN"),
            "CONFIDENCE_CODE_UNKNOWN"
        );
    }

    #[test]
    fn enum_unknown_name_test() {
        assert_eq!(
            enum_unknown_name("ConfidenceCodeSimpleType"),
            "CONFIDENCE_CODE_UNKNOWN"
        );
    }

    // -- package_to_import_path ---------------------------------------------

    #[test]
    fn import_path_niem_structures() {
        assert_eq!(
            package_to_import_path("niem.structures.v5"),
            "niem/structures/v5.proto"
        );
    }

    #[test]
    fn import_path_uc2() {
        assert_eq!(
            package_to_import_path("uc2.battlefield_entity.v4"),
            "uc2/battlefield_entity/v4.proto"
        );
    }
}
