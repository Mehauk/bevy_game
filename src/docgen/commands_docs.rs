use bevy::prelude::*;

use crate::utils::registry::commands::{CommandEntry, CommandRegistry};

use super::write_doc;

pub(super) fn write(registry: Res<CommandRegistry>) {
    write_doc("commands.md", &format_markdown(&registry.commands));
}

fn namespace_of(name: &str) -> &str {
    match name.split_once('>') {
        Some((namespace, _)) => namespace,
        None => "general",
    }
}

pub(super) fn format_markdown(commands: &[CommandEntry]) -> String {
    let mut out = String::from(
        "# Commands\n\n\
         Generated at startup by the `docgen` plugin. Do not edit by hand.\n\n\
         The last `>`-separated segment of a name works as a shorthand when it is unique.\n\n",
    );

    let mut current: Option<&str> = None;
    for command in commands {
        let namespace = namespace_of(&command.name);
        if current != Some(namespace) {
            if current.is_some() {
                out.push('\n');
            }
            out.push_str(&format!("## {namespace}\n\n"));
            current = Some(namespace);
        }
        out.push_str(&format!("- `{}` — {}\n", command.name, command.description));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::registry::commands::CommandAppExt;

    fn registered_app(names: &[&'static str]) -> App {
        let mut app = App::new();
        app.init_resource::<CommandRegistry>();
        for name in names {
            app.register_command(name.to_string(), "does nothing", || {});
        }
        app
    }

    #[test]
    fn empty_registry_still_emits_the_header() {
        let out = format_markdown(&[]);
        assert!(out.starts_with("# Commands\n"));
        assert!(!out.contains("## "));
    }

    #[test]
    fn commands_group_under_their_namespace() {
        let app = registered_app(&["camera>angle_camera", "dev>toggle_wireframe"]);
        let registry = app.world().resource::<CommandRegistry>();
        let out = format_markdown(&registry.commands);
        assert!(out.contains("## camera\n\n- `camera>angle_camera` — does nothing\n"));
        assert!(out.contains("## dev\n\n- `dev>toggle_wireframe` — does nothing\n"));
    }

    #[test]
    fn same_namespace_emits_one_section() {
        let app = registered_app(&["dev>toggle_fps_overlay", "dev>toggle_wireframe"]);
        let registry = app.world().resource::<CommandRegistry>();
        let out = format_markdown(&registry.commands);
        assert_eq!(out.matches("## dev").count(), 1);
    }

    #[test]
    fn names_without_a_namespace_fall_back_to_general() {
        let app = registered_app(&["bare_command"]);
        let registry = app.world().resource::<CommandRegistry>();
        let out = format_markdown(&registry.commands);
        assert!(out.contains("## general\n\n- `bare_command` — does nothing\n"));
    }
}
