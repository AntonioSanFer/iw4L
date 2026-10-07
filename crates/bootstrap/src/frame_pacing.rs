//! Frame pacing, and a `frames:` line every few seconds that says where a
//! frame's time went.
//!
//! `max_fps` sleeps the main world to a fixed frame grid. Under `Fifo` a frame
//! that misses one vsync waits for the next, so a 60 Hz panel shows 60, 30 or
//! 20 fps and a frame time hovering around one step flips between two of them;
//! pacing below the step the device can hold keeps it on one.
//!
//! The line reads: `interval` is the wall between frame starts (what a player
//! sees), `update` the main world's schedules, `render` the render world's
//! schedule without `acquire`, and `acquire` the wait for a swapchain image,
//! which grows when the GPU or vsync is the limit. With pipelined rendering
//! `update` and `render` overlap, so the larger of the two is the CPU bound.
//! On by default on Android; `IW4L_FRAME_LOG=1` turns it on elsewhere.

use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::render::view::prepare_windows;
use bevy::render::{Render, RenderApp, RenderSystems};

const REPORT_EVERY: Duration = Duration::from_secs(5);

const FRAME_LOG_ENV: &str = "IW4L_FRAME_LOG";
const MAX_FPS_ENV: &str = "IW4L_MAX_FPS";

/// Render-world samples in milliseconds, handed to the main world's report.
#[derive(Default)]
struct RenderSamples {
    render: Vec<f32>,
    acquire: Vec<f32>,
}

static RENDER_SAMPLES: Mutex<RenderSamples> = Mutex::new(RenderSamples {
    render: Vec::new(),
    acquire: Vec::new(),
});

#[derive(Resource, Default)]
struct RenderClock {
    begun: Option<Instant>,
    acquire_begun: Option<Instant>,
    acquire: Duration,
}

#[derive(Resource)]
struct FrameStats {
    report_begun: Instant,
    frame_begun: Option<Instant>,
    interval: Vec<f32>,
    update: Vec<f32>,
}

/// Seen once at startup: an override for `max_fps`, so a run can be measured
/// uncapped without touching `settings.cfg`.
pub(crate) fn max_fps(settings: &frame::GameSettings) -> u32 {
    static FPS: OnceLock<Option<u32>> = OnceLock::new();
    FPS.get_or_init(|| env_number(MAX_FPS_ENV))
        .unwrap_or(settings.max_fps)
}

pub(crate) fn env_number<T: std::str::FromStr>(name: &str) -> Option<T> {
    let value = std::env::var(name).ok()?;
    let parsed = value.trim().parse().ok();
    if parsed.is_none() {
        diag::warn!(Launch, "{name}={value} is not a number; ignored");
    }
    parsed
}

fn frame_log_enabled() -> bool {
    cfg!(target_os = "android") || perf::switch(FRAME_LOG_ENV)
}

fn ms(duration: Duration) -> f32 {
    duration.as_secs_f32() * 1000.0
}

fn begin_frame(mut stats: ResMut<FrameStats>) {
    let now = Instant::now();
    if let Some(previous) = stats.frame_begun.replace(now) {
        stats.interval.push(ms(now - previous));
    }
}

fn end_frame(
    mut stats: ResMut<FrameStats>,
    settings: Res<frame::GameSettings>,
    scale: Res<crate::render_scale::SceneScale>,
) {
    let now = Instant::now();
    if let Some(begun) = stats.frame_begun {
        stats.update.push(ms(now - begun));
    }
    if now - stats.report_begun < REPORT_EVERY {
        return;
    }
    let seconds = (now - stats.report_begun).as_secs_f32();
    stats.report_begun = now;
    let mut interval = std::mem::take(&mut stats.interval);
    let mut update = std::mem::take(&mut stats.update);
    let (mut render, mut acquire) = {
        let mut samples = RENDER_SAMPLES
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        (
            std::mem::take(&mut samples.render),
            std::mem::take(&mut samples.acquire),
        )
    };
    if interval.is_empty() {
        return;
    }
    let fps = interval.len() as f32 / seconds;
    let cap = match max_fps(&settings) {
        0 => "off".to_owned(),
        fps => fps.to_string(),
    };
    diag::info!(
        World,
        "frames: fps={fps:.1} interval {} | update {} | render {} | acquire {} | scene={} cap={cap}",
        percentiles(&mut interval),
        percentiles(&mut update),
        percentiles(&mut render),
        percentiles(&mut acquire),
        scale.describe(),
    );
}

/// `p50/p95/p99/max` in milliseconds.
fn percentiles(samples: &mut [f32]) -> String {
    if samples.is_empty() {
        return "-".to_owned();
    }
    samples.sort_by(f32::total_cmp);
    let at = |q: f32| samples[((samples.len() - 1) as f32 * q).round() as usize];
    format!(
        "{:.1}/{:.1}/{:.1}/{:.1}ms",
        at(0.5),
        at(0.95),
        at(0.99),
        samples[samples.len() - 1]
    )
}

/// Sleeps to the next slot of a fixed grid. A frame that overran by more than
/// a whole period restarts the grid rather than rushing to catch up.
fn pace(settings: Res<frame::GameSettings>, mut next: Local<Option<Instant>>) {
    let fps = max_fps(&settings);
    if fps == 0 {
        *next = None;
        return;
    }
    let period = Duration::from_secs_f64(1.0 / f64::from(fps));
    let now = Instant::now();
    let deadline = next.unwrap_or(now);
    if deadline > now {
        std::thread::sleep(deadline - now);
    }
    let slot = if now.saturating_duration_since(deadline) > period {
        now
    } else {
        deadline
    };
    *next = Some(slot + period);
}

fn begin_render(mut clock: ResMut<RenderClock>) {
    clock.begun = Some(Instant::now());
    clock.acquire = Duration::ZERO;
}

fn begin_acquire(mut clock: ResMut<RenderClock>) {
    clock.acquire_begun = Some(Instant::now());
}

fn end_acquire(mut clock: ResMut<RenderClock>) {
    if let Some(begun) = clock.acquire_begun.take() {
        clock.acquire = begun.elapsed();
    }
}

fn end_render(mut clock: ResMut<RenderClock>) {
    let Some(begun) = clock.begun.take() else {
        return;
    };
    let total = begun.elapsed();
    let mut samples = RENDER_SAMPLES
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    samples.render.push(ms(total.saturating_sub(clock.acquire)));
    samples.acquire.push(ms(clock.acquire));
}

pub(crate) struct FramePacingPlugin;

impl Plugin for FramePacingPlugin {
    fn build(&self, app: &mut App) {
        if frame_log_enabled() {
            app.insert_resource(FrameStats {
                report_begun: Instant::now(),
                frame_begun: None,
                interval: Vec::new(),
                update: Vec::new(),
            })
            .add_systems(First, begin_frame)
            .add_systems(Last, end_frame.before(pace));
            if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
                render_app.init_resource::<RenderClock>().add_systems(
                    Render,
                    (
                        begin_render.in_set(RenderSystems::ExtractCommands),
                        begin_acquire
                            .in_set(RenderSystems::PrepareViews)
                            .before(prepare_windows),
                        end_acquire
                            .in_set(RenderSystems::PrepareViews)
                            .after(prepare_windows),
                        end_render.in_set(RenderSystems::PostCleanup),
                    ),
                );
            }
        }
        app.add_systems(Last, pace);
    }
}
