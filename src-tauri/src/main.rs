// Kein Konsolenfenster im Release-Build
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use dialed_core::{BenchRun, Engine, ScanResult, TweakView, WinSys};
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

fn main() {
    #[cfg(windows)]
    if std::env::args().any(|a| a == "--timer") {
        timer::hold();
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
            bench_history
        ])
        .run(tauri::generate_context!())
        .expect("Dialed konnte nicht gestartet werden");
}
