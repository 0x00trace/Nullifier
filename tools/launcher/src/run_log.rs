//! Reading a run's log: frame-rate lines, world frames and their medians.

use std::path::Path;

#[derive(Clone, Default)]
pub(crate) struct LastRun {
    /// Median of the in-game frame lines (a few hundred draws or more).
    pub(crate) fps: Option<f32>,
    /// Median of menu/loading lines, kept to show what the menu runs at.
    pub(crate) menu_fps: Option<f32>,
    /// Whether any line was an in-game one (a few hundred draws or more).
    pub(crate) in_game: bool,
    pub(crate) frame: Option<f32>,
    pub(crate) copies_ms: Option<f32>,
    pub(crate) copies_mb: Option<f32>,
    pub(crate) textures_ms: Option<f32>,
    pub(crate) textures_mb: Option<f32>,
    pub(crate) gpu_ms: Option<f32>,
    pub(crate) back_ms: Option<f32>,
    pub(crate) heap_ms: Option<f32>,
    pub(crate) draws: Option<u32>,
    pub(crate) removals: u32,
}

pub(crate) fn median_fps(mut values: Vec<f32>) -> Option<f32> {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    match values.len() {
        0 => None,
        n if n % 2 == 1 => Some(values[n / 2]),
        n => Some((values[n / 2 - 1] + values[n / 2]) / 2.0),
    }
}

pub(crate) fn before<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    text.split_once(marker).map(|(left, _)| left)
}

pub(crate) fn after<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    text.split_once(marker).map(|(_, right)| right)
}

pub(crate) fn first_num(text: &str) -> Option<f32> {
    text.split_whitespace()
        .next()?
        .trim_end_matches(',')
        .parse()
        .ok()
}

pub(crate) fn last_num(text: &str) -> Option<f32> {
    let token = text.split_whitespace().last()?;
    token
        .trim_matches(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
        .parse()
        .ok()
}

pub(crate) fn megabytes(text: &str) -> Option<f32> {
    let first = text.split(';').next()?;
    let at = first.find(" MB")?;
    first[..at].split_whitespace().last()?.parse().ok()
}

/// One of the title's periodic frame lines, with what says whether the world
/// is loaded: draws, texture slots still waiting and the heap pass.
pub(crate) struct WorldFrame {
    pub(crate) fps: f32,
    draws: u32,
    waiting: Option<u32>,
    heap_ms: Option<f32>,
}

pub(crate) fn world_frames(text: &str) -> Vec<WorldFrame> {
    text.lines()
        .filter_map(|line| {
            let clean = strip_ansi(line);
            let rest = after(&clean, "host: ")?;
            let fps = before(rest, " fps,")?.trim().parse::<f32>().ok()?;
            let draws = before(rest, " draws/frame")
                .and_then(last_num)
                .map(|v| v as u32)?;
            let waiting = before(rest, " waiting,").and_then(last_num).map(|v| v as u32);
            let heap_ms = rest
                .split_once("heap buffers ")
                .and_then(|(_, tail)| tail.split_once(" passes "))
                .and_then(|(_, tail)| before(tail, " ms"))
                .and_then(last_num);
            Some(WorldFrame { fps, draws, waiting, heap_ms })
        })
        .collect()
}

/// World frames are those with 200 draws or more; menus have 13. Streaming
/// frames can have more, so the benchmark also waits for the slots waiting
/// and the heap pass to settle.
pub(crate) fn world_fps(text: &str) -> Vec<f32> {
    world_frames(text)
        .into_iter()
        .filter(world_loaded)
        .map(|f| f.fps)
        .collect()
}

pub(crate) fn world_loaded(frame: &WorldFrame) -> bool {
    frame.draws >= 200
        && frame.waiting.is_none_or(|waiting| waiting < 2_000)
        && frame.heap_ms.is_none_or(|ms| ms < 200.0)
}

pub(crate) fn median(values: &mut [f32]) -> f32 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    if n.is_multiple_of(2) {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    } else {
        values[n / 2]
    }
}

pub(crate) fn percentile(values: &[f32], share: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let at = ((sorted.len() as f32 * share) as usize).max(1) - 1;
    sorted[at.min(sorted.len() - 1)]
}

pub(crate) fn read_tail(path: &Path, max: usize) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(max as u64);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if start > 0 {
        // The first line is cut: drop it.
        Some(text.split_once('\n').map(|(_, rest)| rest.to_string()).unwrap_or(text))
    } else {
        Some(text)
    }
}

pub(crate) fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

pub(crate) fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut escape = false;
    for c in line.chars() {
        if escape {
            if c == 'm' {
                escape = false;
            }
        } else if c == '\u{1b}' {
            escape = true;
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_benchmark_waits_for_streaming_to_settle() {
        let line = |fps: f32, draws: u32, waiting: u32, heap: f32| {
            format!(
                "INFO victory_gpu::host: {fps} fps, 100.0 ms/frame (1.0 preparing; 1.0 on the GPU), {draws} draws/frame, dropped 0 without a host pipeline (0 being made) and 0 indirect; heap buffers 1.0 passes {heap} ms; textures picked by index: 10 uploaded so far (10 MB), {waiting} waiting, 0 passed over unmapped\n"
            )
        };
        let streaming = line(0.6, 810, 12_599, 2_423.0);
        let settled = line(10.8, 956, 1_282, 26.0);
        assert!(!world_loaded(&world_frames(&streaming)[0]), "streaming is not loaded");
        assert!(world_loaded(&world_frames(&settled)[0]), "settled world is loaded");
        let menu = "INFO victory_gpu::host: 30.0 fps, 33.3 ms/frame (1.0 preparing; 0.0 on the GPU), 13 draws/frame\n";
        assert_eq!(world_fps(&format!("{menu}{streaming}{settled}")), vec![10.8]);
    }
}
