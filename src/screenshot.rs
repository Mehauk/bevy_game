use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::{
    camera::{Exposure, RenderTarget, visibility::RenderLayers},
    pbr::DistanceFog,
    prelude::*,
    render::{
        render_resource::TextureFormat,
        view::{
            Tonemapping,
            screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
        },
    },
    window::WindowRef,
};

use crate::utils::registry::commands::{CommandAppExt, CommandArgs, CommandOutput, command};

#[derive(Clone, Debug)]
pub enum CameraSelector {
    ActivePrimary,
    Named(String),
    #[allow(dead_code, reason = "programmatic selector for non-console triggers")]
    Entity(Entity),
}

#[derive(Clone, Debug)]
pub enum CaptureMode {
    CameraView,
    RenderTarget,
}

#[derive(Message, Clone, Debug)]
pub struct CaptureScreenshot {
    pub camera: CameraSelector,
    pub mode: CaptureMode,
    pub resolution: Option<UVec2>,
    pub output_path: Option<PathBuf>,
}

#[derive(Component)]
struct PendingCapture {
    image: Handle<Image>,
    path: PathBuf,
}

#[derive(Component)]
pub struct ScreenshotCaptureCamera;

type CaptureCameras<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Camera,
        &'static GlobalTransform,
        &'static Projection,
        Option<&'static RenderTarget>,
        Option<&'static Name>,
        Option<&'static DistanceFog>,
        Option<&'static Exposure>,
        Option<&'static Tonemapping>,
        Option<&'static RenderLayers>,
    ),
>;

pub struct ScreenshotPlugin;

impl Plugin for ScreenshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CaptureScreenshot>()
            .add_systems(
                Update,
                (handle_capture_requests, start_pending_captures).chain(),
            );

        command!(
            app,
            "Screenshot the camera view: [filename] [width height]",
            screenshot
        );
        command!(
            app,
            "Screenshot the window including UI: [filename]",
            screenshot_window
        );
        command!(
            app,
            "Screenshot a named camera: <camera_name> [filename]",
            screenshot_camera
        );
    }
}

fn resolve_camera(cameras: &CaptureCameras, selector: &CameraSelector) -> Option<Entity> {
    match selector {
        CameraSelector::Entity(entity) => cameras.get(*entity).ok().map(|item| item.0),
        CameraSelector::Named(name) => cameras
            .iter()
            .find(|item| item.5.is_some_and(|camera_name| camera_name.as_str() == name))
            .map(|item| item.0),
        CameraSelector::ActivePrimary => cameras
            .iter()
            .filter(|item| {
                item.1.is_active
                    && item.4
                        .is_none_or(|target| matches!(target, RenderTarget::Window(WindowRef::Primary)))
            })
            .min_by_key(|item| item.1.order)
            .map(|item| item.0),
    }
}

