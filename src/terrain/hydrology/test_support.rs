use crate::terrain::field::ScalarField;
use crate::terrain::topology::WorldTopology;

pub(crate) fn plane(dim: usize, size: f32, topology: WorldTopology, slope_x: f32) -> ScalarField {
    let mut field = ScalarField::new(dim, size, topology);
    for z in 0..dim {
        for x in 0..dim {
            field.set(x, z, x as f32 * slope_x);
        }
    }
    field
}

pub(crate) fn rain(dim: usize, size: f32, topology: WorldTopology) -> ScalarField {
    let mut field = ScalarField::new(dim, size, topology);
    field.fill(1.0);
    field
}

pub(crate) fn constant_field(
    dim: usize,
    size: f32,
    topology: WorldTopology,
    value: f32,
) -> ScalarField {
    let mut field = ScalarField::new(dim, size, topology);
    field.fill(value);
    field
}

pub(crate) fn cone_field(dim: usize, size: f32, topology: WorldTopology) -> ScalarField {
    let mut field = ScalarField::new(dim, size, topology);
    let centre = dim as f32 * 0.5;
    for z in 0..dim {
        for x in 0..dim {
            let dx = x as f32 - centre;
            let dz = z as f32 - centre;
            let radius = (dx * dx + dz * dz).sqrt();
            field.set(x, z, (12.0 - radius * 0.8).max(0.0));
        }
    }
    field
}

pub(crate) fn plain_field(dim: usize, size: f32) -> ScalarField {
    let mut field = ScalarField::new(dim, size, WorldTopology::Finite);
    field.fill(10.0);
    field
}
