//! Integration tests for the full XSD-to-proto pipeline.
//!
//! These tests parse all `schema/core-xsd/` files, build a registry,
//! transform to proto IR, emit to a temp directory, and verify the output.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Once;

use schemata::proto::emitter::emit_proto_file;
use schemata::resolver::build_type_registry;
use schemata::transform::naming::package_to_import_path;
use schemata::transform::transform_schema;
use schemata::xsd::model::NamespaceUri;
use schemata::xsd::parser::parse_schema;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Recursively collect all `.xsd` files under a directory.
fn collect_xsd_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_xsd_files(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("xsd") {
                out.push(path);
            }
        }
    }
}

/// A test output directory that gets created once and reused across tests.
/// We use a fixed path under target/ so cargo clean removes it.
fn output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/integration-proto")
}

static PIPELINE_INIT: Once = Once::new();

/// Run the full pipeline once (idempotent via Once) and return the result.
/// Returns `Vec<(relative_path, content)>`.
fn ensure_pipeline_run() -> Vec<(String, String)> {
    let out_dir = output_dir();

    PIPELINE_INIT.call_once(|| {
        // Clean previous runs.
        let _ = fs::remove_dir_all(&out_dir);
        run_pipeline_to_dir(&out_dir);
    });

    // Read back from disk.
    let mut results = Vec::new();
    collect_proto_results(&out_dir, &out_dir, &mut results);
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

fn collect_proto_results(base: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_proto_results(base, &path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("proto") {
                let rel = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                let content = fs::read_to_string(&path).unwrap();
                out.push((rel, content));
            }
        }
    }
}

fn run_pipeline_to_dir(out_dir: &Path) {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let xsd_root = base.join("schema/core-xsd");

    // Stage 1: Discover.
    let mut xsd_paths = Vec::new();
    collect_xsd_files(&xsd_root, &mut xsd_paths);
    xsd_paths.sort();
    assert!(!xsd_paths.is_empty(), "no XSD files found");

    // Stage 2: Parse.
    let parsed: Vec<_> = xsd_paths
        .into_iter()
        .filter_map(|path| {
            let xml = fs::read_to_string(&path).ok()?;
            parse_schema(&xml, &path).ok().map(|s| (s, path))
        })
        .collect();
    assert!(!parsed.is_empty(), "no schemas were parsed");

    // Stage 3: Build registry.
    let registry = build_type_registry(parsed);
    assert!(
        !registry.schemas.is_empty(),
        "registry should have namespaces"
    );

    // Stage 4: Transform.
    let namespaces: Vec<NamespaceUri> = registry.schemas.keys().cloned().collect();
    let mut proto_files = Vec::new();
    for ns in &namespaces {
        if let Some(pf) = transform_schema(ns, &registry) {
            if !pf.messages.is_empty() || !pf.enums.is_empty() {
                proto_files.push(pf);
            }
        }
    }
    assert!(!proto_files.is_empty(), "should generate proto files");

    // Stage 5: Emit.
    fs::create_dir_all(out_dir).expect("failed to create output directory");

    for pf in &proto_files {
        let rel_path = package_to_import_path(&pf.package);
        let file_path = out_dir.join(&rel_path);

        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).expect("failed to create dir");
        }

        let content = emit_proto_file(pf);
        fs::write(&file_path, &content).expect("failed to write proto file");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn full_pipeline_produces_proto_files() {
    let results = ensure_pipeline_run();
    assert!(
        results.len() > 5,
        "expected at least 5 proto files, got {}",
        results.len()
    );
}

#[test]
fn key_output_files_exist() {
    let results = ensure_pipeline_run();
    let paths: HashSet<&str> = results.iter().map(|(p, _)| p.as_str()).collect();

    let expected = [
        "uc2/battlefield_entity/v4.proto",
        "uc2/uc2_types/v4.proto",
        "uc2/capability/v4.proto",
        "uc2/uc2_core/v4.proto",
        "niem/niem_core/v5.proto",
    ];

    for expected_path in &expected {
        assert!(
            paths.contains(expected_path),
            "expected output file {} not found in {:?}",
            expected_path,
            paths
        );
    }
}

#[test]
fn battlefield_entity_proto_contains_expected_messages() {
    let results = ensure_pipeline_run();
    let be_content = results
        .iter()
        .find(|(p, _)| p == "uc2/battlefield_entity/v4.proto")
        .map(|(_, c)| c.as_str())
        .expect("battlefield_entity proto not found");

    // Verify syntax.
    assert!(
        be_content.contains("syntax = \"proto3\";"),
        "should contain proto3 syntax declaration"
    );

    // Verify package.
    assert!(
        be_content.contains("package uc2.battlefield_entity.v4;"),
        "should contain correct package"
    );

    // Verify key messages.
    assert!(
        be_content.contains("message BattlefieldEntityType"),
        "should contain BattlefieldEntityType message"
    );
    assert!(
        be_content.contains("message EntityDetailsType"),
        "should contain EntityDetailsType message"
    );

    // Verify oneof declarations exist.
    assert!(
        be_content.contains("oneof "),
        "should contain oneof declarations"
    );
}

