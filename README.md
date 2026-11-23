# Dialed

Windows-Tweaking-Tool mit Zustandserkennung. Jeder Tweak wird erkannt, angewendet und sauber zurückgesetzt. Bereits aktive Tweaks sind in der Oberfläche markiert, auch wenn sie nicht von Dialed gesetzt wurden.

- **Stack:** Rust + Tauri 2, Oberfläche ohne Build-Schritt (HTML, CSS, JS in `ui/`)
- **Tweaks:** deklarativ als TOML in `crates/core/tweaks/`, aktuell 128 Einträge in 11 Kategorien
- **Hardware:** CPU, GPU, RAM, Laufwerke, Notebook oder Desktop und Windows-Build werden beim Scan erkannt. Unpassende Tweaks sind ausgegraut.
- **Undo:** Vor dem Anwenden landet der echte Vorher-Wert im Journal (`%APPDATA%\Dialed\journal.json`). Zurücksetzen stellt genau diesen Wert wieder her.
- **Presets:** „Sicher“, „Competitive“ und „Notebook“ aktivieren mehrere Tweaks auf einmal.
- **Benchmark:** Misst ein laufendes Spiel mit Intel PresentMon und merkt sich, welche Tweaks dabei aktiv waren. Ausgewertet wird nur die Swapchain des Spiels. 1%- und 0,1%-Low sind der Durchschnitt der langsamsten Frames, der 0,1%-Wert erst ab 10 000 Frames. Dazu kommen Ruckler (Frames über dem 2,5-Fachen des Durchschnitts), Ausgabeweg und ob GPU, CPU oder ein FPS-Limit begrenzt. Dialed startet nur ein gültig von Intel signiertes PresentMon. Einmalig installieren: `winget install --id Intel.PresentMon.Console -e`
- **Timer-Dienst:** `dialed.exe --timer` hält als geplante Aufgabe die Timer-Auflösung auf 0,5 ms.
- **Audio-Latenz-Dienst:** `dialed.exe --audio` fordert beim Standard-Wiedergabegerät den kleinsten Puffer an, den der Treiber erlaubt. Laut Microsoft nutzen dann alle Programme auf diesem Gerät den kleineren Puffer.
- **Abweichung:** Setzt ein Windows- oder Treiber-Update einen Tweak zurück, zeigt die Oberfläche das an.

## Starten

Voraussetzungen: Rust (MSVC-Toolchain), WebView2 (bei Windows 11 dabei) und die Tauri-CLI.

```powershell
cargo install tauri-cli --version "^2" --locked   # einmalig
cargo tauri dev                                    # Terminal als Administrator starten
cargo tauri build                                  # Installer unter target\release\bundle\nsis
```

Die App fordert per Manifest Administratorrechte an, weil fast alle Tweaks HKLM, `bcdedit` oder `powercfg` brauchen. `cargo tauri dev` funktioniert deshalb nur aus einem Administrator-Terminal.

`check.cmd` im Projektordner prüft per Doppelklick, ob alles kompiliert, und schreibt das Ergebnis nach `build-log.txt`.

Tests der Kernlogik laufen auf jedem Betriebssystem:

```powershell
cargo test -p dialed-core
```

Die Oberfläche lässt sich ohne Tauri im Browser ansehen (`ui/index.html` über einen lokalen Webserver), sie nutzt dann die Beispieldaten aus `ui/mock.json`.

## Releases und Updates

Die installierte App prüft beim Start das neueste GitHub-Release und bietet in der Seitenleiste „Installieren“ an. Updates sind signiert, die App nimmt nur Pakete an, die zu ihrem Public Key passen.

Einmalige Einrichtung: Doppelklick auf `setup-release.cmd`. Das Skript nutzt deine `gh`-Anmeldung und

1. verschiebt den Signaturschlüssel nach `%USERPROFILE%\.tauri\dialed.key` (erzeugt wird er mit `cargo tauri signer generate`, der Public Key steht in `src-tauri/tauri.conf.json`),
2. legt den Workflow nach `.github/workflows/release.yml`,
3. erstellt das öffentliche Repo `dialed` und pusht,
4. hinterlegt die Secrets `TAURI_SIGNING_PRIVATE_KEY` und `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`,
5. taggt auf Wunsch das erste Release.

Das Repo muss öffentlich sein, sonst kann die App die Releases nicht laden.

