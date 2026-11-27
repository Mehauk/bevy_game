use std::fs;
use std::path::{Path, PathBuf};
use toml::Value;

const DEP_SECTIONS: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

pub fn parse_manifest(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let table: toml::Table =
        toml::from_str(&text).unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()));
    Value::Table(table)
}

pub fn package_name(member_dir: &Path) -> String {
    parse_manifest(&member_dir.join("Cargo.toml"))
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or_else(|| panic!("{} has no package name", member_dir.display()))
        .to_string()
}

pub fn member_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![root.to_path_buf()];
    let members = parse_manifest(&root.join("Cargo.toml"))
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();
    for member in members {
        let Some(pattern) = member.as_str() else { continue };
        if let Some(glob_dir) = pattern.strip_suffix("/*") {
            let parent = root.join(glob_dir);
            let entries = fs::read_dir(&parent).unwrap_or_else(|e| panic!("cannot read {}: {e}", parent.display()));
            for entry in entries {
                let dir = entry.expect("read dir entry").path();
                if dir.join("Cargo.toml").is_file() {
                    dirs.push(dir);
                }
            }
        } else {
            dirs.push(root.join(pattern));
        }
    }
    dirs
}

pub fn members_under(root: &Path, group: &str) -> Vec<PathBuf> {
    let group_dir = root.join(group);
    member_dirs(root)
        .into_iter()
        .filter(|dir| dir != root && dir.starts_with(&group_dir))
        .collect()
}

pub fn dependency_packages(member_dir: &Path) -> Vec<(String, String)> {
    let manifest = parse_manifest(&member_dir.join("Cargo.toml"));
    let mut sections: Vec<(String, &Value)> = Vec::new();
    for section in DEP_SECTIONS {
        if let Some(deps) = manifest.get(section) {
            sections.push((section.to_string(), deps));
        }
    }
    if let Some(targets) = manifest.get("target").and_then(|t| t.as_table()) {
        for (cfg, tables) in targets {
            for section in DEP_SECTIONS {
                if let Some(deps) = tables.get(section) {
                    sections.push((format!("target.{cfg}.{section}"), deps));
                }
            }
        }
    }
    let mut deps = Vec::new();
    for (label, table) in sections {
        let Some(entries) = table.as_table() else { continue };
        for (key, value) in entries {
            let package = value.get("package").and_then(|p| p.as_str()).unwrap_or(key);
            deps.push((label.clone(), package.to_string()));
        }
    }
    deps
}
