//! Hardware-Erkennung: ein PowerShell-Block, der zusammen mit den Tweak-Abfragen läuft.
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;

pub const HW_SCRIPT: &str = r#"
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
$mem = @(Get-CimInstance Win32_PhysicalMemory)
$os  = Get-CimInstance Win32_OperatingSystem
$cs  = Get-CimInstance Win32_ComputerSystem
$enc = Get-CimInstance Win32_SystemEnclosure | Select-Object -First 1
$vc  = @(Get-CimInstance Win32_VideoController)
$hw = @{
  cpu       = "$($cpu.Name)".Trim()
  cpu_vendor= "$($cpu.Manufacturer)"
  cores     = [int]$cpu.NumberOfCores
  threads   = [int]$cpu.NumberOfLogicalProcessors
  gpus      = @($vc | ForEach-Object { "$($_.Name)" })
  refresh   = [int]($vc | Measure-Object CurrentRefreshRate -Maximum).Maximum
  ram_gb    = [int][math]::Round((($mem | Measure-Object Capacity -Sum).Sum) / 1GB)
  ram_modules = [int]$mem.Count
  ram_speed = [int]($mem | Select-Object -First 1).ConfiguredClockSpeed
  build     = [int]$os.BuildNumber
  os        = "$($os.Caption)"
  vendor    = "$($cs.Manufacturer)"
  model     = "$($cs.Model)"
  chassis   = @($enc.ChassisTypes | ForEach-Object { [int]$_ })
  battery   = [int]@(Get-CimInstance Win32_Battery).Count
  disks     = @(Get-PhysicalDisk | ForEach-Object { @{ name = "$($_.FriendlyName)"; media = "$($_.MediaType)"; bus = "$($_.BusType)" } })
}
"#;

#[derive(Serialize, Clone, Debug, Default)]
pub struct Hardware {
    pub cpu: String,
    pub cores: u64,
    pub threads: u64,
    pub gpus: Vec<String>,
    pub ram_gb: u64,
    pub ram_modules: u64,
    pub ram_speed: u64,
    pub os: String,
    pub build: u64,
    pub model: String,
    pub refresh: u64,
    pub laptop: bool,
    pub disks: Vec<String>,
    pub flags: BTreeSet<String>,
}

fn s(v: &Value, k: &str) -> String {
    v.get(k)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}
fn n(v: &Value, k: &str) -> u64 {
    v.get(k).and_then(|x| x.as_u64()).unwrap_or(0)
}
/// PowerShell macht aus Ein-Element-Arrays gern Skalare; beides akzeptieren.
fn list(v: &Value, k: &str) -> Vec<Value> {
    match v.get(k) {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::Null) | None => Vec::new(),
        Some(other) => vec![other.clone()],
    }
}

const LAPTOP_CHASSIS: [u64; 11] = [8, 9, 10, 11, 12, 14, 18, 21, 30, 31, 32];

pub fn from_json(v: &Value) -> Hardware {
    let mut hw = Hardware {
        cpu: s(v, "cpu"),
        cores: n(v, "cores"),
        threads: n(v, "threads"),
        gpus: list(v, "gpus")
            .iter()
            .filter_map(|g| g.as_str().map(|x| x.trim().to_string()))
            .collect(),
        ram_gb: n(v, "ram_gb"),
        ram_modules: n(v, "ram_modules"),
        ram_speed: n(v, "ram_speed"),
        os: s(v, "os"),
        build: n(v, "build"),
        refresh: n(v, "refresh"),
        model: format!("{} {}", s(v, "vendor"), s(v, "model"))
            .trim()
            .to_string(),
        ..Default::default()
    };
    let mut f = BTreeSet::new();
    let cpu = hw.cpu.to_lowercase();
    let vendor = s(v, "cpu_vendor").to_lowercase();
    if vendor.contains("amd") || cpu.contains("amd") || cpu.contains("ryzen") {
        f.insert("cpu.amd".to_string());
    } else if vendor.contains("intel") || cpu.contains("intel") {
        f.insert("cpu.intel".to_string());
    }
    // Dual-CCD-X3D: 7900X3D, 7950X3D, 9900X3D, 9950X3D
    if ["7900x3d", "7950x3d", "9900x3d", "9950x3d"]
        .iter()
        .any(|m| cpu.contains(m))
    {
        f.insert("x3d_dual_ccd".to_string());
    }
    for g in &hw.gpus {
        let g = g.to_lowercase();
        if g.contains("nvidia") || g.contains("geforce") {
            f.insert("gpu.nvidia".to_string());
        } else if g.contains("radeon") || g.contains("amd") {
            f.insert("gpu.amd".to_string());
        } else if g.contains("intel") {
            f.insert("gpu.intel".to_string());
        }
    }
    let chassis: Vec<u64> = list(v, "chassis")
        .iter()
        .filter_map(|c| c.as_u64())
        .collect();
    hw.laptop = n(v, "battery") > 0 || chassis.iter().any(|c| LAPTOP_CHASSIS.contains(c));
    f.insert(if hw.laptop { "laptop" } else { "desktop" }.to_string());
    f.insert(if hw.build >= 22000 { "win11" } else { "win10" }.to_string());
    for d in list(v, "disks") {
        let media = s(&d, "media").to_lowercase();
        let bus = s(&d, "bus").to_lowercase();
        if media == "hdd" {
            f.insert("hdd".to_string());
        }
        if media == "ssd" {
            f.insert("ssd".to_string());
        }
        if bus == "nvme" {
            f.insert("nvme".to_string());
        }
        hw.disks.push(
            format!("{} ({} {})", s(&d, "name"), s(&d, "bus"), s(&d, "media")).replace("  ", " "),
        );
    }
    if hw.model.to_lowercase().contains("dell") {
        f.insert("dell".to_string());
    }
    hw.flags = f;
    hw
}

/// Prüft "flag" bzw. "!flag". Liefert das erste nicht erfüllte Flag.
pub fn unmet<'a>(requires: &'a [String], flags: &BTreeSet<String>) -> Option<&'a str> {
    requires
        .iter()
        .map(|r| r.as_str())
        .find(|r| match r.strip_prefix('!') {
            Some(neg) => flags.contains(neg),
            None => !flags.contains(*r),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_flags() {
        let v: Value = serde_json::from_str(
            r#"{"cpu":"AMD Ryzen 9 7950X3D 16-Core Processor","cpu_vendor":"AuthenticAMD","cores":16,"threads":32,
            "gpus":"NVIDIA GeForce RTX 4070","ram_gb":32,"ram_modules":2,"ram_speed":6000,"build":26200,"os":"Microsoft Windows 11 Pro",
            "vendor":"ASUS","model":"X","chassis":[3],"battery":0,"disks":{"name":"Samsung 990","media":"SSD","bus":"NVMe"}}"#,
        )
        .unwrap();
        let hw = from_json(&v);
        for f in [
            "cpu.amd",
            "x3d_dual_ccd",
            "gpu.nvidia",
            "desktop",
            "win11",
            "ssd",
            "nvme",
        ] {
            assert!(hw.flags.contains(f), "{f}");
        }
        assert!(unmet(&["!x3d_dual_ccd".to_string()], &hw.flags).is_some());
        assert!(unmet(&["win11".to_string(), "!laptop".to_string()], &hw.flags).is_none());
    }
}