#[test]
fn uc2_types_proto_has_enums() {
    let results = ensure_pipeline_run();
    let types_content = results
        .iter()
        .find(|(p, _)| p == "uc2/uc2_types/v4.proto")
        .map(|(_, c)| c.as_str())
        .expect("uc2_types proto not found");

    assert!(
        types_content.contains("enum ConfidenceCodeSimpleType"),
        "should contain ConfidenceCodeSimpleType enum"
    );

    // Verify the enum has an UNKNOWN value at number 0.
    assert!(
        types_content.contains("CONFIDENCE_CODE_UNKNOWN = 0;"),
        "should have UNKNOWN = 0 value"
    );
}

#[test]
fn niem_core_proto_has_messages() {
    let results = ensure_pipeline_run();
    let nc_content = results
        .iter()
        .find(|(p, _)| p == "niem/niem_core/v5.proto")
        .map(|(_, c)| c.as_str())
        .expect("niem_core proto not found");

    assert!(
        nc_content.contains("syntax = \"proto3\";"),
        "should have proto3 syntax"
    );
    assert!(
        nc_content.contains("package niem.niem_core.v5;"),
        "should have correct package"
    );
    // niem-core has many complex types that become messages.
    assert!(
        nc_content.contains("message "),
        "should contain message definitions"
    );
}

#[test]
fn capability_proto_has_content() {
    let results = ensure_pipeline_run();
    let cap_content = results
        .iter()
        .find(|(p, _)| p == "uc2/capability/v4.proto")
        .map(|(_, c)| c.as_str())
        .expect("capability proto not found");

    assert!(
        cap_content.contains("package uc2.capability.v4;"),
        "should have correct package"
    );
    assert!(
        cap_content.contains("message ") || cap_content.contains("enum "),
        "should contain messages or enums"
    );
}

#[test]
fn empty_proto_files_are_not_written() {
    let results = ensure_pipeline_run();

    // Verify that structures (which produces only abstract types) is not in
    // the output, since it would be empty.
    let has_structures = results.iter().any(|(p, _)| p == "niem/structures/v5.proto");
    assert!(
        !has_structures,
        "structures.proto should not be generated (all types are abstract/skipped)"
    );

    // Verify that niem-xs proxy (all wrapper types collapsed) is not in output.
    let has_niem_xs = results
        .iter()
        .any(|(p, _)| p == "niem/proxy/niem_xs/v5.proto");
    assert!(
        !has_niem_xs,
        "niem-xs proxy proto should not be generated (all types are NIEM wrappers)"
    );
}

#[test]
fn all_proto_files_have_valid_syntax_and_package() {
    let results = ensure_pipeline_run();

    for (rel_path, content) in &results {
        assert!(
            content.contains("syntax = \"proto3\";"),
            "{} should have proto3 syntax",
            rel_path
        );
        assert!(
            content.contains("package "),
            "{} should have a package declaration",
            rel_path
        );
    }
}

#[test]
fn battlefield_entity_snapshot() {
    let results = ensure_pipeline_run();
    let be_content = results
        .iter()
        .find(|(p, _)| p == "uc2/battlefield_entity/v4.proto")
        .map(|(_, c)| c.as_str())
        .expect("battlefield_entity proto not found");

    // Snapshot-style assertions: verify specific structural elements.
    assert!(
        be_content.contains("syntax = \"proto3\";"),
        "snapshot: must have proto3 syntax"
    );
    assert!(
        be_content.contains("message BattlefieldEntityType {"),
        "snapshot: must contain BattlefieldEntityType message"
    );
    assert!(
        be_content.contains("message EntityDetailsType {"),
        "snapshot: must contain EntityDetailsType message"
    );

    // Should have oneof declarations.
    let oneof_count = be_content.matches("oneof ").count();
    assert!(
        oneof_count >= 2,
        "snapshot: should have at least 2 oneof declarations, found {}",
        oneof_count
    );

    // Should have imports.
    assert!(
        be_content.contains("import \""),
        "snapshot: should have import statements"
    );

    // Should have the correct package.
    assert!(
        be_content.contains("package uc2.battlefield_entity.v4;"),
        "snapshot: should have correct package"
    );
}
