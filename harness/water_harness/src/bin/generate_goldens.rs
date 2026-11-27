use water_harness::cpu::CpuOcean;
use water_harness::golden::{Golden, GoldenSample, write_golden};
use water_harness::scenarios::{FREEZE_FRAMES, VIEWER, scenarios};

fn main() {
    for scenario in scenarios() {
        let mut ocean = CpuOcean::new(&scenario, VIEWER);
        for _ in 0..scenario.frames {
            ocean.step();
        }
        for _ in 0..FREEZE_FRAMES {
            ocean.freeze();
        }
        let time = ocean.time();
        let samples = scenario
            .probes
            .iter()
            .enumerate()
            .map(|(probe, point)| {
                let sample = ocean.sample(point.x, point.y);
                GoldenSample {
                    frame: scenario.frames,
                    probe,
                    height: sample.height,
                    disp_x: sample.disp_x,
                    disp_y: sample.disp_y,
                    disp_z: sample.disp_z,
                    jacobian: sample.jacobian,
                    flow_x: sample.flow.x,
                    flow_z: sample.flow.y,
                    depth: sample.depth,
                }
            })
            .collect();
        let golden = Golden {
            name: scenario.name.to_string(),
            frames: scenario.frames,
            time,
            probes: scenario
                .probes
                .iter()
                .map(|p| [p.x, p.y])
                .collect(),
            samples,
        };
        write_golden(&golden).expect("golden writes");
        println!("wrote golden {} at t={time:.3}", scenario.name);
    }
}
