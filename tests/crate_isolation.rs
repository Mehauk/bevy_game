use std::collections::BTreeSet;
use std::path::PathBuf;

mod isolation_common;
use isolation_common::{dependency_packages, member_dirs, members_under, package_name};

#[test]
fn crates_do_not_depend_on_workspace_members() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let names: BTreeSet<String> = member_dirs(&root).iter().map(|d| package_name(d)).collect();
    let crates = members_under(&root, "crates");
    let mut violations = Vec::new();
    for member in &crates {
        let own = package_name(member);
        for (section, dep) in dependency_packages(member) {
            if dep != own && names.contains(&dep) {
                violations.push(format!("{own} [{section}] -> {dep}"));
            }
        }
    }
    assert!(!crates.is_empty(), "no crates were found under crates/");
    assert!(
        violations.is_empty(),
        "crates under crates/ must not depend on other workspace crates; wire them together in the root crate instead:\n{}",
        violations.join("\n")
    );
}
