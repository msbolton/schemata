use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Recursively collect files with `ext` under `dir`, relative paths, sorted.
fn collect(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    fn walk(root: &Path, dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
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

#[test]
fn xsd_to_proto_matches_golden() {
    let out = tempdir();
    run_convert(&[
        "convert",
        "--input",
        "schemas",
        "--output",
        out.to_str().unwrap(),
    ]);
    assert_dirs_equal(Path::new("tests/golden/proto"), &out, "proto");
    fs::remove_dir_all(&out).ok();
}

fn tempdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("schemata-golden-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}
