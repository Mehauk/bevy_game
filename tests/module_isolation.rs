use std::fs;
use std::path::{Path, PathBuf};

const FEATURES: [&str; 13] = [
    "camera",
    "character",
    "commands",
    "debug_views",
    "dev",
    "distance_field",
    "docgen",
    "input",
    "profiler",
    "ripples",
    "screenshot",
    "state",
    "terrain",
];

const EXEMPT: [&str; 2] = ["integration", "main"];

fn source_files(dir: &Path, into: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
    {
        let path = entry.expect("read dir entry").path();
        if path.is_dir() {
            source_files(&path, into);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            into.push(path);
        }
    }
}

fn owner(src: &Path, file: &Path) -> String {
    let relative = file
        .strip_prefix(src)
        .unwrap_or_else(|e| panic!("{} is outside {}: {e}", file.display(), src.display()));
    let first = relative
        .components()
        .next()
        .expect("a source file has at least one path component")
        .as_os_str()
        .to_str()
        .expect("source paths are utf-8")
        .to_string();
    if relative.components().count() == 1 {
        return first.trim_end_matches(".rs").to_string();
    }
    first
}

fn feature_imports(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let mut rest = line;
        while let Some(at) = rest.find("crate::") {
            rest = &rest[at + "crate::".len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if FEATURES.contains(&name.as_str()) {
                found.push((index + 1, name));
            }
        }
    }
    found
}

fn app_sources() -> (PathBuf, Vec<PathBuf>) {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    source_files(&src, &mut files);
    files.sort();
    (src, files)
}

#[test]
fn the_scanner_only_reports_feature_imports() {
    assert_eq!(
        feature_imports("use crate::terrain::Sea;"),
        vec![(1, "terrain".to_string())]
    );
    assert!(
        feature_imports("use crate::utils::registry::commands::command;").is_empty(),
        "the kernel and the registry are not features"
    );
}

#[test]
fn features_do_not_import_each_other() {
    let (src, files) = app_sources();
    let mut violations = Vec::new();
    for file in &files {
        let module = owner(&src, file);
        if EXEMPT.contains(&module.as_str()) {
            continue;
        }
        let text = fs::read_to_string(file).expect("source files are utf-8");
        for (line, imported) in feature_imports(&text) {
            if imported != module {
                let relative = file.strip_prefix(&src).unwrap();
                violations.push(format!(
                    "{relative}:{line} imports crate::{imported}",
                    relative = relative.display()
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "feature modules must not import one another; wire them together in src/integration/ instead:\n{}",
        violations.join("\n")
    );
}

#[test]
fn utils_imports_no_feature() {
    let (src, files) = app_sources();
    let mut violations = Vec::new();
    for file in &files {
        if owner(&src, file) != "utils" {
            continue;
        }
        let text = fs::read_to_string(file).expect("source files are utf-8");
        for (line, imported) in feature_imports(&text) {
            let relative = file.strip_prefix(&src).unwrap();
            violations.push(format!(
                "{relative}:{line} imports crate::{imported}",
                relative = relative.display()
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "src/utils is a strict kernel and must not import any feature:\n{}",
        violations.join("\n")
    );
}

#[test]
fn utils_declares_no_plugins() {
    let (src, files) = app_sources();
    let mut violations = Vec::new();
    for file in &files {
        if owner(&src, file) != "utils" {
            continue;
        }
        let text = fs::read_to_string(file).expect("source files are utf-8");
        for (line, _) in text
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains("impl Plugin for"))
        {
            let relative = file.strip_prefix(&src).unwrap();
            violations.push(format!(
                "{relative}:{line} declares a plugin",
                relative = relative.display()
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "src/utils is a strict kernel and must not declare plugins:\n{}",
        violations.join("\n")
    );
}
