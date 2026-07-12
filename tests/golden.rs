use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Recursively collect files with `ext` under `dir`, relative paths, sorted.
fn collect(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(root: &Path, dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
        let entries =
            fs::read_dir(dir).unwrap_or_else(|e| panic!("reading dir {}: {e}", dir.display()));
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(root, &p, ext, out);
            } else if p.extension().and_then(|e| e.to_str()) == Some(ext) {
                out.push(p.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    walk(dir, dir, ext, &mut out);
    out.sort();
    out
}

fn run_convert(args: &[&str]) {
    let status = Command::new(env!("CARGO_BIN_EXE_schemata"))
        .args(args)
        .status()
        .expect("failed to run schemata");
    assert!(status.success(), "schemata convert failed");
}

/// Compare every file in `expected_dir` against `actual_dir`, byte for byte.
fn assert_dirs_equal(expected_dir: &Path, actual_dir: &Path, ext: &str) {
    let expected = collect(expected_dir, ext);
    let actual = collect(actual_dir, ext);
    assert_eq!(expected, actual, "file sets differ");
    for rel in &expected {
        let want = fs::read_to_string(expected_dir.join(rel)).unwrap();
        let got = fs::read_to_string(actual_dir.join(rel)).unwrap();
        assert_eq!(want, got, "content differs for {}", rel.display());
    }
}

/// Removes the wrapped directory on drop, even if the test panics.
struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn tempdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("schemata-golden-{}-{name}", std::process::id()));
    fs::remove_dir_all(&dir).ok();
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn xsd_to_proto_matches_golden() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let input = manifest_dir.join("schemas");
    let golden = manifest_dir.join("tests/golden/proto");

    let out = tempdir("xsd_to_proto");
    let _guard = TempDirGuard(out.clone());
    run_convert(&[
        "convert",
        "--input",
        input.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]);
    assert_dirs_equal(&golden, &out, "proto");
}

#[test]
fn xsd_to_schemata_matches_golden() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let input = manifest_dir.join("schemas");
    let golden = manifest_dir.join("tests/golden/schemata");

    let out = tempdir("xsd_to_schemata");
    let _guard = TempDirGuard(out.clone());
    run_convert(&[
        "convert",
        "--input",
        input.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--to",
        "schemata",
    ]);
    assert_dirs_equal(&golden, &out, "schemata");
}

#[test]
fn schemata_to_proto_matches_proto_golden() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let input = manifest_dir.join("tests/golden/schemata");
    let golden = manifest_dir.join("tests/golden/proto");

    let out = tempdir("schemata_to_proto");
    let _guard = TempDirGuard(out.clone());
    run_convert(&[
        "convert",
        "--input",
        input.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--to",
        "proto",
    ]);
    assert_dirs_equal(&golden, &out, "proto");
}
