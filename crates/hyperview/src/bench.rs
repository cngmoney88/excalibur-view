//! `Hyperview.exe --benchmark drawing.pdf`: how fast it really is, measured.
//!
//! Opens the drawing, lets it settle, then drives the window the way a person
//! does — sits still, pans, zooms in and out, flips through the sheets — and
//! times every frame, with the screen's refresh rate taken out of the way so
//! the numbers are the program's and not the monitor's. It also times each
//! part of the window and every tile the PDF engine draws.
//!
//! The results are written to `hyperview-benchmark.txt` beside the drawing,
//! and the program closes. The same run on two builds says which is faster
//! and by how much, which is the only kind of "faster" worth saying.

use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    Loading,
    Idle,
    Pan,
    Zoom,
    Flip,
    Done,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Phase::Loading => "loading",
            Phase::Idle => "sitting still",
            Phase::Pan => "panning",
            Phase::Zoom => "zooming",
            Phase::Flip => "flipping sheets",
            Phase::Done => "done",
        }
    }
}

/// Set by `--benchmark` before the window opens.
pub static REQUESTED: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub struct Bench {
    pub drawing: PathBuf,
    started: Instant,
    phase: Phase,
    phase_frames: u32,
    last_frame: Option<Instant>,
    /// (phase, whole frame ms, update ms)
    frames: Vec<(Phase, f64, f64)>,
    /// Time in each part of the window, summed, per phase.
    parts: Vec<(Phase, &'static str, f64)>,
    tiles: Vec<f64>,
    opened: Option<Instant>,
    first_sheet_sharp: Option<f64>,
    sheets: u32,
    /// Waiting for the screen to be sharp again, since when and from which
    /// frame of the phase.
    waiting_since: Option<Instant>,
    waiting_from: u32,
    /// When the screen last went sharp.
    sharp_at: Option<Instant>,
    /// How long a person looks at a sheet before turning to the next.
    dwell: Duration,
    rounds: u32,
    /// (phase, ms until sharp again)
    settled: Vec<(Phase, f64)>,
    /// Sheets' line-work read for snapping: (lines, ms).
    lines: Vec<(usize, f64)>,
    /// Snap queries: (ms, caught something).
    snaps: Vec<(f64, bool)>,
}

/// Times the parts of one frame.
pub struct Marks {
    at: Instant,
    pub done: Vec<(&'static str, f64)>,
}

impl Marks {
    pub fn start() -> Marks {
        Marks { at: Instant::now(), done: Vec::new() }
    }
    pub fn mark(&mut self, part: &'static str) {
        let now = Instant::now();
        self.done.push((part, (now - self.at).as_secs_f64() * 1000.0));
        self.at = now;
    }
}

pub fn mark(marks: &mut Option<Marks>, part: &'static str) {
    if let Some(m) = marks.as_mut() {
        m.mark(part);
    }
}

/// What the benchmark wants done to the window this frame.
pub enum Action {
    Nothing,
    Pan(egui::Vec2),
    Zoom(f32),
    NextSheet,
    Finish,
}

impl Bench {
    pub fn new(drawing: PathBuf) -> Bench {
        Bench {
            drawing,
            started: Instant::now(),
            phase: Phase::Loading,
            phase_frames: 0,
            last_frame: None,
            frames: Vec::new(),
            parts: Vec::new(),
            tiles: Vec::new(),
            opened: None,
            first_sheet_sharp: None,
            sheets: 0,
            waiting_since: None,
            waiting_from: 0,
            sharp_at: None,
            dwell: Duration::from_millis(800),
            rounds: 0,
            settled: Vec::new(),
            lines: Vec::new(),
            snaps: Vec::new(),
        }
    }

    pub fn opened(&mut self, sheets: u32) {
        if self.opened.is_none() {
            self.opened = Some(Instant::now());
            self.sheets = sheets;
        }
    }

    /// A tile arrived. While the drawing is first loading, the last tile to
    /// arrive is when the first sheet finished going sharp.
    pub fn tile(&mut self, millis: f64) {
        self.tiles.push(millis);
    }

    pub fn lines_read(&mut self, lines: usize, millis: f64) {
        self.lines.push((lines, millis));
    }

    pub fn snapped(&mut self, millis: f64, caught: bool) {
        self.snaps.push((millis, caught));
    }

    pub fn is_open(&self) -> bool {
        self.opened.is_some()
    }

    /// Called at the start of every frame with how many tiles the screen is
    /// still waiting for. Says what to do to the window.
    ///
    /// Two things are measured. How long each frame takes while the window is
    /// being moved — that is smoothness. And, after each sheet flip, burst of
    /// zooming and drag, how long until the screen is sharp again — that is
    /// what waiting on the program feels like.
    pub fn begin_frame(&mut self, missing: usize) -> Action {
        let now = Instant::now();
        self.last_frame = Some(now);
        self.phase_frames += 1;
        let n = self.phase_frames;

        // Waiting for the screen to go sharp after something happened.
        if let Some(since) = self.waiting_since {
            let waited = now - since;
            if (missing == 0 && n > self.waiting_from + 1) || waited > Duration::from_secs(6) {
                self.settled.push((self.phase, waited.as_secs_f64() * 1000.0));
                self.waiting_since = None;
                self.sharp_at = Some(now);
                self.rounds += 1;
            } else {
                return Action::Nothing;
            }
        }

        let (next, action) = match self.phase {
            Phase::Loading => {
                if missing == 0 && self.first_sheet_sharp.is_none() && n > 2 {
                    if let Some(at) = self.opened {
                        self.first_sheet_sharp = Some(at.elapsed().as_secs_f64() * 1000.0);
                    }
                }
                let settled = self.opened.is_some_and(|at| at.elapsed() > Duration::from_millis(500))
                    && missing == 0;
                if settled || self.started.elapsed() > Duration::from_secs(15) {
                    (Some(Phase::Idle), Action::Nothing)
                } else {
                    (None, Action::Nothing)
                }
            }
            Phase::Idle => (next_after(n, 120, Phase::Pan), Action::Nothing),
            Phase::Pan => {
                // A drag of 40 frames, then a wait for sharp; four times.
                if self.rounds >= 4 {
                    (Some(Phase::Zoom), Action::Nothing)
                } else if n % 41 == 0 {
                    self.wait(now, n);
                    (None, Action::Nothing)
                } else {
                    let step = if self.rounds % 2 == 0 { 16.0 } else { -16.0 };
                    (None, Action::Pan(egui::vec2(step, step * 0.4)))
                }
            }
            Phase::Zoom => {
                // A spin of the wheel — six notches — then a wait for sharp.
                // In, out, in, out, in, out.
                if self.rounds >= 6 {
                    (Some(Phase::Flip), Action::Nothing)
                } else if n % 7 == 0 {
                    self.wait(now, n);
                    (None, Action::Nothing)
                } else {
                    let factor = if self.rounds % 2 == 0 { 1.15 } else { 1.0 / 1.15 };
                    (None, Action::Zoom(factor))
                }
            }
            Phase::Flip => {
                // A person looks at a sheet for a moment before turning to the
                // next; that moment is part of how the program is used, so it
                // is part of the test. Then: how long until the next is sharp.
                let flips = self.sheets.clamp(1, 20);
                let looked = self.sharp_at.is_some_and(|at| at.elapsed() >= self.dwell);
                if self.rounds >= flips {
                    (Some(Phase::Done), Action::Nothing)
                } else if self.rounds == 0 && self.sharp_at.is_none() {
                    self.sharp_at = Some(now);
                    (None, Action::Nothing)
                } else if looked {
                    self.sharp_at = None;
                    self.wait(now, n);
                    (None, Action::NextSheet)
                } else {
                    (None, Action::Nothing)
                }
            }
            Phase::Done => (None, Action::Finish),
        };
        if let Some(phase) = next {
            self.phase = phase;
            self.phase_frames = 0;
            self.rounds = 0;
        }
        action
    }

    fn wait(&mut self, now: Instant, frame: u32) {
        self.waiting_since = Some(now);
        self.waiting_from = frame;
    }

    /// Called at the end of every frame with how long the program's own part
    /// of it took and where that went.
    pub fn end_frame(&mut self, update: Duration, marks: Option<Marks>) {
        let update_ms = update.as_secs_f64() * 1000.0;
        if let Some(marks) = marks {
            for (part, ms) in marks.done {
                match self.parts.iter_mut().find(|(p, n, _)| *p == self.phase && *n == part) {
                    Some(slot) => slot.2 += ms,
                    None => self.parts.push((self.phase, part, ms)),
                }
            }
        }
        self.frames.push((self.phase, 0.0, update_ms));
    }

    /// Records the whole frame time for the frame before this one, measured
    /// between the starts of consecutive frames.
    pub fn frame_interval(&mut self, interval: Duration) {
        if let Some(last) = self.frames.last_mut() {
            last.1 = interval.as_secs_f64() * 1000.0;
        }
    }

    pub fn report(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "EXCALIBUR HYPERVIEW {} BENCHMARK\n\n",
            env!("CARGO_PKG_VERSION")
        ));
        out.push_str(&format!("drawing   {}\n", self.drawing.display()));
        out.push_str(&format!("sheets    {}\n", self.sheets));
        if let Some(ms) = self.first_sheet_sharp {
            out.push_str(&format!("first sheet sharp after {ms:.0} ms\n"));
        }
        out.push_str(&format!(
            "tiles     {} drawn, {:.1} ms median, {:.1} ms 95th, {:.1} ms worst\n\n",
            self.tiles.len(),
            percentile(&self.tiles, 0.5),
            percentile(&self.tiles, 0.95),
            percentile(&self.tiles, 1.0)
        ));
        out.push_str("                   frames   whole frame ms (median / 95th / worst)   program ms (median / 95th)   fps\n");
        for phase in [Phase::Idle, Phase::Pan, Phase::Zoom, Phase::Flip] {
            let whole: Vec<f64> = self
                .frames
                .iter()
                .filter(|(p, w, _)| *p == phase && *w > 0.0)
                .map(|(_, w, _)| *w)
                .collect();
            let program: Vec<f64> =
                self.frames.iter().filter(|(p, _, _)| *p == phase).map(|(_, _, u)| *u).collect();
            if program.is_empty() {
                continue;
            }
            let median = percentile(&whole, 0.5);
            out.push_str(&format!(
                "{:<18} {:>6}   {:>8.2} / {:>6.2} / {:>7.2}                {:>7.2} / {:>6.2}          {:>5.0}\n",
                phase.name(),
                program.len(),
                median,
                percentile(&whole, 0.95),
                percentile(&whole, 1.0),
                percentile(&program, 0.5),
                percentile(&program, 0.95),
                if median > 0.0 { 1000.0 / median } else { 0.0 },
            ));
        }
        if !self.lines.is_empty() {
            let most = self.lines.iter().map(|(n, _)| *n).max().unwrap_or(0);
            let ms: Vec<f64> = self.lines.iter().map(|(_, ms)| *ms).collect();
            out.push_str(&format!(
                "snapping  line-work read on {} sheet(s), up to {} lines, {:.0} ms median, {:.0} ms worst\n",
                self.lines.len(),
                most,
                percentile(&ms, 0.5),
                percentile(&ms, 1.0)
            ));
        }
        if !self.snaps.is_empty() {
            let ms: Vec<f64> = self.snaps.iter().map(|(ms, _)| *ms).collect();
            let caught = self.snaps.iter().filter(|(_, c)| *c).count();
            out.push_str(&format!(
                "          finding a snap: {:.3} ms median, {:.3} ms worst, over {} looks ({} caught something)\n",
                percentile(&ms, 0.5),
                percentile(&ms, 1.0),
                ms.len(),
                caught
            ));
        }
        out.push_str("\nsharp again after (ms)       median     worst   times\n");
        for phase in [Phase::Pan, Phase::Zoom, Phase::Flip] {
            let waits: Vec<f64> =
                self.settled.iter().filter(|(p, _)| *p == phase).map(|(_, ms)| *ms).collect();
            if waits.is_empty() {
                continue;
            }
            let label = match phase {
                Phase::Pan => "a drag",
                Phase::Zoom => "a spin of the wheel",
                _ => "a sheet flip",
            };
            out.push_str(&format!(
                "  {:<24} {:>8.0}  {:>8.0}   {}\n",
                label,
                percentile(&waits, 0.5),
                percentile(&waits, 1.0),
                waits.len()
            ));
        }
        out.push_str("\nwhere the program's time goes, ms per frame on average\n");
        for phase in [Phase::Idle, Phase::Pan, Phase::Zoom, Phase::Flip] {
            let frames = self.frames.iter().filter(|(p, _, _)| *p == phase).count().max(1) as f64;
            let mut parts: Vec<&(Phase, &str, f64)> =
                self.parts.iter().filter(|(p, _, _)| *p == phase).collect();
            parts.sort_by(|a, b| b.2.total_cmp(&a.2));
            let listed: Vec<String> = parts
                .iter()
                .take(6)
                .map(|(_, name, ms)| format!("{name} {:.2}", ms / frames))
                .collect();
            out.push_str(&format!("  {:<16} {}\n", phase.name(), listed.join(", ")));
        }
        out
    }

    pub fn done(&self) -> bool {
        self.phase == Phase::Done
    }

    pub fn write(&self) -> Option<PathBuf> {
        let at = self
            .drawing
            .parent()
            .map(|d| d.join("hyperview-benchmark.txt"))
            .unwrap_or_else(|| PathBuf::from("hyperview-benchmark.txt"));
        let text = self.report();
        println!("{text}");
        std::fs::write(&at, &text).ok().map(|_| at)
    }
}

