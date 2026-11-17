//! Auswertung von PresentMon-CSV-Dateien.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
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
    /// Tweaks, die während der Messung aktiv waren
    pub active: Vec<String>,
}

/// Spaltennamen der Frametime je nach PresentMon-Version.
const COLUMNS: [&str; 3] = ["msbetweenpresents", "frametime", "msbetweenappstart"];

/// Liest die Frametimes in Millisekunden aus einer PresentMon-CSV.
pub fn frametimes(csv: &str) -> Result<Vec<f64>, String> {
    let mut lines = csv.lines().filter(|l| !l.trim().is_empty());
    let header: Vec<String> = lines
        .next()
        .ok_or("CSV ist leer")?
        .split(',')
        .map(|h| h.trim().to_lowercase())
        .collect();
    let col = |name: &str| header.iter().position(|h| h == name);
    let single = COLUMNS.iter().find_map(|c| col(c));
    let pair = col("mscpubusy").zip(col("mscpuwait"));
    if single.is_none() && pair.is_none() {
        return Err("keine Frametime-Spalte in der PresentMon-Ausgabe gefunden".into());
    }
    let num = |cells: &[&str], i: usize| cells.get(i).and_then(|c| c.trim().parse::<f64>().ok());
    let mut out = Vec::new();
    for line in lines {
        let cells: Vec<&str> = line.split(',').collect();
        let ms = match (single, pair) {
            (Some(i), _) => num(&cells, i),
            (None, Some((a, b))) => num(&cells, a).zip(num(&cells, b)).map(|(x, y)| x + y),
            _ => None,
        };
        if let Some(ms) = ms.filter(|v| v.is_finite() && *v > 0.0) {
            out.push(ms);
        }
    }
    Ok(out)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let idx = ((sorted.len() as f64) * p).ceil() as usize;
    sorted[idx.clamp(1, sorted.len()) - 1]
}

/// (Durchschnitt, 1%-Low, 0,1%-Low) in FPS.
pub fn stats(frametimes: &[f64]) -> Option<(f64, f64, f64)> {
    if frametimes.len() < 100 {
        return None;
    }
    let mut sorted = frametimes.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    let round = |fps: f64| (fps * 10.0).round() / 10.0;
    Some((
        round(1000.0 / mean),
        round(1000.0 / percentile(&sorted, 0.99)),
        round(1000.0 / percentile(&sorted, 0.999)),
    ))
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
        assert!(frametimes("a,b\n1,2\n").is_err());
    }

    #[test]
    fn computes_lows() {
        // 990 Frames mit 5 ms, 10 Ausreißer mit 20 ms
        let mut ft = vec![5.0; 990];
        ft.extend(vec![20.0; 10]);
        let (avg, low1, low01) = stats(&ft).unwrap();
        assert_eq!(avg, 194.2);
        assert_eq!(low1, 200.0);
        assert_eq!(low01, 50.0);
        assert!(stats(&[5.0; 10]).is_none());
    }
}
