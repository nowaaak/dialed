// Kein Konsolenfenster im Release-Build
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use dialed_core::{BenchComparison, BenchRun, Engine, ScanResult, TweakView, WinSys};
use serde::Serialize;
use tauri::State;
use tauri_plugin_updater::UpdaterExt;

type Eng = Engine<WinSys>;

#[tauri::command]
async fn scan(engine: State<'_, Eng>) -> Result<ScanResult, String> {
    engine.scan()
}

#[tauri::command]
async fn apply(engine: State<'_, Eng>, id: String) -> Result<TweakView, String> {
    engine.apply(&id)
}

#[tauri::command]
async fn revert(engine: State<'_, Eng>, id: String) -> Result<TweakView, String> {
    engine.revert(&id)
}

#[tauri::command]
async fn run_oneshot(engine: State<'_, Eng>, id: String) -> Result<String, String> {
    engine.run_oneshot(&id)
}

#[tauri::command]
async fn restore_point(engine: State<'_, Eng>) -> Result<(), String> {
    engine.restore_point()
}

#[tauri::command]
async fn benchmark(
    engine: State<'_, Eng>,
    process: String,
    seconds: u32,
) -> Result<BenchRun, String> {
    engine.benchmark(&process, seconds)
}

#[tauri::command]
async fn bench_history(engine: State<'_, Eng>) -> Result<Vec<BenchRun>, String> {
    Ok(engine.bench_history())
}

#[tauri::command]
async fn bench_compare(
    engine: State<'_, Eng>,
    a: Vec<u64>,
    b: Vec<u64>,
) -> Result<BenchComparison, String> {
    engine.bench_compare(&a, &b)
}

#[derive(Serialize)]
struct UpdateInfo {
    version: String,
    notes: Option<String>,
}

/// Fragt das neueste GitHub-Release ab. None = App ist aktuell.
#[tauri::command]
async fn check_update(app: tauri::AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| e.to_string())?;
    Ok(update.map(|u| UpdateInfo {
        version: u.version.clone(),
        notes: u.body.clone(),
    }))
}

/// Lädt das Update, prüft die Signatur, startet den Installer und danach die App neu.
#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    if let Some(update) = updater.check().await.map_err(|e| e.to_string())? {
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|e| e.to_string())?;
        app.restart();
    }
    Ok(())
}

/// Headless-Modus `dialed.exe --timer`: hält die Timer-Auflösung auf 0,5 ms, ohne Fenster.
#[cfg(windows)]
mod timer {
    use std::ffi::c_void;

    #[repr(C)]
    struct PowerThrottlingState {
        version: u32,
        control_mask: u32,
        state_mask: u32,
    }

    #[link(name = "ntdll")]
    extern "system" {
        fn NtSetTimerResolution(desired: u32, set: u8, current: *mut u32) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn FreeConsole() -> i32;
        fn SetProcessInformation(process: isize, class: i32, info: *const c_void, size: u32)
            -> i32;
    }

    pub fn hold() -> ! {
        unsafe {
            // Debug-Builds haben ein Konsolenfenster, das hier niemand braucht.
            FreeConsole();
            // Windows 11 ignoriert Timer-Anfragen von Prozessen ohne sichtbares Fenster,
            // außer der Prozess schaltet diese Drosselung für sich ab.
            let state = PowerThrottlingState {
                version: 1,
                control_mask: 0x4,
                state_mask: 0,
            };
            SetProcessInformation(
                GetCurrentProcess(),
                4, // ProcessPowerThrottling
                &state as *const PowerThrottlingState as *const c_void,
                std::mem::size_of::<PowerThrottlingState>() as u32,
            );
            let mut current = 0u32;
            NtSetTimerResolution(5000, 1, &mut current); // Einheit: 100 ns
        }
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }
}

#[cfg(windows)]
mod audio {
    use std::time::Duration;
    use windows::core::Result;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioClient3, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    #[link(name = "kernel32")]
    extern "system" {
        fn FreeConsole() -> i32;
    }

    pub fn hold() -> ! {
        unsafe {
            FreeConsole();
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        loop {
            let _ = keep_small_buffer();
            std::thread::sleep(Duration::from_secs(5));
        }
    }

    fn device_id(device: &IMMDevice) -> Result<String> {
        unsafe {
            let raw = device.GetId()?;
            let id = raw.to_string().unwrap_or_default();
            CoTaskMemFree(Some(raw.0 as _));
            Ok(id)
        }
    }

    fn keep_small_buffer() -> Result<()> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let id = device_id(&device)?;
            let client: IAudioClient3 = device.Activate(CLSCTX_ALL, None)?;
            let format = client.GetMixFormat()?;
            let (mut default, mut fundamental, mut min, mut max) = (0, 0, 0, 0);
            let supported = client
                .GetSharedModeEnginePeriod(
                    format,
                    &mut default,
                    &mut fundamental,
                    &mut min,
                    &mut max,
                )
                .is_ok()
                && min > 0
                && min < default;
            let started = supported
                && client
                    .InitializeSharedAudioStream(0, min, format, None)
                    .and_then(|_| client.Start())
                    .is_ok();
            CoTaskMemFree(Some(format as _));
            loop {
                std::thread::sleep(Duration::from_secs(2));
                let current = enumerator
                    .GetDefaultAudioEndpoint(eRender, eConsole)
                    .and_then(|d| device_id(&d));
                if current.ok().as_deref() != Some(id.as_str()) {
                    break;
                }
                if started && client.GetCurrentPadding().is_err() {
                    break;
                }
            }
            if started {
                let _ = client.Stop();
            }
            Ok(())
        }
    }
}

fn main() {
    #[cfg(windows)]
    if std::env::args().any(|a| a == "--timer") {
        timer::hold();
    }
    #[cfg(windows)]
    if std::env::args().any(|a| a == "--audio") {
        audio::hold();
    }
    // PowerShell-Aktionen (z. B. der Timer-Dienst) brauchen den Pfad dieser Exe.
    if let Ok(exe) = std::env::current_exe() {
        std::env::set_var("DIALED_EXE", exe);
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Engine::new(WinSys))
        .invoke_handler(tauri::generate_handler![
            scan,
            apply,
            revert,
            run_oneshot,
            restore_point,
            check_update,
            install_update,
            benchmark,
            bench_history,
            bench_compare
        ])
        .run(tauri::generate_context!())
        .expect("Dialed konnte nicht gestartet werden");
}
