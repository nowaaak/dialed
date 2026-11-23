# Recherche Runde 2 (November 2026)

Fünf Recherchen zu Spielen, Windows 11, Messmethodik, Hardware und Audio. Alles mit A/B/C-Einordnung wie im Katalog. Was in diesem Branch umgesetzt ist, steht unter „Umgesetzt“. Der Rest ist nach Aufwand sortiert und mit Begründung festgehalten.

## Umgesetzt

### Benchmark
- Nur noch die Swapchain des Spiels wird ausgewertet. Vorher wurden alle Swapchains und Prozesse mit gleichem Namen vermischt.
- 1%- und 0,1%-Low sind jetzt der Durchschnitt der langsamsten Frames (wie CapFrameX „low average“). Das alte Perzentil hat Ruckler verschluckt: 10 Frames mit 20 ms unter 1000 Frames ergaben 200 FPS „1%-Low“ statt 50.
- 0,1%-Low erst ab 10 000 Frames, darunter beruht der Wert auf zu wenigen Frames.
- Neu: Ruckler (Frames über dem 2,5-Fachen des Durchschnitts), Ausgabeweg laut PresentMon, Anteil GPU-Busy (GPU- oder CPU-Limit), Erkennung eines FPS-Limits.
- 5 Sekunden Vorlauf (`--delay 5`), damit der Wechsel ins Spiel nicht mitgemessen wird. Feste Session `DialedBench`.
- **Sicherheit:** PresentMon wurde mit Adminrechten aus `%APPDATA%` und dem WinGet-Paketordner gestartet. In beide darf jedes Programm schreiben, das war ein Weg an der Benutzerkontensteuerung vorbei. Jetzt startet Dialed nur ein gültig von Intel signiertes PresentMon.
- Unterschiede zwischen Einzelläufen werden nicht mehr grün oder rot gefärbt. Ein einzelner Lauf schwankt um einige Prozent.

### Neue Prüfungen
| ID | Einordnung | Bemerkung |
|---|---|---|
| `audio.bluetooth_output_check` | B | Standardausgabe über Bluetooth (100 bis 300 ms) |
| `audio.comm_device_check` | A | Kommunikationsgerät weicht ab |
| `audio.small_buffer_check` | A | Zeigt, ob der Audio-Latenz-Dienst überhaupt wirken kann |
| `sec.exploit_protection_check` | A | CFG/DEP systemweit aus bricht Vanguard (VAN 9002) |
| `sec.mitigations_check` | A | Spectre/Meltdown-Override erkannt |
| `sec.kernel_stack_eac_check` | A | Kernel-Stapelschutz kann EAC blockieren |
| `cpu.boost_policy` | A | Boost im Energieplan aus oder Maximaltakt unter 100 % |
| `gpu.driver_check` | B | Basistreiber, NVIDIA unter 581.94, Treiber älter als ein Jahr |
| `cpu.cppc_check` | B | Bevorzugte Kerne aus Ereignis 55 |
| `cpu.firmware_limit_check` | B | Firmware-Drosselung (Ereignis 35/37) |
| `cpu.amd_chipset_check` | B | AMD Chipset Software fehlt |
| `mem.same_modules_check` | B | Gemischte RAM-Riegel |
| `mem.diagnostics_check` | A | Ergebnis der Windows-Speicherdiagnose |
| `sys.error_events_check` | B | Grafiktreiber-Abstürze und Laufwerksfehler in 14 Tagen |
| `fw.am4_agesa_check` | B | AM4-BIOS vor den Fixes für fTPM-Ruckler und USB-Aussetzer |
| `fw.secureboot_ca2023_check` | A | Secure-Boot-Zertifikate 2023 getauscht |
| `net.link_speed_check` | A | Kabelverbindung unter 1 Gbit/s |
| `storage.games_on_ssd_check` | A | Steam- und Epic-Bibliotheken auf Festplatte |
| `storage.system_drive_check` | B | Windows auf SATA, obwohl NVMe vorhanden |
| `fn.reflex_check` | B | Reflex in Fortnite (Zuordnung von `LatencyTweak2` aus der Community) |
| `fivem.display_check` | B | Auflösung und Rate in FiveM |
| `ow.display_check` | B | Bildwiederholrate in Overwatch 2 |

### Neue Einträge
- `dotnet.ngen_flush` (A, Aktion): .NET-Neuübersetzung nach Updates sofort erledigen statt mitten im Spiel.
- `gpu.shader_cache_clear` (B, Aktion): nur bei Problemen, löst danach Neuübersetzungs-Ruckler aus.
- `r6.fps_cap` (A): `FPSLimit` in der `GameSettings.ini`, von Ubisoft dokumentiert.

