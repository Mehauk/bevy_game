use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GoldenSample {
    pub frame: u64,
    pub probe: usize,
    pub height: f32,
    pub disp_x: f32,
    pub disp_y: f32,
    pub disp_z: f32,
    pub jacobian: f32,
    pub flow_x: f32,
    pub flow_z: f32,
    pub depth: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Golden {
    pub name: String,
    pub frames: u64,
    pub time: f32,
    pub probes: Vec<[f32; 2]>,
    pub samples: Vec<GoldenSample>,
}

pub fn goldens_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("goldens")
}

fn golden_path(name: &str) -> PathBuf {
    goldens_dir().join(format!("{name}.json"))
}

pub fn write_golden(golden: &Golden) -> io::Result<()> {
    fs::create_dir_all(goldens_dir())?;
    fs::write(golden_path(&golden.name), serde_json::to_string_pretty(golden).unwrap())
}

pub fn read_golden(name: &str) -> io::Result<Golden> {
    let text = fs::read_to_string(golden_path(name))?;
    serde_json::from_str(&text).map_err(io::Error::other)
}
