//! Dünne Schicht zum Betriebssystem. Alles Windows-spezifische liegt hier.
use crate::hardware::Display;
use crate::model::RegKind;
use std::path::PathBuf;

pub trait Sys: Send + Sync {
    fn reg_get(
        &self,
        hive: &str,
        path: &str,
        name: &str,
        kind: RegKind,
    ) -> Result<Option<String>, String>;
    fn reg_set(
        &self,
        hive: &str,
        path: &str,
        name: &str,
        kind: RegKind,
        value: &str,
    ) -> Result<(), String>;
    fn reg_delete(&self, hive: &str, path: &str, name: &str) -> Result<(), String>;
    fn reg_subkeys(&self, hive: &str, path: &str) -> Result<Vec<String>, String>;
    /// Startet ein Programm ohne Fenster, liefert stdout. Exit-Code != 0 ist ein Fehler.
    fn run(&self, exe: &str, args: &[&str]) -> Result<String, String>;
    /// Führt ein PowerShell-Skript aus, liefert stdout.
    fn ps(&self, script: &str) -> Result<String, String>;
    fn env(&self, name: &str) -> Option<String>;
    fn data_dir(&self) -> PathBuf;
    /// NVIDIA-Treibereinstellungen (Basisprofil) lesen. None = nicht gesetzt.
    fn nv_read(&self, app: Option<&str>, ids: &[u32]) -> Result<Vec<Option<u32>>, String>;
    /// NVIDIA-Treibereinstellung setzen (Some) oder auf Treiberstandard zurück (None).
    fn nv_write(&self, app: Option<&str>, id: u32, value: Option<u32>) -> Result<(), String>;
    fn displays(&self) -> Vec<Display>;
    fn nv_vrr(&self, displays: &[String]) -> Result<bool, String>;
}

pub struct WinSys;

#[cfg(windows)]
mod win {
    use super::*;
    use std::io::ErrorKind;
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use std::sync::atomic::{AtomicU32, Ordering};
    use winreg::enums::*;
    use winreg::RegKey;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn root(hive: &str) -> Result<RegKey, String> {
        match hive {
            "HKLM" => Ok(RegKey::predef(HKEY_LOCAL_MACHINE)),
            "HKCU" => Ok(RegKey::predef(HKEY_CURRENT_USER)),
            other => Err(format!("unbekannter Hive {other}")),
        }
    }