### Korrekturen
- `hw.dual_channel` prüft jetzt die Kanäle, nicht nur die Anzahl der Riegel.
- `hw.xmp` prüft alle Riegel und warnt nicht mehr pauschal bei JEDEC-Kits.
- `prio.win32_2a`: Die Beschreibung versprach einen 3:1-Vorteil. Bei festen Zeitscheiben greift der Vordergrund-Bonus nicht.
- `sec.hvci_off`: typisch etwa 5 % (ComputerBase, 24H2), Anforderungen von Vanguard und FACEIT, Rückkehr per Windows-Update ab Oktober 2026.
- `debloat.telemetry`: Bei Home und Pro bedeutet der Wert 0 „Erforderlich“, nicht „aus“. Warnung für Insider-Builds.
- `gfx.mpo_check` von A auf B, die Wirkung betrifft nur Overlays über dem Spiel.
- `win.game_mode_check`: Spielmodus pausiert auch den Suchindex.
- `storage.bitlocker_check` von 5,4 s auf 0,08 s, gleiches Ergebnis über die Shell-Eigenschaft statt `Get-BitLockerVolume`.

## Offen, sinnvoll, größerer Aufwand

| Idee | Warum | Aufwand |
|---|---|---|
| A/B-Vergleich mehrerer Läufe mit Konfidenzintervall | Erst damit lässt sich sagen, ob ein Tweak wirkt. Statistik auf Laufebene (Welch-t), nie auf Frame-Ebene | M |
| Geführter A/B-Test im Wechsel ABAB | Gleicht Temperatur und Shader-Cache aus | M bis L |
| PresentMon fest mitliefern (MIT-Lizenz) | Gleiche Version, gleiche Spalten, kein WinGet nötig | S bis M |
| Software-Latenz aus PresentMon (`MsAllInputToPhotonLatency`, `MsPCLatency`) | Reflex, FPS-Limit und G-SYNC wirken auf die Latenz, nicht auf die FPS | S bis M |
| Frametime-Verlauf als Grafik, Rohdaten je Lauf speichern | Ruckler sichtbar machen, alte Läufe neu auswerten | M |
| Tweaks mit Neustartbedarf als „ausstehend“ statt „aktiv“ in Messungen | Sonst werden Messungen falschen Tweaks zugeschrieben | S |
| Mausrate messen (Raw Input, Dialed im Vordergrund) | Findet Mäuse, die auf 125 Hz hängen | M |
| Timer-Auflösung messen (`NtQueryTimerResolution`, `Sleep(1)`) | Beweist, dass der Timer-Dienst greift | S |
| Bildwiederholrate als Bruch über `QueryDisplayConfig` | `EnumDisplaySettings` schneidet 59,94 auf 59 ab | S |
| Farbformat und native Auflösung je Monitor | YCbCr oder unter 8 bpc, falsche Auflösung | M |
| NVIDIA-Profil anlegen, wenn der Treiber das Spiel nicht kennt | Nötig für `javaw.exe` (Minecraft), das kein Profil hat | M |
| Minecraft: OpenGL über DXGI-Swapchain (`OGL_CPL_PREFER_DXPRESENT_ID`) | Flip-Modell und VRR im Fenster | nach dem Profil-Anlegen S |
| DPC- und ISR-Latenz je Treiber per ETW | Findet Treiber, die ruckeln lassen | L |
| Bufferbloat-Test | Zeigt, ob der Router SQM braucht. Nur mit Zustimmung, kontaktiert Cloudflare | M |
| Alle Abweichungen per Klick neu anwenden, Hinweis nach Windows- oder Treiber-Update | Bequem, nie still im Hintergrund | S bis M |
| Profil exportieren und importieren | Einstellungen auf einen zweiten PC übertragen | S |
| Wiederherstellungspunkt prüfen | `Checkpoint-Computer` meldet Erfolg, auch wenn wegen der 24-Stunden-Sperre keiner entsteht | S |

## Bewusst nicht aufgenommen

- **Konfigurationsdateien mit Anti-Cheat-Risiko:** Valorant (Einstellungen liegen auf dem Server), Call of Duty (Ricochet), Marvel Rivals (NetEase warnt ausdrücklich), The Finals, PUBG, Apex mit schreibgeschützter `videoconfig.txt`, Tarkov.
- **CS2-Dateien schreiben:** Steam Cloud überschreibt sie. Eine reine Prüfung der Valve-Empfehlung (G-SYNC + V-Sync + Reflex) ist möglich, braucht aber einen Steam-Helfer.
- **Fortnite `LowInputLatencyModeIsEnabled`, `bLatencyTweak1`:** Bedeutung unbekannt, steht zwischen Mobil-Schlüsseln.
- **Mikrofon-Latenz-Dienst:** Hält das Mikrofon dauerhaft offen (Datenschutz-Anzeige), erzwingt bei Bluetooth den Freisprechmodus, Sprachchats puffern ohnehin 20 ms und mehr.
- **Freisprechprofil bei Bluetooth abschalten:** Möglich, aber das Headset-Mikrofon geht dann nicht mehr. Höchstens als ausdrückliche Option.
- **Spieleordner vom Virenscan ausnehmen, Dev Drive für Spiele, automatische Wartung aus, Suchindex aus, Xbox-Dienste aus:** kein gemessener Gewinn oder Sicherheitsverlust.
- **Vollbildoptimierungen pro Spiel aus:** nur als Notlösung bei einzelnen Problemen.
- **Sensoren über WinRing0 oder RWEverything:** Defender stuft WinRing0 als anfälligen Treiber ein.

