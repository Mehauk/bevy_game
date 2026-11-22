use bevy::prelude::*;

use crate::utils::registry::commands::{CommandEntry, CommandRegistry};

use super::write_doc;

pub(super) fn write(registry: Res<CommandRegistry>) {
    write_doc("commands.md", &render(&registry));
}

pub(super) fn render(registry: &CommandRegistry) -> String {
    let mut out = String::from(
        "# Console Commands\n\n\
         Generated at startup by the `docgen` plugin. Do not edit by hand.\n\n\
         Type a name in the in-game console, or call it from Lua with \
         `commands.run(\"name\", ...)`. Arguments follow the name.\n\n",
    );

    let mut entries: Vec<&CommandEntry> = registry.commands.iter().collect();
    entries.sort_by(|left, right| left.name.cmp(&right.name));

    let mut groups: Vec<(&str, Vec<&CommandEntry>)> = Vec::new();
    for entry in entries {
        let group = group_of(&entry.name);
        match groups.last_mut() {
            Some((last, list)) if *last == group => list.push(entry),
            _ => groups.push((group, vec![entry])),
        }
    }

    if groups.is_empty() {
        out.push_str("_No commands are registered._\n");
        return out;
    }

    for (group, list) in groups {
        out.push_str(&format!("## {group}\n\n"));
        for entry in list {
            out.push_str(&format!("- `{}` — {}\n", entry.name, entry.description));
        }
        out.push('\n');
    }

    out
}

fn group_of(name: &str) -> &str {
    match name.find('>') {
        Some(at) => &name[..at],
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::registry::commands::CommandAppExt;

    fn render_with(entries: &[(&str, &'static str)]) -> String {
        let mut app = App::new();
        app.init_resource::<CommandRegistry>();
        for (name, description) in entries {
            app.register_command(name.to_string(), description, || {});
        }
        render(app.world().resource::<CommandRegistry>())
    }

    fn sample_command() {}

    #[test]
    fn every_registered_command_is_listed_with_its_description() {
        let text = render_with(&[
            ("terrain>find_feature", "Find a named feature"),
            ("utils>help", "List commands"),
        ]);
        assert!(text.contains("`terrain>find_feature`"));
        assert!(text.contains("Find a named feature"));
        assert!(text.contains("`utils>help`"));
        assert!(text.contains("List commands"));
    }

    #[test]
    fn commands_are_grouped_by_namespace_and_sorted_within_group() {
        let text = render_with(&[
            ("terrain>world_summary", "Second terrain command"),
            ("camera>focus", "Camera command"),
            ("terrain>find_feature", "First terrain command"),
        ]);
        let camera_at = text.find("## camera").expect("camera group");
        let terrain_at = text.find("## terrain").expect("terrain group");
        assert!(
            camera_at < terrain_at,
            "groups must sort alphabetically:\n{text}"
        );
        let find_at = text
            .find("terrain>find_feature")
            .expect("find_feature listed");
        let summary_at = text
            .find("terrain>world_summary")
            .expect("world_summary listed");
        assert!(find_at < summary_at, "commands must sort by name:\n{text}");
        assert_eq!(
            text.matches("## terrain").count(),
            1,
            "a namespace must open exactly one section:\n{text}"
        );
    }

    #[test]
    fn a_command_registered_through_the_macro_reaches_the_doc() {
        let mut app = App::new();
        app.init_resource::<CommandRegistry>();
        crate::command!(&mut app, "Registered through the macro", sample_command);
        let text = render(app.world().resource::<CommandRegistry>());
        assert!(
            text.contains("sample_command") && text.contains("Registered through the macro"),
            "macro-registered commands must appear in the doc:\n{text}"
        );
    }

    #[test]
    fn an_empty_registry_still_renders_a_document() {
        let text = render(&CommandRegistry::default());
        assert!(text.starts_with("# Console Commands"));
        assert!(text.contains("No commands are registered"));
    }
}