Neue Version veröffentlichen: Version in `src-tauri/tauri.conf.json` und `src-tauri/Cargo.toml` erhöhen, `Cargo.lock` mit `cargo check -p dialed` nachziehen und auf `main` pushen.

```powershell
cargo check -p dialed
git commit -am "chore: release 0.2.0"
git push
```

Der Workflow `.github/workflows/release.yml` erkennt die neue Version, legt den Tag `v<Version>` an, baut den NSIS-Installer, lädt ihn samt `latest.json` ins Release hoch und veröffentlicht es. Für einen lokalen `cargo tauri build` müssen dieselben zwei Variablen als Umgebungsvariablen gesetzt sein.

## Einen Tweak hinzufügen

Ein neuer Block in einer Datei unter `crates/core/tweaks/` genügt, Code muss dafür nicht geändert werden:

```toml
[[tweak]]
id = "kernel.global_timer_resolution"
name = "Globale Timer-Resolution (Windows 11)"
category = "windows"          # windows | power | gpu | storage | network | input | debloat | fortnite | gta | games | hardware
description = "…"
evidence = "A"                # A belegt, B plausibel, C umstritten
risk = "low"                  # low | medium | high
reboot = true
requires = ["win11"]          # Flags, mit ! verneint: cpu.amd, cpu.intel, gpu.nvidia, gpu.amd, gpu.intel,
                              # laptop, desktop, win10, win11, hdd, ssd, nvme, x3d_dual_ccd, dell, fortnite,
                              # gta_legacy, gta_enhanced, minecraft, fivem, r6, overwatch, vrr (G-SYNC aktiv)
[[tweak.actions]]
type = "registry"             # registry | bcd | powercfg | ps | ini | xml | device_registry | nvidia_drs
hive = "HKLM"
path = 'SYSTEM\CurrentControlSet\Control\Session Manager\kernel'
name = "GlobalTimerResolutionRequests"
kind = "dword"
apply = 1                     # default weggelassen = Windows-Standard ist „Wert fehlt“
```

`kind = "check"` macht aus einem Eintrag eine reine Erkennung (BIOS, RAM), `kind = "oneshot"` eine einmalige Aktion.

`xml` ändert das Attribut `value` eines Elements, der Pfad führt über die Elementnamen (`path = "Settings/video/ReflexMode"`). In Dateipfaden steht `%DOCUMENTS%` für den echten Dokumente-Ordner, auch wenn er umgeleitet ist. `nvidia_drs` schreibt ohne `app` ins Basisprofil, mit `app = "GTA5.exe"` ins Treiberprofil dieser Anwendung. `apply = "refresh-3"` setzt die Bildwiederholrate des Hauptmonitors minus 3.

PowerShell-Skripte bekommen von Dialed vier Umgebungsvariablen: `DIALED_REFRESH` (Bildwiederholrate des Hauptmonitors), `DIALED_DISPLAYS` (alle Monitore als JSON mit aktueller und höchster Rate), `DIALED_VRR` (`1`, wenn der NVIDIA-Treiber G-SYNC als aktiv meldet) und `DIALED_AUDIO` (Standard-Wiedergabegerät als JSON mit Name, Anschluss, Puffergrößen und abweichendem Kommunikationsgerät). Die Werte kommen aus der Windows-Anzeige-API und aus NVAPI, nicht aus WMI, das die Rate abrundet.

## Aufbau

```
crates/core      Engine: Katalog, Erkennung, Anwenden, Zurücksetzen, Journal
  tweaks/        Tweak-Definitionen
src-tauri        Fenster und Tauri-Befehle
ui               Oberfläche
```

## Hinweise

- Vor größeren Änderungen in der App einen Wiederherstellungspunkt anlegen.
- GTA V: Dialed fasst nur die `settings.xml` im Dokumente-Ordner und das NVIDIA-Treiberprofil des Spiels an. Mods und alles, was in den Spielprozess eingreift, bleiben wegen BattlEye draußen.
- Fortnite: Dialed fasst nur Windows-Einstellungen und die `GameUserSettings.ini` an. Nichts greift in den laufenden Prozess ein. Die Prioritäts-Tweaks über Image File Execution Options sind von Epic nicht offiziell freigegeben. „Fortnite ohne CPU 0“ fehlt bewusst: Launcher und Anti-Cheat-Starter erben die Zuweisung, das Spiel selbst läuft aber wieder auf allen Kernen (gemessen mit `GetProcessAffinityMask`).
- Schrift: Geist (SIL Open Font License, siehe `ui/fonts/LICENSE-Geist.txt`).