fn handle_capture_requests(
    mut commands: Commands,
    mut requests: MessageReader<CaptureScreenshot>,
    mut images: ResMut<Assets<Image>>,
    cameras: CaptureCameras,
) {
    for request in requests.read() {
        let Some((_, camera, global_transform, projection, render_target, _, fog, exposure, tonemapping, layers)) =
            resolve_camera(&cameras, &request.camera).map(|entity| cameras.get(entity).unwrap())
        else {
            warn!("Screenshot request for unknown camera {:?}", request.camera);
            continue;
        };

        let path = request
            .output_path
            .clone()
            .unwrap_or_else(default_screenshot_path);
        if let Some(dir) = path.parent()
            && let Err(err) = fs::create_dir_all(dir) {
                warn!("Failed to create screenshot directory {dir:?}: {err}");
                continue;
            }

        match request.mode {
            CaptureMode::RenderTarget => {
                let target = render_target.cloned().unwrap_or_default();
                let logged = path.clone();
                commands
                    .spawn(Screenshot(target))
                    .observe(save_to_disk(path))
                    .observe(move |_: On<ScreenshotCaptured>| {
                        info!("Saved screenshot to {}", logged.display());
                    });
            }
            CaptureMode::CameraView => {
                let size = request
                    .resolution
                    .or_else(|| camera.physical_viewport_size())
                    .unwrap_or(UVec2::new(1920, 1080));
                let image = images.add(Image::new_target_texture(
                    size.x,
                    size.y,
                    TextureFormat::Rgba8Unorm,
                    Some(TextureFormat::Rgba8UnormSrgb),
                ));
                let mut capture_camera = camera.clone();
                capture_camera.order -= 1;
                capture_camera.is_active = true;
                let mut capture = commands.spawn((
                    Name::new("ScreenshotCaptureCamera"),
                    ScreenshotCaptureCamera,
                    Camera3d::default(),
                    capture_camera,
                    projection.clone(),
                    RenderTarget::Image(image.clone().into()),
                    Transform::from_matrix(global_transform.to_matrix()),
                    PendingCapture {
                        image,
                        path: path.clone(),
                    },
                ));
                if let Some(fog) = fog {
                    capture.insert(fog.clone());
                }
                if let Some(exposure) = exposure {
                    capture.insert(exposure.clone());
                }
                if let Some(tonemapping) = tonemapping {
                    capture.insert(tonemapping.clone());
                }
                if let Some(layers) = layers {
                    capture.insert(layers.clone());
                }
            }
        }
    }
}

fn start_pending_captures(mut commands: Commands, pending: Query<(Entity, Ref<PendingCapture>)>) {
    for (entity, capture) in &pending {
        if capture.is_added() {
            continue;
        }
        let path = capture.path.clone();
        commands
            .spawn(Screenshot::image(capture.image.clone()))
            .observe(save_to_disk(path.clone()))
            .observe(move |_: On<ScreenshotCaptured>, mut commands: Commands| {
                commands.entity(entity).despawn();
                info!("Saved screenshot to {}", path.display());
            });
        commands.entity(entity).remove::<PendingCapture>();
    }
}

fn default_screenshot_path() -> PathBuf {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    Path::new("screenshots").join(format!("screenshot-{seconds}.png"))
}

fn screenshot_path(name: Option<&String>) -> PathBuf {
    let file_name = match name {
        Some(raw) if !raw.is_empty() => {
            let mut file = raw.clone();
            if Path::new(&file).extension().is_none() {
                file.push_str(".png");
            }
            file
        }
        _ => return default_screenshot_path(),
    };
    Path::new("screenshots").join(file_name)
}

fn parse_resolution(args: &CommandArgs) -> Option<UVec2> {
    let width = args.get_f32(1)?;
    let height = args.get_f32(2)?;
    if width >= 1.0 && height >= 1.0 {
        Some(UVec2::new(width as u32, height as u32))
    } else {
        None
    }
}

fn screenshot(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    mut requests: MessageWriter<CaptureScreenshot>,
) {
    let path = screenshot_path(args.args.first());
    let resolution = parse_resolution(&args);
    requests.write(CaptureScreenshot {
        camera: CameraSelector::ActivePrimary,
        mode: CaptureMode::CameraView,
        resolution,
        output_path: Some(path.clone()),
    });
    output.set(format!("Screenshot queued -> {}", path.display()));
}

fn screenshot_window(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    mut requests: MessageWriter<CaptureScreenshot>,
) {
    let path = screenshot_path(args.args.first());
    requests.write(CaptureScreenshot {
        camera: CameraSelector::ActivePrimary,
        mode: CaptureMode::RenderTarget,
        resolution: None,
        output_path: Some(path.clone()),
    });
    output.set(format!("Window screenshot queued -> {}", path.display()));
}

