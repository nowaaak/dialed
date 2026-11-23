//! Auswertung von PresentMon-CSV-Dateien.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BenchRun {
    pub at: u64,
    pub process: String,
    pub seconds: u32,
    pub frames: usize,
    pub avg_fps: f64,
    /// FPS am 99. Perzentil der Frametimes
    pub low1_fps: f64,
    /// FPS am 99,9. Perzentil der Frametimes
    pub low01_fps: f64,
    #[serde(default)]
    pub low1_avg_fps: Option<f64>,
    #[serde(default)]
    pub low01_avg_fps: Option<f64>,
    #[serde(default)]
    pub stutters: Option<usize>,
    #[serde(default)]
    pub present_mode: Option<String>,
    #[serde(default)]
    pub present_share: Option<f64>,
    #[serde(default)]
    pub gpu_busy: Option<f64>,
    #[serde(default)]
    pub capped: bool,
    #[serde(default)]
    pub swapchains: Option<usize>,
    /// Tweaks, die während der Messung aktiv waren
    pub active: Vec<String>,
}

/// Spaltennamen der Frametime je nach PresentMon-Version.
const COLUMNS: [&str; 3] = ["msbetweenpresents", "frametime", "msbetweenappstart"];
const MIN_FRAMES: usize = 300;
const MIN_TAIL: usize = 10;

#[derive(Debug, Default)]
pub struct Capture {
    pub frametimes: Vec<f64>,
    pub present_mode: Option<(String, f64)>,
    pub gpu_busy: Option<f64>,
    pub swapchains: usize,
}

#[derive(Default)]
struct Chain {
    frametimes: Vec<f64>,
    modes: HashMap<String, usize>,
    gpu_ratios: Vec<f64>,
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(values[values.len() / 2])
}

pub fn parse(csv: &str) -> Result<Capture, String> {
    let mut lines = csv.lines().filter(|l| !l.trim().is_empty());
    let header: Vec<String> = lines
        .next()
        .ok_or("CSV ist leer")?
        .split(',')
        .map(|h| h.trim().to_lowercase())
        .collect();
    let col = |name: &str| header.iter().position(|h| h == name);
    let single = COLUMNS.iter().find_map(|c| col(c));
    let pair = col("mscpubusy")
        .zip(col("mscpuwait"))
        .or_else(|| col("cpubusy").zip(col("cpuwait")));
    if single.is_none() && pair.is_none() {
        return Err("keine Frametime-Spalte in der PresentMon-Ausgabe gefunden".into());
    }
    let pid = col("processid");
    let swapchain = col("swapchainaddress");
    let mode = col("presentmode");
    let gpu = col("msgpubusy").or_else(|| col("gpubusy"));
    let num = |cells: &[&str], i: usize| cells.get(i).and_then(|c| c.trim().parse::<f64>().ok());
    let text = |cells: &[&str], i: Option<usize>| {
        i.and_then(|i| cells.get(i))
            .map(|c| c.trim().to_string())
            .unwrap_or_default()
    };
    let mut chains: HashMap<(String, String), Chain> = HashMap::new();
    for line in lines {
        let cells: Vec<&str> = line.split(',').collect();
        let ms = match (single, pair) {
            (Some(i), _) => num(&cells, i),
            (None, Some((a, b))) => num(&cells, a).zip(num(&cells, b)).map(|(x, y)| x + y),
            _ => None,
        };
        let Some(ms) = ms.filter(|v| v.is_finite() && *v > 0.0) else {
            continue;
        };
        let chain = chains
            .entry((text(&cells, pid), text(&cells, swapchain)))
            .or_default();
        chain.frametimes.push(ms);
        let m = text(&cells, mode);
        if !m.is_empty() {
            *chain.modes.entry(m).or_default() += 1;
        }
        if let Some(busy) = gpu.and_then(|i| num(&cells, i)).filter(|b| *b > 0.0) {
            chain.gpu_ratios.push(busy / ms);
        }
    }
    let swapchains = chains.len();
    let Some(mut main) = chains.into_values().max_by(|a, b| {
        a.frametimes
            .iter()
            .sum::<f64>()
            .total_cmp(&b.frametimes.iter().sum::<f64>())
    }) else {
        return Ok(Capture::default());
    };
    let counted: usize = main.modes.values().sum();
    let present_mode = main
        .modes
        .iter()
        .max_by_key(|(_, n)| **n)
        .map(|(m, n)| (m.clone(), *n as f64 / counted as f64));
    Ok(Capture {
        gpu_busy: median(&mut main.gpu_ratios),
        frametimes: main.frametimes,
        present_mode,
        swapchains,
    })
}

