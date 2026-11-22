use bevy::prelude::*;
use bevy::time::TimeSystems;

use avian3d::prelude::LinearVelocity;

use crate::character::{CAPSULE_HEIGHT, CAPSULE_RADIUS, Character, GroundState};
use crate::state::PlayState;
use crate::utils::registry::commands::{CommandHistory, RunCommand};

pub const DECISION_LOG_LIMIT: usize = 256;

#[derive(Resource, Reflect)]
#[reflect(Resource)]
pub struct PlaytestClock {
    pub step_frames: u32,
    pub auto_pause: bool,
    pub speed: f32,
}

impl Default for PlaytestClock {
    fn default() -> Self {
        Self {
            step_frames: 0,
            auto_pause: false,
            speed: 1.0,
        }
    }
}

#[derive(Resource, Reflect, Default)]
#[reflect(Resource)]
pub struct DebugVisuals {
    pub enabled: bool,
}

#[derive(Resource, Reflect, Default)]
#[reflect(Resource)]
pub struct AgentConsole {
    pub request: String,
    pub last_output: String,
    pub pending: bool,
}

#[derive(Reflect, Clone, Debug)]
pub struct DecisionRecord {
    pub frame: u64,
    pub subject: String,
    pub state: String,
    pub action: String,
    pub reason: String,
}

#[derive(Resource, Reflect, Default)]
#[reflect(Resource)]
pub struct DecisionLog {
    pub frame: u64,
    pub entries: Vec<DecisionRecord>,
}

