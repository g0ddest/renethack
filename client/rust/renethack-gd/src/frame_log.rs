//! The frame log (`RENETHACK_FRAME_LOG=<file>`): a line a frame, for timing
//! the game end to end (`tools/perf.py` reads it). It records how long the
//! frame took by the wall clock and what the game's own work in it spent,
//! by part. It records what the renderer compiled and the memory used. It
//! records what was going on: the title or the game, the start-up veil, the
//! level, the self-test's step.
//!
//! A frame is timed from the start of its `process` to the start of the
//! next one: its draw is in it. Its line is written when the next frame
//! starts. The renderer's own CPU time for the frame (`render`: its setup
//! and the viewport's draw), its draw calls and the nodes in the tree tell
//! the rest of the frame apart from the game's work; the GPU's own time
//! needs timestamp queries, which Metal does not give Godot.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::Instant;

use godot::classes::performance::Monitor;
use godot::classes::rendering_server::RenderingInfo;
use godot::classes::{Performance, RenderingServer};
use godot::obj::Singleton;
use godot::prelude::Rid;

/// The columns, in order.
const HEADER: &str = "frame,t,wall,work,input,pump,views,map,warm,preload,test,render,draws,nodes,pipelines,canvas,mesh,surface,draw,specialization,vmem,static,built,level,depth,branch,requests,state,veil,firsts,step";

/// The game's own work in a frame, by part (milliseconds).
#[derive(Debug, Default, Clone, Copy)]
pub struct Parts {
    pub input: f64,
    pub pump: f64,
    pub views: f64,
    /// The map's own sync, in `views` (the HUD and the panels are the
    /// rest).
    pub map: f64,
    /// The title's warm-ups and the start-up veil.
    pub warm: f64,
    /// The map's loading ahead (or the title's tick instead).
    pub preload: f64,
    /// The self-test's step.
    pub test: f64,
}

/// What a frame was about, besides its timings.
#[derive(Debug, Default, Clone)]
pub struct Context {
    /// The game's state (the title, a game...).
    pub state: String,
    /// The start-up veil: 2 down, 1 lifting, 0 gone.
    pub veil: u8,
    /// The level's generation (it changes with the level), its depth and
    /// branch.
    pub level: u64,
    pub depth: i64,
    pub branch: String,
    /// Models the map built this frame.
    pub built: usize,
    /// The engine's requests so far.
    pub requests: u64,
    /// The models drawn for the first time (names, by spaces).
    pub firsts: String,
    /// The self-test's step.
    pub step: String,
}

/// The frame being timed.
struct Frame {
    begun: Instant,
    work: f64,
    parts: Parts,
    context: Context,
}

pub struct FrameLog {
    out: BufWriter<File>,
    start: Instant,
    frame: u64,
    now: Option<Frame>,
    /// The renderer's counters at the frame's start: pipelines compiled,
    /// video memory (MB).
    pipelines: [i64; 5],
    vmem: f64,
    /// The window's viewport, its render time measured.
    viewport: Option<Rid>,
}

/// The pipelines Godot compiled so far, by its five kinds (canvas, mesh,
/// surface, draw, specialization).
fn pipelines_compiled() -> [i64; 5] {
    let perf = Performance::singleton();
    [
        Monitor::PIPELINE_COMPILATIONS_CANVAS,
        Monitor::PIPELINE_COMPILATIONS_MESH,
        Monitor::PIPELINE_COMPILATIONS_SURFACE,
        Monitor::PIPELINE_COMPILATIONS_DRAW,
        Monitor::PIPELINE_COMPILATIONS_SPECIALIZATION,
    ]
    .map(|m| perf.get_monitor(m) as i64)
}

fn video_memory() -> f64 {
    Performance::singleton().get_monitor(Monitor::RENDER_VIDEO_MEM_USED) / 1e6
}

impl FrameLog {
    /// The log `RENETHACK_FRAME_LOG` names, if it does and can be made.
    pub fn from_env() -> Option<FrameLog> {
        let path = std::env::var_os("RENETHACK_FRAME_LOG")?;
        let file = match File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                godot::global::godot_warn!(
                    "renethack: no frame log at {}: {e}",
                    std::path::Path::new(&path).display()
                );
                return None;
            }
        };
        let mut out = BufWriter::new(file);
        let _ = writeln!(out, "{HEADER}");
        Some(FrameLog {
            out,
            start: Instant::now(),
            frame: 0,
            now: None,
            pipelines: pipelines_compiled(),
            vmem: video_memory(),
            viewport: None,
        })
    }

    /// The viewport whose render time is logged.
    pub fn set_viewport(&mut self, rid: Rid) {
        if self.viewport.is_none() {
            RenderingServer::singleton().viewport_set_measure_render_time(rid, true);
            self.viewport = Some(rid);
        }
    }

    /// A frame starts: the one before is over, and written.
    pub fn begin(&mut self) {
        let now = Instant::now();
        let pipelines = pipelines_compiled();
        let vmem = video_memory();
        if let Some(f) = self.now.take() {
            let wall = now.duration_since(f.begun).as_secs_f64() * 1000.0;
            let t = f.begun.duration_since(self.start).as_secs_f64();
            let p = f.parts;
            let c = &f.context;
            let perf = Performance::singleton();
            let memory = perf.get_monitor(Monitor::MEMORY_STATIC) / 1e6;
            let nodes = perf.get_monitor(Monitor::OBJECT_NODE_COUNT) as i64;
            let rs = RenderingServer::singleton();
            let render = rs.get_frame_setup_time_cpu()
                + self
                    .viewport
                    .map_or(0.0, |rid| rs.viewport_get_measured_render_time_cpu(rid));
            let draws = rs.get_rendering_info(RenderingInfo::TOTAL_DRAW_CALLS_IN_FRAME);
            let new: [i64; 5] = std::array::from_fn(|i| pipelines[i] - self.pipelines[i]);
            let _ = writeln!(
                self.out,
                "{},{t:.3},{wall:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{render:.2},{draws},{nodes},{},{},{:.1},{memory:.1},{},{},{},{},{},{},{},\"{}\",\"{}\"",
                self.frame,
                f.work,
                p.input,
                p.pump,
                p.views,
                p.map,
                p.warm,
                p.preload,
                p.test,
                new.iter().sum::<i64>(),
                new.map(|n| n.to_string()).join(","),
                vmem - self.vmem,
                c.built,
                c.level,
                c.depth,
                c.branch,
                c.requests,
                c.state,
                c.veil,
                c.firsts,
                c.step.replace('"', "'"),
            );
        }
        self.frame += 1;
        // on disk now and then: a run killed at its timeout keeps its frames
        if self.frame.is_multiple_of(64) {
            let _ = self.out.flush();
        }
        self.pipelines = pipelines;
        self.vmem = vmem;
        self.now = Some(Frame {
            begun: now,
            work: 0.0,
            parts: Parts::default(),
            context: Context::default(),
        });
    }

    /// The frame's own work is done: its parts, and what it was about.
    pub fn end(&mut self, parts: Parts, context: Context) {
        if let Some(f) = self.now.as_mut() {
            f.work = f.begun.elapsed().as_secs_f64() * 1000.0;
            f.parts = parts;
            f.context = context;
        }
    }
}

impl Drop for FrameLog {
    fn drop(&mut self) {
        let _ = self.out.flush();
    }
}
