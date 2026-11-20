use std::{collections::VecDeque, fmt, time::Instant};

use bevy::prelude::*;

use crate::utils::registry::commands::{CommandAppExt, CommandArgs, command};

const DEFAULT_SPIKE_THRESHOLD_MS: f64 = 100.0;
const DEFAULT_MAX_SAMPLES: usize = 300;
const WARMUP_FRAMES: u64 = 5;

pub struct ProfilerPlugin;

impl Plugin for ProfilerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProfilerState>()
            .add_systems(First, begin_frame)
            .add_systems(
                RunFixedMainLoop,
                before_fixed.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
            )
            .add_systems(FixedFirst, count_fixed_step)
            .add_systems(
                RunFixedMainLoop,
                after_fixed.in_set(RunFixedMainLoopSystems::AfterFixedMainLoop),
            )
            .add_systems(SpawnScene, after_update)
            .add_systems(Last, end_frame);

        command!(
            app,
            "Log frame time summary (avg/p95/p99/max) and worst spike",
            profiler_report
        );
        command!(
            app,
            "Set spike warning threshold in ms: <ms> (0 disables warnings)",
            set_spike_threshold
        );
        command!(
            app,
            "Clear recorded profiler samples and worst spike",
            reset_profiler
        );
        command!(app, "Pause or resume the frame profiler", toggle_profiler);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameSample {
    pub frame: u64,
    pub total_ms: f64,
    pub outside_ms: f64,
    pub pre_ms: f64,
    pub fixed_ms: f64,
    pub fixed_steps: u32,
    pub update_ms: f64,
    pub post_ms: f64,
}

impl FrameSample {
    pub fn dominant_phase(&self) -> &'static str {
        let phases = [
            ("render/present/OS", self.outside_ms),
            ("pre", self.pre_ms),
            ("fixed", self.fixed_ms),
            ("update", self.update_ms),
            ("post", self.post_ms),
        ];
        phases
            .into_iter()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map_or("unknown", |(name, _)| name)
    }
}

impl fmt::Display for FrameSample {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "frame {} ({:.1} ms, hotspot: {}) | render/present/OS: {:.1}ms | pre: {:.1}ms | fixed(x{}): {:.1}ms | update: {:.1}ms | post: {:.1}ms",
            self.frame,
            self.total_ms,
            self.dominant_phase(),
            self.outside_ms,
            self.pre_ms,
            self.fixed_steps,
            self.fixed_ms,
            self.update_ms,
            self.post_ms
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ProfilerSummary {
    pub count: usize,
    pub avg_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    pub spike_count: u64,
}

#[derive(Resource)]
pub struct ProfilerState {
    pub enabled: bool,
    pub spike_threshold_ms: f64,
    max_samples: usize,
    samples: VecDeque<FrameSample>,
    spike_count: u64,
    worst_spike: Option<FrameSample>,
    frame_counter: u64,
    last_end: Option<Instant>,
    frame_start: Instant,
    fixed_start: Instant,
    after_fixed: Instant,
    after_update: Instant,
    outside_ms: f64,
    pre_ms: f64,
    fixed_ms: f64,
    update_ms: f64,
    fixed_steps: u32,
}

impl Default for ProfilerState {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            enabled: true,
            spike_threshold_ms: DEFAULT_SPIKE_THRESHOLD_MS,
            max_samples: DEFAULT_MAX_SAMPLES,
            samples: VecDeque::with_capacity(DEFAULT_MAX_SAMPLES),
            spike_count: 0,
            worst_spike: None,
            frame_counter: 0,
            last_end: None,
            frame_start: now,
            fixed_start: now,
            after_fixed: now,
            after_update: now,
            outside_ms: 0.0,
            pre_ms: 0.0,
            fixed_ms: 0.0,
            update_ms: 0.0,
            fixed_steps: 0,
        }
    }
}

impl ProfilerState {
    fn record(&mut self, sample: FrameSample) {
        if self.samples.len() >= self.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);