impl DecisionLog {
    pub fn push(&mut self, subject: &str, state: &str, action: &str, reason: &str) {
        self.entries.push(DecisionRecord {
            frame: self.frame,
            subject: subject.to_string(),
            state: state.to_string(),
            action: action.to_string(),
            reason: reason.to_string(),
        });
        if self.entries.len() > DECISION_LOG_LIMIT {
            self.entries.remove(0);
        }
        info!(
            "[AI_DECISION] frame={} subject={} state={} action={} reason={}",
            self.frame, subject, state, action, reason
        );
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DebugArrow {
    pub start: Vec3,
    pub end: Vec3,
    pub label: &'static str,
}

pub fn character_arrows(position: Vec3, velocity: Vec3, is_grounded: bool) -> Vec<DebugArrow> {
    let mut arrows = vec![DebugArrow {
        start: position,
        end: position + velocity,
        label: "velocity",
    }];
    let lift = if is_grounded { 1.2 } else { 0.35 };
    let label = if is_grounded { "grounded" } else { "airborne" };
    arrows.push(DebugArrow {
        start: position,
        end: position + Vec3::Y * lift,
        label,
    });
    arrows
}

pub fn overlay_lines(
    play_state_running: bool,
    auto_pause: bool,
    characters: &[(Vec3, Vec3, bool)],
) -> Vec<DebugArrow> {
    let origin = Vec3::ZERO;
    let mut arrows = vec![DebugArrow {
        start: origin,
        end: origin + Vec3::Y * 4.0,
        label: if play_state_running {
            "state_running"
        } else {
            "state_paused"
        },
    }];
    if auto_pause {
        arrows.push(DebugArrow {
            start: origin - Vec3::X,
            end: origin + Vec3::X,
            label: "clock_paused",
        });
    }
    for (position, velocity, is_grounded) in characters {
        arrows.extend(character_arrows(*position, *velocity, *is_grounded));
    }
    arrows
}

fn arrow_color(label: &str) -> Color {
    match label {
        "velocity" => Color::srgb(0.95, 0.2, 0.2),
        "grounded" => Color::srgb(0.2, 0.9, 0.2),
        "airborne" => Color::srgb(0.2, 0.5, 0.95),
        "state_running" => Color::srgb(0.2, 0.9, 0.2),
        "state_paused" => Color::srgb(0.95, 0.8, 0.1),
        "clock_paused" => Color::srgb(0.95, 0.1, 0.1),
        _ => Color::WHITE,
    }
}

fn tick_frame_counter(mut log: ResMut<DecisionLog>) {
    log.frame += 1;
}

fn manage_playtest_clock(
    mut clock: ResMut<PlaytestClock>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    if !clock.auto_pause {
        return;
    }
    if clock.step_frames > 0 {
        virtual_time.unpause();
        virtual_time.set_relative_speed(clock.speed);
        clock.step_frames -= 1;
    } else {
        virtual_time.pause();
    }
}

fn draw_debug_overlays(
    visuals: Res<DebugVisuals>,
    play_state: Option<Res<State<PlayState>>>,
    clock: Res<PlaytestClock>,
    mut gizmos: Gizmos,
    characters: Query<(&GlobalTransform, &LinearVelocity, &GroundState), With<Character>>,
) {
    if !visuals.enabled {
        return;
    }
    let running = play_state.as_ref().is_none_or(|s| *s.get() == PlayState::Running);
    let chars: Vec<(Vec3, Vec3, bool)> = characters
        .iter()
        .map(|(transform, velocity, ground)| {
            (
                transform.translation(),
                velocity.0,
                ground.is_grounded,
            )
        })
        .collect();
    for arrow in overlay_lines(running, clock.auto_pause, &chars) {
        gizmos.line(arrow.start, arrow.end, arrow_color(arrow.label));
    }
    for (transform, _, _) in &characters {
        let position = transform.translation();
        let radius = CAPSULE_RADIUS;
        let height = CAPSULE_HEIGHT + CAPSULE_RADIUS * 2.0;
        let corners = [
            position + Vec3::new(-radius, -height * 0.5, -radius),
            position + Vec3::new(radius, -height * 0.5, -radius),
            position + Vec3::new(radius, -height * 0.5, radius),
            position + Vec3::new(-radius, -height * 0.5, radius),
        ];
        gizmos.lineloop(corners, arrow_color("collider"));
    }
}

fn relay_agent_console(
    mut console: ResMut<AgentConsole>,
    history: Res<CommandHistory>,
    mut messages: ResMut<Messages<RunCommand>>,
    mut log: ResMut<DecisionLog>,
) {
    if !console.request.is_empty() {
        let request = std::mem::take(&mut console.request);
        messages.write(RunCommand(request.clone()));
        console.pending = true;
        log.push("agent_console", "running", &request, "remote request");
    }
    if console.pending && history.is_changed() {
        if let Some(last) = history.entries.back() {
            console.last_output = last.output.clone().unwrap_or_default();
            console.pending = false;
        }
    }
}

fn log_pause_decision(mut log: ResMut<DecisionLog>) {
    log.push("play_state", "Paused", "pause", "state transition");
}

fn log_resume_decision(mut log: ResMut<DecisionLog>) {
    log.push("play_state", "Running", "resume", "state transition");
}

fn debug_visuals_active(visuals: Res<DebugVisuals>) -> bool {
    visuals.enabled
}

pub fn register(app: &mut App) {
    app.register_type::<PlaytestClock>()
        .register_type::<DebugVisuals>()
        .register_type::<AgentConsole>()
        .register_type::<DecisionRecord>()
        .register_type::<DecisionLog>()
        .init_resource::<PlaytestClock>()
        .init_resource::<DebugVisuals>()
        .init_resource::<AgentConsole>()
        .init_resource::<DecisionLog>()
        .add_systems(
            First,
            (tick_frame_counter, manage_playtest_clock)
                .chain()
                .before(TimeSystems),
        )
        .add_systems(Update, relay_agent_console)
        .add_systems(
            Update,
            draw_debug_overlays.run_if(debug_visuals_active),
        )
        .add_systems(OnEnter(PlayState::Paused), log_pause_decision)
        .add_systems(OnEnter(PlayState::Running), log_resume_decision);
}

#[cfg(test)]
#[derive(Resource, Default)]
struct GizmoBufferProbe(usize);

#[cfg(test)]
fn probe_gizmo_buffer(mut gizmos: Gizmos, mut probe: ResMut<GizmoBufferProbe>) {
    for arrow in overlay_lines(true, false, &[]) {
        gizmos.line(arrow.start, arrow.end, Color::WHITE);
    }
    probe.0 = gizmos.buffer().list_positions.len();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::registry::commands::{
        CommandAppExt, CommandArgs, CommandOutput, CommandRegistryPlugin,
    };
    use bevy::ecs::reflect::AppTypeRegistry;
    use bevy::gizmos::GizmoPlugin;
    use bevy::time::TimePlugin;
    use std::any::TypeId;

    fn clock_app() -> App {
        let mut app = App::new();
        app.add_plugins(TimePlugin)
            .init_resource::<PlaytestClock>()
            .add_systems(First, manage_playtest_clock);
        app
    }

    #[test]
    fn auto_pause_freezes_virtual_time_when_no_steps_remain() {
        let mut app = clock_app();
        app.world_mut().resource_mut::<PlaytestClock>().auto_pause = true;
        app.update();
        assert!(app.world().resource::<Time<Virtual>>().is_paused());
    }

    #[test]
    fn step_frames_unpause_for_exactly_that_many_frames() {
        let mut app = clock_app();
        {
            let mut clock = app.world_mut().resource_mut::<PlaytestClock>();
            clock.auto_pause = true;
            clock.step_frames = 2;
        }
        app.update();
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
        assert_eq!(app.world().resource::<PlaytestClock>().step_frames, 1);
        app.update();
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
        app.update();
        assert!(app.world().resource::<Time<Virtual>>().is_paused());
        assert_eq!(app.world().resource::<PlaytestClock>().step_frames, 0);
    }

    #[test]
    fn auto_pause_off_leaves_time_untouched() {
        let mut app = clock_app();
        app.world_mut().resource_mut::<Time<Virtual>>().pause();
        app.update();
        assert!(
            app.world().resource::<Time<Virtual>>().is_paused(),
            "the clock must not unpause a game-paused simulation"
        );
    }

    #[test]
    fn agent_resources_are_reflection_registered() {
        let mut app = App::new();
        register(&mut app);
        let registry = app.world().resource::<AppTypeRegistry>();
        let registry = registry.0.read();
        for id in [
            TypeId::of::<PlaytestClock>(),
            TypeId::of::<DebugVisuals>(),
            TypeId::of::<AgentConsole>(),
            TypeId::of::<DecisionLog>(),
            TypeId::of::<DecisionRecord>(),
        ] {
            assert!(registry.get(id).is_some(), "{id:?} must be registered");
        }
    }

    #[test]
    fn decision_log_is_bounded_and_keeps_newest() {
        let mut log = DecisionLog::default();
        for i in 0..(DECISION_LOG_LIMIT + 10) {
            log.frame = i as u64;
            log.push("subject", "state", "act", "why");
        }
        assert_eq!(log.entries.len(), DECISION_LOG_LIMIT);
        assert_eq!(log.entries.first().unwrap().frame, 10);
        assert_eq!(
            log.entries.last().unwrap().frame,
            (DECISION_LOG_LIMIT + 9) as u64
        );
    }

    #[test]
    fn agent_console_round_trips_through_the_command_registry() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CommandRegistryPlugin))
            .init_resource::<DecisionLog>()
            .init_resource::<AgentConsole>()
            .init_resource::<PlaytestClock>()
            .init_resource::<DebugVisuals>();
        app.register_command(
            "test>echo",
            "d",
            |args: Res<CommandArgs>, mut out: ResMut<CommandOutput>| {
                out.set(args.args.join(" "));
            },
        );
        app.add_systems(Update, relay_agent_console);
        app.world_mut().resource_mut::<AgentConsole>().request =
            "test>echo hello agent".into();
        for _ in 0..4 {
            app.update();
        }
        assert_eq!(
            app.world().resource::<AgentConsole>().last_output,
            "hello agent"
        );
        assert!(!app.world().resource::<AgentConsole>().pending);
    }

    #[test]
    fn character_arrows_encode_velocity_and_ground_state() {
        let arrows = character_arrows(Vec3::ZERO, Vec3::X * 2.0, true);
        assert_eq!(arrows[0].label, "velocity");
        assert_eq!(arrows[0].end, Vec3::X * 2.0);
        assert_eq!(arrows[1].label, "grounded");
        assert_eq!(arrows[1].end, Vec3::Y * 1.2);
        let airborne = character_arrows(Vec3::ZERO, Vec3::ZERO, false);
        assert_eq!(airborne[1].label, "airborne");
        assert_eq!(airborne[1].end, Vec3::Y * 0.35);
    }

    #[test]
    fn overlay_lines_describe_state_clock_and_characters() {
        let running = overlay_lines(true, false, &[]);
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].label, "state_running");
        assert_eq!(running[0].end, Vec3::Y * 4.0);
        let paused_stepping = overlay_lines(false, true, &[(Vec3::ZERO, Vec3::X, true)]);
        let labels: Vec<&str> = paused_stepping.iter().map(|a| a.label).collect();
        assert_eq!(
            labels,
            vec!["state_paused", "clock_paused", "velocity", "grounded"]
        );
    }

    #[test]
    fn gizmo_buffer_receives_overlay_lines_when_visuals_enabled() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            bevy::asset::AssetPlugin::default(),
            bevy::mesh::MeshPlugin,
            GizmoPlugin,
        ))
        .init_resource::<PlaytestClock>()
        .init_resource::<DecisionLog>()
        .init_resource::<DebugVisuals>()
        .init_resource::<GizmoBufferProbe>();
        app.world_mut().resource_mut::<DebugVisuals>().enabled = true;
        app.add_systems(Update, draw_debug_overlays);
        app.add_systems(Update, probe_gizmo_buffer);
        app.update();
        assert!(
            app.world().resource::<GizmoBufferProbe>().0 > 0,
            "the state marker must land in the gizmo buffer"
        );
        assert!(app
            .world()
            .contains_resource::<bevy::gizmos::gizmos::GizmoStorage<
                bevy::gizmos::prelude::DefaultGizmoConfigGroup,
                (),
            >>());
    }
}