fn screenshot_camera(
    args: Res<CommandArgs>,
    mut output: ResMut<CommandOutput>,
    mut requests: MessageWriter<CaptureScreenshot>,
    names: Query<&Name, With<Camera>>,
) {
    let Some(camera_name) = args.args.first() else {
        output.set("Usage: screenshot_camera <camera_name> [filename]");
        return;
    };
    if !names.iter().any(|name| name.as_str() == camera_name) {
        output.set(format!("error: no camera named '{camera_name}'"));
        return;
    }
    let path = screenshot_path(args.args.get(1));
    requests.write(CaptureScreenshot {
        camera: CameraSelector::Named(camera_name.clone()),
        mode: CaptureMode::CameraView,
        resolution: None,
        output_path: Some(path.clone()),
    });
    output.set(format!(
        "Screenshot of '{camera_name}' queued -> {}",
        path.display()
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Picked(Option<Entity>);

    fn app_with_cameras() -> App {
        let mut app = App::new();
        app.init_resource::<Picked>()
            .init_resource::<Assets<Image>>();
        app
    }

    fn spawn_camera(app: &mut App, camera: Camera, name: Option<&str>) -> Entity {
        let mut entity = app
            .world_mut()
            .spawn((camera, Projection::default(), GlobalTransform::default()));
        if let Some(name) = name {
            entity.insert(Name::new(name.to_string()));
        }
        entity.id()
    }

    fn pick(app: &mut App, selector: CameraSelector) -> Option<Entity> {
        app.add_systems(Update, move |cameras: CaptureCameras, mut picked: ResMut<Picked>| {
            picked.0 = resolve_camera(&cameras, &selector);
        });
        app.update();
        app.world().resource::<Picked>().0
    }

    #[test]
    fn active_primary_skips_inactive_and_non_primary_cameras() {
        let mut app = app_with_cameras();
        let active = spawn_camera(&mut app, Camera::default(), None);
        spawn_camera(
            &mut app,
            Camera {
                is_active: false,
                ..default()
            },
            None,
        );
        assert_eq!(pick(&mut app, CameraSelector::ActivePrimary), Some(active));
    }

    #[test]
    fn active_primary_prefers_the_lowest_order_camera() {
        let mut app = app_with_cameras();
        spawn_camera(&mut app, Camera { order: 1, ..default() }, None);
        let first = spawn_camera(&mut app, Camera { order: 0, ..default() }, None);
        assert_eq!(pick(&mut app, CameraSelector::ActivePrimary), Some(first));
    }

    #[test]
    fn named_selector_matches_by_name() {
        let mut app = app_with_cameras();
        let named = spawn_camera(&mut app, Camera::default(), Some("MainCamera"));
        spawn_camera(&mut app, Camera::default(), Some("Other"));
        assert_eq!(
            pick(&mut app, CameraSelector::Named("MainCamera".into())),
            Some(named)
        );
    }

    #[test]
    fn named_selector_rejects_unknown_names() {
        let mut app = app_with_cameras();
        spawn_camera(&mut app, Camera::default(), Some("MainCamera"));
        assert_eq!(pick(&mut app, CameraSelector::Named("Missing".into())), None);
    }

    #[test]
    fn entity_selector_matches_camera_entities() {
        let mut app = app_with_cameras();
        let camera = spawn_camera(&mut app, Camera::default(), None);
        assert_eq!(pick(&mut app, CameraSelector::Entity(camera)), Some(camera));
    }

    #[test]
    fn entity_selector_rejects_non_cameras() {
        let mut app = app_with_cameras();
        spawn_camera(&mut app, Camera::default(), None);
        let not_a_camera = app.world_mut().spawn_empty().id();
        assert_eq!(pick(&mut app, CameraSelector::Entity(not_a_camera)), None);
    }

    #[test]
    fn screenshot_path_adds_png_extension_when_missing() {
        let path = screenshot_path(Some(&"aerial".to_string()));
        assert_eq!(path.file_name().unwrap(), "aerial.png");
        let path = screenshot_path(Some(&"aerial.jpg".to_string()));
        assert_eq!(path.file_name().unwrap(), "aerial.jpg");
    }

    #[test]
    fn screenshot_path_defaults_into_the_screenshots_directory() {
        let path = screenshot_path(None);
        assert_eq!(path.parent().unwrap(), Path::new("screenshots"));
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("screenshot-")
        );
    }

    #[test]
    fn camera_view_request_spawns_and_arms_a_capture_camera() {
        let mut app = App::new();
        app.add_message::<CaptureScreenshot>()
            .init_resource::<Assets<Image>>()
            .add_systems(
                Update,
                (handle_capture_requests, start_pending_captures).chain(),
            );
        spawn_camera(&mut app, Camera::default(), None);

        {
            let world = app.world_mut();
            world
                .resource_mut::<Messages<CaptureScreenshot>>()
                .write(CaptureScreenshot {
                    camera: CameraSelector::ActivePrimary,
                    mode: CaptureMode::CameraView,
                    resolution: Some(UVec2::new(8, 8)),
                    output_path: Some(Path::new("target/test-screenshots/flow.png").into()),
                });
        }
        app.update();

        let capture = app
            .world_mut()
            .query_filtered::<Entity, (With<Name>, With<PendingCapture>, With<Camera3d>)>()
            .single(app.world())
            .expect("the request spawns a pending capture camera");
        let pending_image = app
            .world()
            .get::<PendingCapture>(capture)
            .unwrap()
            .image
            .clone();
        assert!(
            matches!(
                app.world().get::<RenderTarget>(capture),
                Some(RenderTarget::Image(render)) if render.handle == pending_image
            ),
            "the capture camera renders to the pending image"
        );

        app.update();
        app.update();
        assert!(
            app.world().get::<PendingCapture>(capture).is_none(),
            "the pending capture is armed and consumed"
        );
    }

    #[test]
    fn screenshot_camera_rejects_names_that_belong_to_non_cameras() {
        let mut app = App::new();
        app.init_resource::<CommandArgs>()
            .init_resource::<CommandOutput>()
            .add_message::<CaptureScreenshot>();
        app.world_mut().spawn((Name::new("Terrain"), GlobalTransform::default()));
        *app.world_mut().resource_mut::<CommandArgs>() = CommandArgs {
            args: vec!["Terrain".into()],
        };

        let system = app.world_mut().register_system(screenshot_camera);
        app.world_mut().run_system(system).unwrap();

        assert!(
            app.world()
                .resource::<Messages<CaptureScreenshot>>()
                .is_empty(),
            "a named non-camera must not queue a capture request"
        );
    }

    #[test]
    fn screenshot_camera_accepts_named_cameras() {
        let mut app = App::new();
        app.init_resource::<CommandArgs>()
            .init_resource::<CommandOutput>()
            .add_message::<CaptureScreenshot>();
        app.world_mut()
            .spawn((Camera::default(), Name::new("Main"), GlobalTransform::default()));
        *app.world_mut().resource_mut::<CommandArgs>() = CommandArgs {
            args: vec!["Main".into()],
        };

        let system = app.world_mut().register_system(screenshot_camera);
        app.world_mut().run_system(system).unwrap();

        assert_eq!(
            app.world()
                .resource::<Messages<CaptureScreenshot>>()
                .len(),
            1,
            "a named camera must queue exactly one capture request"
        );
    }

    #[test]
    fn resolution_needs_both_dimensions() {
        let args = CommandArgs {
            args: vec!["shot".into(), "1280".into(), "720".into()],
        };
        assert_eq!(parse_resolution(&args), Some(UVec2::new(1280, 720)));
        let args = CommandArgs {
            args: vec!["shot".into(), "1280".into()],
        };
        assert_eq!(parse_resolution(&args), None);
        let args = CommandArgs {
            args: vec!["shot".into(), "0".into(), "720".into()],
        };
        assert_eq!(parse_resolution(&args), None);
    }
}
