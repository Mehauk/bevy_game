use bevy::prelude::*;
use bevy_mod_scripting::bindings::AppScriptNamespaceMembers;

use super::write_doc;

pub(super) fn write(members: Res<AppScriptNamespaceMembers>) {
    let mut out = String::from(
        "# Lua Hooks\n\n\
         Generated at startup by the `docgen` plugin. Do not edit by hand.\n\n",
    );

    let members = members.read();
    for (namespace, contents) in members.iter() {
        if contents.functions.is_empty() {
            continue;
        }
        out.push_str(&format!("## {namespace}\n\n"));
        for name in contents.functions.keys() {
            out.push_str(&format!("- `{namespace}.{name}()`\n"));
        }
        out.push('\n');
    }

    write_doc("lua-only-hooks.md", &out);
}