fn next_after(n: u32, frames: u32, next: Phase) -> Option<Phase> {
    (n >= frames).then_some(next)
}

pub fn percentile(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let at = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[at.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_are_read_off_the_sorted_values() {
        let v = [5.0, 1.0, 3.0, 2.0, 4.0];
        assert_eq!(percentile(&v, 0.5), 3.0);
        assert_eq!(percentile(&v, 1.0), 5.0);
        assert_eq!(percentile(&v, 0.0), 1.0);
        assert_eq!(percentile(&[], 0.5), 0.0);
    }

    #[test]
    fn the_run_goes_through_every_phase_and_then_finishes() {
        let mut bench = Bench::new(PathBuf::from("/tmp/x.pdf"));
        bench.dwell = Duration::ZERO;
        bench.opened(3);
        bench.opened = Some(Instant::now() - Duration::from_secs(5));
        let mut seen = Vec::new();
        for _ in 0..2000 {
            if !seen.contains(&bench.phase) {
                seen.push(bench.phase);
            }
            if let Action::Finish = bench.begin_frame(0) {
                break;
            }
            bench.end_frame(Duration::from_millis(1), None);
        }
        assert_eq!(
            seen,
            vec![Phase::Loading, Phase::Idle, Phase::Pan, Phase::Zoom, Phase::Flip, Phase::Done]
        );
    }
}