/// Liest die Frametimes in Millisekunden aus einer PresentMon-CSV.
pub fn frametimes(csv: &str) -> Result<Vec<f64>, String> {
    parse(csv).map(|c| c.frametimes)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let idx = ((sorted.len() as f64) * p).ceil() as usize;
    sorted[idx.clamp(1, sorted.len()) - 1]
}

#[derive(Debug, PartialEq)]
pub struct Stats {
    pub avg: f64,
    pub p1: f64,
    pub p01: f64,
    pub low1_avg: f64,
    pub low01_avg: Option<f64>,
    pub stutters: usize,
    pub median_fps: f64,
}

/// (Durchschnitt, 1%-Low, 0,1%-Low) in FPS.
pub fn stats(frametimes: &[f64]) -> Option<Stats> {
    if frametimes.len() < MIN_FRAMES {
        return None;
    }
    let mut sorted = frametimes.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    let mean = sorted.iter().sum::<f64>() / n as f64;
    let round = |fps: f64| (fps * 10.0).round() / 10.0;
    let tail_avg = |share: f64| {
        let k = ((n as f64) * share).ceil() as usize;
        let tail = &sorted[n - k.max(1)..];
        (k >= MIN_TAIL || share >= 0.01)
            .then(|| round(1000.0 / (tail.iter().sum::<f64>() / tail.len() as f64)))
    };
    Some(Stats {
        avg: round(1000.0 / mean),
        p1: round(1000.0 / percentile(&sorted, 0.99)),
        p01: round(1000.0 / percentile(&sorted, 0.999)),
        low1_avg: tail_avg(0.01).unwrap_or(0.0),
        low01_avg: tail_avg(0.001),
        stutters: frametimes.iter().filter(|ft| **ft > mean * 2.5).count(),
        median_fps: round(1000.0 / sorted[n / 2]),
    })
}

pub fn looks_capped(median_fps: f64, refresh: u64) -> bool {
    if refresh == 0 {
        return false;
    }
    [refresh as f64, refresh as f64 - 3.0]
        .iter()
        .any(|cap| (median_fps - cap).abs() <= cap * 0.02)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_formats() {
        let v1 = "Application,ProcessID,MsBetweenPresents\nx.exe,1,5.0\nx.exe,1,NA\nx.exe,1,10.0\n";
        assert_eq!(frametimes(v1).unwrap(), vec![5.0, 10.0]);
        let v2 = "Application,FrameTime,MsCPUBusy\nx.exe,4.0,1\n";
        assert_eq!(frametimes(v2).unwrap(), vec![4.0]);
        let v3 = "Application,MsCPUBusy,MsCPUWait\nx.exe,3.0,1.0\n";
        assert_eq!(frametimes(v3).unwrap(), vec![4.0]);
        let v4 = "Application,CPUBusy,CPUWait\nx.exe,3.0,2.0\n";
        assert_eq!(frametimes(v4).unwrap(), vec![5.0]);
        assert!(frametimes("a,b\n1,2\n").is_err());
    }

    #[test]
    fn keeps_only_the_main_swapchain() {
        let mut csv = String::from(
            "Application,ProcessID,SwapChainAddress,PresentMode,MsBetweenPresents,MsGPUBusy\n",
        );
        for _ in 0..100 {
            csv.push_str("g.exe,1,0xA,Hardware: Independent Flip,7.0,6.9\n");
        }
        for _ in 0..3 {
            csv.push_str("g.exe,1,0xB,Composed: Flip,100.0,1.0\n");
        }
        let c = parse(&csv).unwrap();
        assert_eq!(c.frametimes.len(), 100);
        assert_eq!(c.swapchains, 2);
        let (mode, share) = c.present_mode.unwrap();
        assert_eq!(mode, "Hardware: Independent Flip");
        assert_eq!(share, 1.0);
        assert!(c.gpu_busy.unwrap() > 0.95);
    }

    #[test]
    fn computes_lows() {
        // 990 Frames mit 5 ms, 10 Ausreißer mit 20 ms
        let mut ft = vec![5.0; 990];
        ft.extend(vec![20.0; 10]);
        let s = stats(&ft).unwrap();
        assert_eq!(s.avg, 194.2);
        assert_eq!(s.p1, 200.0);
        assert_eq!(s.low1_avg, 50.0);
        assert_eq!(s.p01, 50.0);
        assert_eq!(s.low01_avg, None);
        assert_eq!(s.stutters, 10);
        assert!(stats(&[5.0; 10]).is_none());
        let long = vec![5.0; 20_000];
        assert_eq!(stats(&long).unwrap().low01_avg, Some(200.0));
    }

    #[test]
    fn detects_caps() {
        assert!(looks_capped(141.0, 144));
        assert!(looks_capped(143.5, 144));
        assert!(!looks_capped(170.0, 144));
        assert!(!looks_capped(141.0, 0));
    }
}
