use std::collections::BTreeSet;
use std::path::PathBuf;

mod isolation_common;
use isolation_common::{dependency_packages, members_under, package_name};

#[test]
fn subcrates_only_depend_on_crates_members() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let subcrates = members_under(&root, "subcrates");
    assert!(!subcrates.is_empty(), "no subcrates were found under subcrates/");
    let crate_names: BTreeSet<String> = members_under(&root, "crates")
        .iter()
        .map(|d| package_name(d))
        .collect();
    let subcrate_names: BTreeSet<String> =
        subcrates.iter().map(|d| package_name(d)).collect();
    let root_name = package_name(&root);
    let mut violations = Vec::new();
    for subcrate in &subcrates {
        let own = package_name(subcrate);
        for (section, dep) in dependency_packages(subcrate) {
            if dep == own {
                continue;
            }
            let is_workspace_member =
                crate_names.contains(&dep) || subcrate_names.contains(&dep) || dep == root_name;
            if is_workspace_member && !crate_names.contains(&dep) {
                violations.push(format!("{own} [{section}] -> {dep}"));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "subcrates may depend on crates under crates/ but not on each other or the root app; wire them together in the root crate instead:\n{}",
        violations.join("\n")
    );
}