    fn exec(mut cmd: Command) -> Result<String, String> {
        let out = cmd
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| e.to_string())?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if out.status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let msg = if stderr.trim().is_empty() {
                stdout.trim().to_string()
            } else {
                stderr.trim().to_string()
            };
            Err(msg)
        }
    }

    impl Sys for WinSys {
        fn reg_get(
            &self,
            hive: &str,
            path: &str,
            name: &str,
            kind: RegKind,
        ) -> Result<Option<String>, String> {
            let key = match root(hive)?.open_subkey_with_flags(path, KEY_READ) {
                Ok(k) => k,
                Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e.to_string()),
            };
            let res = match kind {
                RegKind::Dword => key.get_value::<u32, _>(name).map(|v| v.to_string()),
                RegKind::Qword => key.get_value::<u64, _>(name).map(|v| v.to_string()),
                RegKind::Sz => key.get_value::<String, _>(name),
            };
            match res {
                Ok(v) => Ok(Some(v)),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e.to_string()),
            }
        }

        fn reg_set(
            &self,
            hive: &str,
            path: &str,
            name: &str,
            kind: RegKind,
            value: &str,
        ) -> Result<(), String> {
            let (key, _) = root(hive)?.create_subkey(path).map_err(|e| e.to_string())?;
            let num =
                || crate::parse::parse_u64(value).ok_or_else(|| format!("keine Zahl: {value}"));
            match kind {
                RegKind::Dword => key.set_value(name, &(num()? as u32)),
                RegKind::Qword => key.set_value(name, &num()?),
                RegKind::Sz => key.set_value(name, &value.to_string()),
            }
            .map_err(|e| e.to_string())
        }

        fn reg_delete(&self, hive: &str, path: &str, name: &str) -> Result<(), String> {
            let key = match root(hive)?.open_subkey_with_flags(path, KEY_SET_VALUE) {
                Ok(k) => k,
                Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(e.to_string()),
            };
            match key.delete_value(name) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }

        fn reg_subkeys(&self, hive: &str, path: &str) -> Result<Vec<String>, String> {
            match root(hive)?.open_subkey_with_flags(path, KEY_READ) {
                Ok(k) => Ok(k.enum_keys().filter_map(|r| r.ok()).collect()),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(Vec::new()),
                Err(e) => Err(e.to_string()),
            }
        }

        fn run(&self, exe: &str, args: &[&str]) -> Result<String, String> {
            let mut cmd = Command::new(exe);
            cmd.args(args);
            exec(cmd)
        }

        fn ps(&self, script: &str) -> Result<String, String> {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let file = self
                .data_dir()
                .join(format!("run-{}-{n}.ps1", std::process::id()));
            // BOM, damit Windows PowerShell 5.1 die Datei als UTF-8 liest
            let body =
                format!("\u{FEFF}[Console]::OutputEncoding=[Text.Encoding]::UTF8\r\n{script}\r\n");
            std::fs::write(&file, body).map_err(|e| e.to_string())?;
            let mut cmd = Command::new("powershell.exe");
            cmd.args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&file);
            let res = exec(cmd);
            let _ = std::fs::remove_file(&file);
            res
        }

        fn env(&self, name: &str) -> Option<String> {
            std::env::var(name).ok()
        }

        fn data_dir(&self) -> PathBuf {
            let base = std::env::var("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| std::env::temp_dir());
            let dir = base.join("Dialed");
            let _ = std::fs::create_dir_all(&dir);
            dir
        }

        fn nv_read(&self, app: Option<&str>, ids: &[u32]) -> Result<Vec<Option<u32>>, String> {
            crate::nv::read(app, ids)
        }

        fn nv_write(&self, app: Option<&str>, id: u32, value: Option<u32>) -> Result<(), String> {
            crate::nv::write(app, id, value)
        }

        fn displays(&self) -> Vec<Display> {
            use windows::core::PCWSTR;
            use windows::Win32::Graphics::Gdi::{
                EnumDisplayDevicesW, EnumDisplaySettingsW, DEVMODEW, DISPLAY_DEVICEW,
                DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, DISPLAY_DEVICE_PRIMARY_DEVICE,
                ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE,
            };
            let mut out = Vec::new();
            let devmode = || DEVMODEW {
                dmSize: std::mem::size_of::<DEVMODEW>() as u16,
                ..Default::default()
            };
            unsafe {
                for index in 0.. {
                    let mut dev = DISPLAY_DEVICEW {
                        cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
                        ..Default::default()
                    };
                    if !EnumDisplayDevicesW(PCWSTR::null(), index, &mut dev, 0).as_bool() {
                        break;
                    }
                    if (dev.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP).0 == 0 {
                        continue;
                    }
                    let name = PCWSTR(dev.DeviceName.as_ptr());
                    let mut current = devmode();
                    if !EnumDisplaySettingsW(name, ENUM_CURRENT_SETTINGS, &mut current).as_bool() {
                        continue;
                    }
                    let mut max = current.dmDisplayFrequency;
                    for mode_index in 0.. {
                        let mut mode = devmode();
                        if !EnumDisplaySettingsW(
                            name,
                            ENUM_DISPLAY_SETTINGS_MODE(mode_index),
                            &mut mode,
                        )
                        .as_bool()
                        {
                            break;
                        }
                        if mode.dmPelsWidth == current.dmPelsWidth
                            && mode.dmPelsHeight == current.dmPelsHeight
                        {
                            max = max.max(mode.dmDisplayFrequency);
                        }
                    }
                    let len = dev
                        .DeviceName
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(dev.DeviceName.len());
                    out.push(Display {
                        name: String::from_utf16_lossy(&dev.DeviceName[..len]),
                        width: current.dmPelsWidth,
                        height: current.dmPelsHeight,
                        current: current.dmDisplayFrequency,
                        max,
                        primary: (dev.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE).0 != 0,
                    });
                }
            }
            out
        }

        fn nv_vrr(&self, displays: &[String]) -> Result<bool, String> {
            crate::nv::vrr(displays)
        }
    }
}

/// Platzhalter, damit das Projekt auch außerhalb von Windows kompiliert (Tests, UI-Arbeit).
#[cfg(not(windows))]
impl Sys for WinSys {
    fn reg_get(&self, _: &str, _: &str, _: &str, _: RegKind) -> Result<Option<String>, String> {
        Err("nur unter Windows".into())
    }
    fn reg_set(&self, _: &str, _: &str, _: &str, _: RegKind, _: &str) -> Result<(), String> {
        Err("nur unter Windows".into())
    }
    fn reg_delete(&self, _: &str, _: &str, _: &str) -> Result<(), String> {
        Err("nur unter Windows".into())
    }
    fn reg_subkeys(&self, _: &str, _: &str) -> Result<Vec<String>, String> {
        Err("nur unter Windows".into())
    }
    fn run(&self, _: &str, _: &[&str]) -> Result<String, String> {
        Err("nur unter Windows".into())
    }
    fn ps(&self, _: &str) -> Result<String, String> {
        Err("nur unter Windows".into())
    }
    fn env(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
    fn data_dir(&self) -> PathBuf {
        std::env::temp_dir().join("dialed")
    }
    fn nv_read(&self, _: Option<&str>, _: &[u32]) -> Result<Vec<Option<u32>>, String> {
        Err("nur unter Windows".into())
    }
    fn nv_write(&self, _: Option<&str>, _: u32, _: Option<u32>) -> Result<(), String> {
        Err("nur unter Windows".into())
    }
    fn displays(&self) -> Vec<Display> {
        Vec::new()
    }
    fn nv_vrr(&self, _: &[String]) -> Result<bool, String> {
        Err("nur unter Windows".into())
    }
}