## Mythen

- Spectre-Schutz aus auf Zen 3 bringt FPS. Die Windows-Standards auf AMD sind bereits schlank.
- CFG oder DEP systemweit aus bringt FPS. Kein Gewinn, Vanguard startet nicht.
- Windows-Suche oder automatische Wartung aus. Beide halten bei Aktivität ohnehin an.
- „HVCI kostet immer 15 %“. Typisch sind etwa 5 % in CPU-limitierten Szenen.
- 96 kHz oder 32 Bit verbessern die Ortung. Spiele mischen mit 48 kHz.
- AAC hat unter Windows weniger Latenz als SBC. Nicht belegt.
- 8000 Hz Mausrate ist immer besser. Unter 1 ms Gewinn, kostet in manchen Spielen CPU.
- Klangverbesserungen aus senkt immer die Latenz. Nur wenn ein verzögerndes APO aktiv ist, bei Sonar schaltet es Sonar selbst ab.
- Pro-Audio-MMCSS oder höhere Priorität für `audiodg` helfen Spielen. Spiele melden sich nicht in dieser Klasse an.
- CS2 `-tickrate 128`, `+cl_forcepreload`, `-threads`. Wirkungslos seit Subtick.
- Threaded Optimization für Minecraft abschalten. Sodium macht das selbst.
- Shader-Cache regelmäßig leeren. Löst Neuübersetzungs-Ruckler aus.
- Ein USB-3-Port senkt die Mauslatenz. Mäuse sind Full-Speed-Geräte am selben Controller.
- `Win32_Processor.CurrentClockSpeed` zeigt den Boost. Er zeigt immer den Basistakt.

## Befunde auf dem Entwicklungs-PC

- Standard-Wiedergabe ist das Arctis Nova 7 über **Bluetooth** (10 ms, keine kleinere Stufe). Der USB-Dongle war nicht eingesteckt, Sonar war aus.
- Das Kommunikationsgerät ist der **Monitor** (HDMI-Audio des VG270U P).
- Windows liegt auf einer Kingston SA400 (SATA), die Spiele auf NVMe.
- CPPC mit unterschiedlichen Kernwerten aktiv, keine Firmware-Drosselung, etwa 4,7 GHz unter Last.
- AMD Chipset Software 7.09.23.2230, AMD bietet inzwischen 8.x an.
- RAM: zwei gleiche Crucial-Riegel, je einer pro Kanal, JEDEC DDR4-3200.
- RTX 3060 Ti mit Treiber 621.14, PCIe 4.0 x16, Resizable BAR an, keine Treiberabstürze.
- BIOS vom 19.09.2025, Secure-Boot-Zertifikate getauscht, UEFI ohne CSM.
- VBS und HVCI laufen, globale Timer-Anfragen wirken auch auf Build 26300 (`Sleep(1)` etwa 1,45 ms).

## Quellen (Auswahl)

- PresentMon Console: https://github.com/GameTechDev/PresentMon/blob/main/README-ConsoleApplication.md
- CapFrameX, Kennzahlen: https://www.capframex.com/blog/post/Explanation%20of%20different%20performance%20metrics
- GamersNexus, Streuung: https://gamersnexus.net/guides/3240-bench-theory-reliability-standard-deviation-game-benchmarks
- Microsoft, Low Latency Audio: https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/low-latency-audio
- RTINGS, Bluetooth-Latenz: https://www.rtings.com/headphones/learn/research/latency
- Ubisoft, FPSLimit: https://www.ubisoft.com/en-us/game/rainbow-six/siege/news-updates/7MPHeuO6hPcy49Mdc3rbDg/100-cpu-usage
- Riot, VAN 9002: https://support.riotgames.com/valorant/support-tools/how-to-enable-exploit-protection-and-prevent-error-code-van9002
- Microsoft, Telemetrie auf Insider-Builds: https://learn.microsoft.com/en-us/windows-insider/data-settings
- Microsoft, Secure Boot 2023: https://support.microsoft.com/kb/5062710
- AMD, fTPM-Ruckler: https://www.amd.com/en/resources/support-articles/faqs/PA-410.html
- Tom's Hardware, NVIDIA 581.94: https://www.tomshardware.com/pc-components/gpu-drivers/nvidia-releases-emergency-driver-update-for-windows-11-25h2-and-24h2-fixes-reduced-gaming-performance-driven-by-botched-windows-updates
- ComputerBase, HVCI unter 24H2: https://www.computerbase.de/2024-09/benchmarks-windows-11-23h2-kb5041587-24h2-hvci/
- Microsoft, .NET-Neuübersetzung: https://learn.microsoft.com/en-us/troubleshoot/developer/dotnet/framework/general/computer-stops-responding-mscorsvw
