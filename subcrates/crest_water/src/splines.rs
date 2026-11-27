use bevy::prelude::*;

use crate::inputs::{FlowSpline, FoamSpline};

#[derive(Component, Clone, Copy, Debug)]
pub struct SplinePoint {
    pub position: Vec3,
    pub width: f32,
}

#[derive(Component, Clone, Debug, Default)]
pub struct Spline {
    pub points: Vec<Vec3>,
    pub widths: Vec<f32>,
    pub flow_speed: f32,
    pub foam_amount: f32,
}

impl Spline {
    pub fn from_points(points: &[SplinePoint], flow_speed: f32, foam_amount: f32) -> Self {
        Self {
            points: points.iter().map(|point| point.position).collect(),
            widths: points.iter().map(|point| point.width).collect(),
            flow_speed,
            foam_amount,
        }
    }

    pub fn average_width(&self) -> f32 {
        if self.widths.is_empty() {
            return 1.0;
        }
        self.widths.iter().sum::<f32>() / self.widths.len() as f32
    }
}

fn sync_splines(
    mut commands: Commands,
    splines: Query<(Entity, &Spline, Option<&FlowSpline>, Option<&FoamSpline>), Changed<Spline>>,
) {
    for (entity, spline, flow, foam) in &splines {
        if spline.points.len() < 2 {
            if flow.is_some() {
                commands.entity(entity).remove::<FlowSpline>();
            }
            if foam.is_some() {
                commands.entity(entity).remove::<FoamSpline>();
            }
            continue;
        }
        let points: Vec<[f32; 3]> = spline.points.iter().map(|point| point.to_array()).collect();
        let width = spline.average_width();
        if spline.flow_speed != 0.0 {
            commands.entity(entity).insert(FlowSpline {
                points: points.clone(),
                width,
                flow_speed: spline.flow_speed,
            });
        } else if flow.is_some() {
            commands.entity(entity).remove::<FlowSpline>();
        }
        if spline.foam_amount != 0.0 {
            commands.entity(entity).insert(FoamSpline {
                points,
                width,
                foam_amount: spline.foam_amount,
            });
        } else if foam.is_some() {
            commands.entity(entity).remove::<FoamSpline>();
        }
    }
}

pub struct SplinesPlugin;

impl Plugin for SplinesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, sync_splines);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let mut app = App::default();
        app.add_systems(PreUpdate, sync_splines);
        app
    }

    fn sample_spline() -> Spline {
        Spline {
            points: vec![
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(10.0, 0.0, 0.0),
                Vec3::new(20.0, 0.0, 0.0),
            ],
            widths: vec![2.0, 4.0, 6.0],
            flow_speed: 2.5,
            foam_amount: 0.5,
        }
    }

    #[test]
    fn spline_inserts_flow_and_foam() {
        let mut app = test_app();
        let entity = app.world_mut().spawn(sample_spline()).id();
        app.update();
        let flow = app.world().get::<FlowSpline>(entity).unwrap();
        assert_eq!(flow.points.len(), 3);
        assert_eq!(flow.points[1], [10.0, 0.0, 0.0]);
        assert_eq!(flow.flow_speed, 2.5);
        assert_eq!(flow.width, 4.0);
        let foam = app.world().get::<FoamSpline>(entity).unwrap();
        assert_eq!(foam.points.len(), 3);
        assert_eq!(foam.foam_amount, 0.5);
    }

    #[test]
    fn zero_flow_speed_removes_flow_spline() {
        let mut app = test_app();
        let entity = app.world_mut().spawn(sample_spline()).id();
        app.update();
        assert!(app.world().get::<FlowSpline>(entity).is_some());
        app.world_mut().get_mut::<Spline>(entity).unwrap().flow_speed = 0.0;
        app.update();
        assert!(app.world().get::<FlowSpline>(entity).is_none());
        assert!(app.world().get::<FoamSpline>(entity).is_some());
    }

    #[test]
    fn short_spline_has_no_emitters() {
        let mut app = test_app();
        let entity = app
            .world_mut()
            .spawn(Spline {
                points: vec![Vec3::X],
                widths: vec![2.0],
                flow_speed: 1.0,
                foam_amount: 1.0,
            })
            .id();
        app.update();
        assert!(app.world().get::<FlowSpline>(entity).is_none());
        assert!(app.world().get::<FoamSpline>(entity).is_none());
    }

    #[test]
    fn from_points_averages_width() {
        let points = [
            SplinePoint {
                position: Vec3::X,
                width: 2.0,
            },
            SplinePoint {
                position: Vec3::Y,
                width: 4.0,
            },
        ];
        let spline = Spline::from_points(&points, 1.0, 0.0);
        assert_eq!(spline.points.len(), 2);
        assert_eq!(spline.average_width(), 3.0);
        assert_eq!(Spline::default().average_width(), 1.0);
    }
}