        if self.spike_threshold_ms > 0.0 && sample.total_ms >= self.spike_threshold_ms {
            self.spike_count += 1;
            let is_worse = self
                .worst_spike
                .is_none_or(|prev| sample.total_ms > prev.total_ms);
            if is_worse {
                self.worst_spike = Some(sample);
            }
        }
    }

    pub fn summary(&self) -> Option<ProfilerSummary> {
        if self.samples.is_empty() {
            return None;
        }
        let mut totals: Vec<f64> = self.samples.iter().map(|s| s.total_ms).collect();
        totals.sort_by(f64::total_cmp);

        let sum: f64 = totals.iter().sum();
        Some(ProfilerSummary {
            count: totals.len(),
            avg_ms: sum / totals.len() as f64,
            p95_ms: percentile(&totals, 0.95),
            p99_ms: percentile(&totals, 0.99),
            max_ms: *totals.last().unwrap_or(&0.0),
            spike_count: self.spike_count,
        })
    }

    pub fn reset(&mut self) {
        self.samples.clear();
        self.spike_count = 0;
        self.worst_spike = None;
        self.last_end = None;
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn ms_since(start: Instant, now: Instant) -> f64 {
    now.saturating_duration_since(start).as_secs_f64() * 1000.0
}

fn begin_frame(mut state: ResMut<ProfilerState>) {
    if !state.enabled {
        return;
    }
    let now = Instant::now();
    state.outside_ms = state.last_end.map_or(0.0, |t| ms_since(t, now));
    state.frame_start = now;
    state.fixed_steps = 0;
}

fn before_fixed(mut state: ResMut<ProfilerState>) {
    if !state.enabled {
        return;
    }
    let now = Instant::now();
    state.pre_ms = ms_since(state.frame_start, now);
    state.fixed_start = now;
}

fn count_fixed_step(mut state: ResMut<ProfilerState>) {
    if !state.enabled {
        return;
    }
    state.fixed_steps += 1;
}

fn after_fixed(mut state: ResMut<ProfilerState>) {
    if !state.enabled {
        return;
    }
    let now = Instant::now();
    state.fixed_ms = ms_since(state.fixed_start, now);
    state.after_fixed = now;
}

fn after_update(mut state: ResMut<ProfilerState>) {
    if !state.enabled {
        return;
    }
    let now = Instant::now();
    state.update_ms = ms_since(state.after_fixed, now);
    state.after_update = now;
}

fn end_frame(mut state: ResMut<ProfilerState>) {
    if !state.enabled {
        return;
    }
    let now = Instant::now();
    let post_ms = ms_since(state.after_update, now);
    let total_ms = state.outside_ms + ms_since(state.frame_start, now);

    state.frame_counter += 1;
    state.last_end = Some(now);

    if state.frame_counter <= WARMUP_FRAMES {
        return;
    }

    let sample = FrameSample {
        frame: state.frame_counter,
        total_ms,
        outside_ms: state.outside_ms,
        pre_ms: state.pre_ms,
        fixed_ms: state.fixed_ms,
        fixed_steps: state.fixed_steps,
        update_ms: state.update_ms,
        post_ms,
    };

    state.record(sample);

    if state.spike_threshold_ms > 0.0 && sample.total_ms >= state.spike_threshold_ms {
        warn!("SPIKE {sample}");
    }
}

fn profiler_report(state: Res<ProfilerState>) {
    let Some(summary) = state.summary() else {
        info!("Profiler: no samples recorded yet");
        return;
    };
    info!(
        "Profiler (last {} frames): avg={:.1}ms | p95={:.1}ms | p99={:.1}ms | max={:.1}ms | spikes(>={:.0}ms)={}",
        summary.count,
        summary.avg_ms,
        summary.p95_ms,
        summary.p99_ms,
        summary.max_ms,
        state.spike_threshold_ms,
        summary.spike_count,
    );
    if let Some(worst) = state.worst_spike {
        info!("Worst spike: {worst}");
    }
}

fn set_spike_threshold(args: Res<CommandArgs>, mut state: ResMut<ProfilerState>) {
    let Some(ms) = args.get_f32(0).filter(|v| *v >= 0.0) else {
        info!(
            "Current spike threshold: {:.1}ms. Usage: set_spike_threshold <ms> (0 disables)",
            state.spike_threshold_ms
        );
        return;
    };
    state.spike_threshold_ms = ms as f64;
    info!("Set profiler spike threshold to {:.1}ms", state.spike_threshold_ms);
}

fn reset_profiler(mut state: ResMut<ProfilerState>) {
    state.reset();
    info!("Profiler samples and worst spike cleared");
}

fn toggle_profiler(mut state: ResMut<ProfilerState>) {
    state.enabled = !state.enabled;
    if !state.enabled {
        state.last_end = None;
    }
    info!(
        "Profiler {}",
        if state.enabled { "resumed" } else { "paused" }
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_caps_samples_and_tracks_worst_spike() {
        let mut state = ProfilerState {
            max_samples: 3,
            spike_threshold_ms: 50.0,
            ..default()
        };

        for i in 1..=5 {
            state.record(FrameSample {
                frame: i,
                total_ms: i as f64 * 20.0,
                outside_ms: i as f64 * 20.0,
                ..default()
            });
        }

        assert_eq!(state.samples.len(), 3);
        assert_eq!(state.samples.front().unwrap().frame, 3);
        assert_eq!(state.spike_count, 3);
        assert_eq!(state.worst_spike.unwrap().frame, 5);
    }

    #[test]
    fn summary_computes_percentiles_and_dominant_phase() {
        let mut state = ProfilerState::default();
        for i in 1..=100 {
            state.record(FrameSample {
                frame: i,
                total_ms: i as f64,
                update_ms: i as f64,
                ..default()
            });
        }

        let s = state.summary().unwrap();
        assert_eq!(s.count, 100);
        assert!((s.avg_ms - 50.5).abs() < 1e-6);
        assert_eq!(s.p95_ms, 95.0);
        assert_eq!(s.p99_ms, 99.0);
        assert_eq!(s.max_ms, 100.0);
        assert_eq!(state.samples.back().unwrap().dominant_phase(), "update");
    }
}
