//! Scan, Anwenden und Zurücksetzen. Vorher-Werte landen im Journal.
use crate::bench::{self, BenchRun};
use crate::hardware::{self, Hardware, HW_SCRIPT};
use crate::model::*;
use crate::parse;
use crate::sys::Sys;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const FORTNITE_INI: &str =
    r"%LOCALAPPDATA%\FortniteGame\Saved\Config\WindowsClient\GameUserSettings.ini";
const PCI_ENUM: &str = r"SYSTEM\CurrentControlSet\Enum\PCI";

#[derive(Serialize, Deserialize, Clone)]
struct Entry {
    at: u64,
    /// Ist-Wert jeder Aktion vor dem Anwenden (None = war nicht gesetzt).
    previous: Vec<Option<String>>,
}

#[derive(Serialize, Deserialize, Default)]
struct Journal {
    entries: BTreeMap<String, Entry>,
}

#[derive(Serialize)]
pub struct ScanResult {
    pub hardware: Hardware,
    pub tweaks: Vec<TweakView>,
    pub ms: u128,
}

/// Einmal pro Scan gesammelte Ausgaben teurer Abfragen.
struct Ctx {
    bcd: Result<HashMap<String, String>, String>,
    ps: Result<HashMap<String, String>, String>,
    nv: Result<HashMap<u32, Option<u32>>, String>,
}

enum Obs {
    Value {
        raw: Option<String>,
        applied: bool,
        shown: String,
    },
    Unsupported(String),
    Error(String),
}

pub struct Engine<S: Sys> {
    sys: S,
    tweaks: Vec<Tweak>,
    journal: Mutex<Journal>,
    hw: Mutex<Hardware>,
}

fn shown(v: &Option<String>) -> String {
    v.clone().unwrap_or_else(|| "nicht gesetzt".into())
}

fn same(a: &Option<String>, b: &Option<String>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => x.eq_ignore_ascii_case(y),
        _ => false,
    }
}

fn norm_reg(kind: RegKind, v: &Val) -> String {
    match kind {
        RegKind::Sz => v.text(),
        _ => parse::parse_u64(&v.text())
            .map(|n| n.to_string())
            .unwrap_or_else(|| v.text()),
    }
}

/// Zielwert einer Aktion in normalisierter Form (None = soll fehlen).
fn target(a: &Action) -> Option<String> {
    match a {
        Action::Registry { kind, apply, .. } => apply.as_ref().map(|v| norm_reg(*kind, v)),
        Action::Bcd { apply, .. } => apply.as_ref().map(|v| parse::norm_bcd(&v.text())),
        Action::Powercfg { apply, .. } => Some(apply.to_string()),
        Action::Ps { expect, .. } => Some(expect.clone()),
        Action::Ini { apply, .. } => Some(apply.clone()),
        Action::DeviceRegistry { apply, .. } => Some(apply.to_string()),
        Action::NvidiaDrs { apply, .. } => Some(num(apply).to_string()),
    }
}

/// Windows-Standard laut Definition (None = fehlt bzw. unbekannt).
fn default_of(a: &Action) -> Option<String> {
    match a {
        Action::Registry { kind, default, .. } => default.as_ref().map(|v| norm_reg(*kind, v)),
        Action::Bcd { default, .. } => default.as_ref().map(|v| parse::norm_bcd(&v.text())),
        Action::Powercfg { default, .. } => default.map(|v| v.to_string()),
        Action::Ini { default, .. } => default.clone(),
        Action::Ps { .. } | Action::DeviceRegistry { .. } | Action::NvidiaDrs { .. } => None,
    }
}

fn num(v: &Val) -> u32 {
    parse::parse_u64(&v.text()).unwrap_or(0) as u32
}

fn label(a: &Action) -> String {
    match a {
        Action::Registry {
            hive, path, name, ..
        } => format!("{hive}\\{path} → {name}"),
        Action::Bcd { element, .. } => format!("bcdedit {element}"),
        Action::Powercfg { sub, setting, .. } => format!("powercfg {sub} {setting}"),
        Action::Ps { .. } => "PowerShell-Abfrage".into(),
        Action::Ini { key, .. } => format!("GameUserSettings.ini → {key}"),
        Action::DeviceRegistry { name, .. } => format!("Geräte-Registry → {name}"),
        Action::NvidiaDrs { setting, .. } => format!("NVIDIA-Treiberprofil {:#010x}", num(setting)),
    }
}

impl<S: Sys> Engine<S> {
    pub fn new(sys: S) -> Self {
        let tweaks = crate::catalog::load().expect("eingebetteter Tweak-Katalog ist fehlerhaft");
        let journal = std::fs::read_to_string(sys.data_dir().join("journal.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Engine {
            sys,
            tweaks,
            journal: Mutex::new(journal),
            hw: Mutex::new(Hardware::default()),
        }
    }

    fn save_journal(&self, j: &Journal) -> Result<(), String> {
        let text = serde_json::to_string_pretty(j).map_err(|e| e.to_string())?;
        std::fs::write(self.sys.data_dir().join("journal.json"), text)
            .map_err(|e| format!("Journal: {e}"))
    }

    fn expand(&self, path: &str) -> String {
        parse::expand_env(path, |n| self.sys.env(n))
    }

    fn find(&self, id: &str) -> Result<&Tweak, String> {
        self.tweaks
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("unbekannter Tweak {id}"))
    }

    // ---------- Erkennung ----------

    fn ps_detect_lines(t: &Tweak, out: &mut String) {
        for (i, a) in t.actions.iter().enumerate() {
            if let Action::Ps { detect, .. } = a {
                if !detect.is_empty() {
                    out.push_str(&format!(
                        "$r['{}#{i}'] = try {{ [string]((& {{\n{detect}\n}} | Out-String).Trim()) }} catch {{ 'ERR: ' + $_.Exception.Message }}\n",
                        t.id
                    ));
                }
            }
        }
    }

    fn parse_ps(out: &str) -> Result<serde_json::Value, String> {
        let start = out.find('{').ok_or("PowerShell lieferte keine Daten")?;
        serde_json::from_str(out[start..].trim()).map_err(|e| format!("PowerShell-Ausgabe: {e}"))
    }

    fn ps_map(v: &serde_json::Value) -> HashMap<String, String> {
        v.get("detect")
            .and_then(|d| d.as_object())
            .map(|o| {
                o.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn read_bcd(&self) -> Result<HashMap<String, String>, String> {
        self.sys
            .run("bcdedit", &["/enum", "{current}"])
            .map(|o| parse::parse_bcd(&o))
            .map_err(|e| format!("bcdedit: {e}"))
    }

    fn read_nv<'a>(
        &self,
        tweaks: impl Iterator<Item = &'a Tweak>,
    ) -> Result<HashMap<u32, Option<u32>>, String> {
        let mut ids: Vec<u32> = tweaks
            .flat_map(|t| t.actions.iter())
            .filter_map(|a| match a {
                Action::NvidiaDrs { setting, .. } => Some(num(setting)),
                _ => None,
            })
            .collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let values = self.sys.nv_read(&ids)?;
        Ok(ids.into_iter().zip(values).collect())
    }

    /// Kontext für einen einzelnen Tweak (nur die Abfragen, die er braucht).
    fn ctx_for(&self, t: &Tweak) -> Ctx {
        let bcd = if t.actions.iter().any(|a| matches!(a, Action::Bcd { .. })) {
            self.read_bcd()
        } else {
            Ok(HashMap::new())
        };
        let mut script = String::from("$ErrorActionPreference='SilentlyContinue'\n$r=@{}\n");
        let before = script.len();
        Self::ps_detect_lines(t, &mut script);
        let ps = if script.len() == before {
            Ok(HashMap::new())
        } else {
            script.push_str("@{detect=$r} | ConvertTo-Json -Depth 4 -Compress\n");
            self.sys
                .ps(&script)
                .and_then(|o| Self::parse_ps(&o))
                .map(|v| Self::ps_map(&v))
        };
        Ctx {
            bcd,
            ps,
            nv: self.read_nv(std::iter::once(t)),
        }
    }

    fn devices(
        &self,
        class_guid: &str,
        subpath: &str,
        name: &str,
    ) -> Result<BTreeMap<String, Option<String>>, String> {
        let mut map = BTreeMap::new();
        for hwid in self.sys.reg_subkeys("HKLM", PCI_ENUM)? {
            let base = format!("{PCI_ENUM}\\{hwid}");
            for inst in self.sys.reg_subkeys("HKLM", &base)? {
                let dev = format!("{base}\\{inst}");
                let class = self
                    .sys
                    .reg_get("HKLM", &dev, "ClassGUID", RegKind::Sz)
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                if class.eq_ignore_ascii_case(class_guid) {
                    let val = self.sys.reg_get(
                        "HKLM",
                        &format!("{dev}\\{subpath}"),
                        name,
                        RegKind::Dword,
                    )?;
                    map.insert(dev, val);
                }
            }
        }
        Ok(map)
    }

    fn observe(&self, t: &Tweak, idx: usize, a: &Action, ctx: &Ctx) -> Obs {
        let want = target(a);
        let value = |raw: Option<String>| {
            let applied = same(&raw, &want);
            let s = shown(&raw);
            Obs::Value {
                raw,
                applied,
                shown: s,
            }
        };
        match a {
            Action::Registry {
                hive,
                path,
                name,
                kind,
                ..
            } => match self.sys.reg_get(hive, path, name, *kind) {
                Ok(v) => value(v),
                Err(e) => Obs::Error(e),
            },
            Action::Bcd { element, .. } => match &ctx.bcd {
                Ok(map) => value(map.get(&element.to_lowercase()).cloned()),
                Err(e) => Obs::Error(e.clone()),
            },
            Action::Powercfg { sub, setting, .. } => {
                match self
                    .sys
                    .run("powercfg", &["/qh", "SCHEME_CURRENT", sub, setting])
                {
                    Ok(out) => match parse::parse_powercfg_ac(&out) {
                        Some(v) => value(Some(v.to_string())),
                        None => {
                            Obs::Unsupported("Einstellung gibt es auf diesem System nicht".into())
                        }
                    },
                    Err(_) => {
                        Obs::Unsupported("Einstellung gibt es auf diesem System nicht".into())
                    }
                }
            }
            Action::Ps { .. } => match &ctx.ps {
                Ok(map) => {
                    let out = map
                        .get(&format!("{}#{idx}", t.id))
                        .cloned()
                        .unwrap_or_default();
                    if let Some(msg) = out.strip_prefix("ERR: ") {
                        Obs::Error(msg.to_string())
                    } else {
                        value(Some(out))
                    }
                }
                Err(e) => Obs::Error(e.clone()),
            },
            Action::Ini {
                file, section, key, ..
            } => match std::fs::read(self.expand(file)) {
                Ok(bytes) => {
                    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
                        return Obs::Unsupported("Datei ist UTF-16 kodiert".into());
                    }
                    value(parse::ini_get(
                        &String::from_utf8_lossy(&bytes),
                        section,
                        key,
                    ))
                }
                Err(_) => Obs::Unsupported("Konfigurationsdatei nicht gefunden".into()),
            },
            Action::NvidiaDrs { setting, .. } => match &ctx.nv {
                Ok(map) => value(
                    map.get(&num(setting))
                        .copied()
                        .flatten()
                        .map(|v| v.to_string()),
                ),
                Err(e) => Obs::Error(e.clone()),
            },
            Action::DeviceRegistry {
                class_guid,
                subpath,
                name,
                apply,
            } => match self.devices(class_guid, subpath, name) {
                Ok(map) if map.is_empty() => {
                    Obs::Unsupported("kein passendes Gerät gefunden".into())
                }
                Ok(map) => {
                    let want = Some(apply.to_string());
                    let done = map.values().filter(|v| **v == want).count();
                    Obs::Value {
                        raw: serde_json::to_string(&map).ok(),
                        applied: done == map.len(),
                        shown: format!("{done} von {} Geräten", map.len()),
                    }
                }
                Err(e) => Obs::Error(e),
            },
        }
    }

    fn view(&self, t: &Tweak, ctx: &Ctx, hw: &Hardware, journal: &Journal) -> TweakView {
        let managed = journal.entries.contains_key(&t.id);
        let mut v = TweakView {
            id: t.id.clone(),
            name: t.name.clone(),
            category: t.category.clone(),
            description: t.description.clone(),
            note: t.note.clone(),
            evidence: t.evidence.clone(),
            risk: t.risk.clone(),
            reboot: t.reboot,
            kind: t.kind,
            irreversible: t.irreversible,
            status: "inactive",
            detail: String::new(),
            drift: false,
            managed,
            actions: Vec::new(),
        };
        if let Some(flag) = hardware::unmet(&t.requires, &hw.flags) {
            v.status = "unsupported";
            v.detail = format!("Passt nicht zu diesem System ({flag})");
            return v;
        }
        if t.kind == Kind::Oneshot {
            v.status = "action";
            return v;
        }
        let (mut applied, mut unsupported, mut error) = (0, None, None);
        for (i, a) in t.actions.iter().enumerate() {
            let (state, current) = match self.observe(t, i, a, ctx) {
                Obs::Value {
                    applied: true,
                    shown,
                    ..
                } => {
                    applied += 1;
                    ("applied", shown)
                }
                Obs::Value { shown, .. } => ("open", shown),
                Obs::Unsupported(m) => {
                    unsupported.get_or_insert(m.clone());
                    ("unsupported", m)
                }
                Obs::Error(m) => {
                    error.get_or_insert(m.clone());
                    ("error", m)
                }
            };
            v.actions.push(ActionView {
                label: label(a),
                target: shown(&target(a)),
                current,
                state,
            });
        }
        let total = t.actions.len();
        if let Some(m) = unsupported {
            v.status = "unsupported";
            v.detail = m;
        } else if let Some(m) = error {
            v.status = "error";
            v.detail = m;
        } else if applied == total {
            v.status = "active";
        } else if applied > 0 {
            v.status = "partial";
            v.detail = format!("{applied} von {total} Einstellungen aktiv");
        }
        v.drift = managed && t.kind == Kind::Toggle && v.status != "active";
        v
    }

    /// Kompletter Scan: Hardware plus Zustand aller Tweaks.
    pub fn scan(&self) -> Result<ScanResult, String> {
        let started = Instant::now();
        let mut script = String::from("$ErrorActionPreference='SilentlyContinue'\n");
        script.push_str(HW_SCRIPT);
        script.push_str("$r=@{}\n");
        for t in &self.tweaks {
            Self::ps_detect_lines(t, &mut script);
        }
        script.push_str("@{hw=$hw; detect=$r} | ConvertTo-Json -Depth 6 -Compress\n");

        // PowerShell ist der langsamste Teil und läuft parallel zu bcdedit.
        let (ps_out, bcd) = std::thread::scope(|s| {
            let h = s.spawn(|| self.sys.ps(&script));
            let bcd = self.read_bcd();
            (
                h.join()
                    .unwrap_or_else(|_| Err("PowerShell-Thread abgestürzt".into())),
                bcd,
            )
        });
        let parsed = ps_out.and_then(|o| Self::parse_ps(&o));
        let mut hw = match &parsed {
            Ok(v) => hardware::from_json(v.get("hw").unwrap_or(&serde_json::Value::Null)),
            Err(_) => Hardware::default(),
        };
        if std::path::Path::new(&self.expand(FORTNITE_INI)).exists() {
            hw.flags.insert("fortnite".into());
        }
        let ctx = Ctx {
            bcd,
            ps: parsed.map(|v| Self::ps_map(&v)),
            nv: self.read_nv(self.tweaks.iter()),
        };
        let journal = self.journal.lock().unwrap();
        let tweaks = self
            .tweaks
            .iter()
            .map(|t| self.view(t, &ctx, &hw, &journal))
            .collect();
        *self.hw.lock().unwrap() = hw.clone();
        Ok(ScanResult {
            hardware: hw,
            tweaks,
            ms: started.elapsed().as_millis(),
        })
    }

    fn view_one(&self, t: &Tweak) -> TweakView {
        let ctx = self.ctx_for(t);
        let hw = self.hw.lock().unwrap().clone();
        let journal = self.journal.lock().unwrap();
        self.view(t, &ctx, &hw, &journal)
    }

    // ---------- Schreiben ----------

    /// Setzt eine Aktion auf einen konkreten Wert (None = Wert entfernen).
    fn set(&self, a: &Action, val: Option<&str>) -> Result<(), String> {
        match a {
            Action::Registry {
                hive,
                path,
                name,
                kind,
                ..
            } => match val {
                Some(v) => self.sys.reg_set(hive, path, name, *kind, v),
                None => self.sys.reg_delete(hive, path, name),
            },
            Action::Bcd { element, .. } => match val {
                Some(v) => self
                    .sys
                    .run("bcdedit", &["/set", "{current}", element, v])
                    .map(|_| ()),
                None => match self
                    .sys
                    .run("bcdedit", &["/deletevalue", "{current}", element])
                {
                    Ok(_) => Ok(()),
                    // Löschen eines nicht vorhandenen Elements meldet einen Fehler, ist aber das Ziel.
                    Err(e) => match self.read_bcd() {
                        Ok(map) if !map.contains_key(&element.to_lowercase()) => Ok(()),
                        _ => Err(e),
                    },
                },
            },
            Action::Powercfg { sub, setting, .. } => {
                let v = val.ok_or("kein Vorher-Wert bekannt")?;
                self.sys.run(
                    "powercfg",
                    &["/setacvalueindex", "SCHEME_CURRENT", sub, setting, v],
                )?;
                self.sys
                    .run("powercfg", &["/setactive", "SCHEME_CURRENT"])
                    .map(|_| ())
            }
            Action::Ini {
                file, section, key, ..
            } => {
                let path = self.expand(file);
                let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
                let text = String::from_utf8(bytes)
                    .map_err(|_| "Datei ist nicht UTF-8 kodiert".to_string())?;
                std::fs::write(&path, parse::ini_set(&text, section, key, val))
                    .map_err(|e| format!("{path}: {e}"))
            }
            Action::DeviceRegistry {
                class_guid,
                subpath,
                name,
                ..
            } => {
                let v = val.ok_or("kein Vorher-Wert bekannt")?;
                // Entweder ein Wert für alle Geräte oder die gespeicherte Karte Gerät → Vorher-Wert.
                let plan: BTreeMap<String, Option<String>> = if v.starts_with('{') {
                    serde_json::from_str(v).map_err(|e| e.to_string())?
                } else {
                    self.devices(class_guid, subpath, name)?
                        .into_keys()
                        .map(|k| (k, Some(v.to_string())))
                        .collect()
                };
                for (dev, value) in plan {
                    let path = format!("{dev}\\{subpath}");
                    match value {
                        Some(x) => self.sys.reg_set("HKLM", &path, name, RegKind::Dword, &x)?,
                        None => self.sys.reg_delete("HKLM", &path, name)?,
                    }
                }
                Ok(())
            }
            Action::NvidiaDrs { setting, .. } => {
                let value = match val {
                    Some(v) => Some(parse::parse_u64(v).ok_or("ungültiger Wert")? as u32),
                    None => None,
                };
                self.sys.nv_write(num(setting), value)
            }
            Action::Ps { .. } => Err("interner Fehler: PowerShell-Aktion".into()),
        }
    }

    fn run_ps(&self, script: &str) -> Result<String, String> {
        self.sys
            .ps(&format!("$ErrorActionPreference='Stop'\n{script}"))
    }

    fn do_apply(&self, a: &Action) -> Result<(), String> {
        match a {
            Action::Ps { apply, .. } => self
                .run_ps(apply.as_deref().ok_or("kein Apply-Skript")?)
                .map(|_| ()),
            _ => self.set(a, target(a).as_deref()),
        }
    }

    fn do_restore(&self, a: &Action, previous: Option<&Option<String>>) -> Result<(), String> {
        match a {
            Action::Ps { revert, .. } => self
                .run_ps(
                    revert
                        .as_deref()
                        .ok_or("lässt sich nicht automatisch zurücksetzen")?,
                )
                .map(|_| ()),
            _ => {
                // Vorher-Wert aus dem Journal; war der Tweak schon vor Dialed aktiv, gilt der Windows-Standard.
                let want = target(a);
                let value = match previous {
                    Some(p) if !same(p, &want) && !matches!(a, Action::DeviceRegistry { .. }) => {
                        p.clone()
                    }
                    Some(p) if matches!(a, Action::DeviceRegistry { .. }) => p.clone(),
                    _ => default_of(a),
                };
                self.set(a, value.as_deref())
            }
        }
    }

    pub fn apply(&self, id: &str) -> Result<TweakView, String> {
        let t = self.find(id)?;
        if t.kind != Kind::Toggle {
            return Err("dieser Eintrag ist kein Schalter".into());
        }
        if let Some(flag) = hardware::unmet(&t.requires, &self.hw.lock().unwrap().flags) {
            return Err(format!("passt nicht zu diesem System ({flag})"));
        }
        let ctx = self.ctx_for(t);
        let mut previous = Vec::new();
        for (i, a) in t.actions.iter().enumerate() {
            match self.observe(t, i, a, &ctx) {
                Obs::Value { raw, .. } => previous.push(raw),
                Obs::Unsupported(m) | Obs::Error(m) => return Err(m),
            }
        }
        for (i, a) in t.actions.iter().enumerate() {
            if let Err(e) = self.do_apply(a) {
                // Bereits gesetzte Aktionen dieses Tweaks zurückrollen.
                for j in (0..i).rev() {
                    let _ = self.do_restore(&t.actions[j], Some(&previous[j]));
                }
                return Err(e);
            }
        }
        {
            let mut journal = self.journal.lock().unwrap();
            let at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            // Bei erneutem Anwenden (Drift) bleibt der ursprüngliche Vorher-Wert erhalten.
            journal
                .entries
                .entry(t.id.clone())
                .or_insert(Entry { at, previous });
            self.save_journal(&journal)?;
        }
        Ok(self.view_one(t))
    }

    pub fn revert(&self, id: &str) -> Result<TweakView, String> {
        let t = self.find(id)?;
        if t.kind != Kind::Toggle || t.irreversible {
            return Err("lässt sich nicht automatisch zurücksetzen".into());
        }
        let entry = self.journal.lock().unwrap().entries.get(&t.id).cloned();
        for (i, a) in t.actions.iter().enumerate() {
            let prev = entry.as_ref().and_then(|e| e.previous.get(i));
            self.do_restore(a, prev)?;
        }
        {
            let mut journal = self.journal.lock().unwrap();
            journal.entries.remove(&t.id);
            self.save_journal(&journal)?;
        }
        Ok(self.view_one(t))
    }

    pub fn run_oneshot(&self, id: &str) -> Result<String, String> {
        let t = self.find(id)?;
        if t.kind != Kind::Oneshot {
            return Err("dieser Eintrag ist keine Aktion".into());
        }
        let mut out = String::new();
        for a in &t.actions {
            if let Action::Ps {
                apply: Some(script),
                ..
            } = a
            {
                out.push_str(self.run_ps(script)?.trim());
            }
        }
        Ok(out)
    }

    // ---------- Benchmark ----------

    pub fn bench_history(&self) -> Vec<BenchRun> {
        std::fs::read_to_string(self.sys.data_dir().join("bench.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Misst ein laufendes Spiel mit PresentMon und speichert das Ergebnis samt aktiver Tweaks.
    pub fn benchmark(&self, process: &str, seconds: u32) -> Result<BenchRun, String> {
        let name = process
            .trim()
            .trim_end_matches(".exe")
            .trim_end_matches(".EXE");
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_. ".contains(c))
        {
            return Err("ungültiger Prozessname".into());
        }
        if !(10..=600).contains(&seconds) {
            return Err("Dauer muss zwischen 10 und 600 Sekunden liegen".into());
        }
        let active: Vec<String> = self
            .scan()?
            .tweaks
            .into_iter()
            .filter(|t| t.kind == Kind::Toggle && t.status == "active")
            .map(|t| t.id)
            .collect();
        let script = format!(
            r#"$exe = @(Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\Intel.PresentMon.Console*" -Recurse -Filter 'PresentMon*.exe' -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName) + @("$env:APPDATA\Dialed\PresentMon.exe") | Where-Object {{ $_ -and (Test-Path $_) }} | Select-Object -First 1
if (-not $exe) {{ throw 'PresentMon fehlt. Installieren mit: winget install --id Intel.PresentMon.Console -e' }}
if (-not (Get-Process -Name '{name}' -ErrorAction SilentlyContinue)) {{ throw '{name}.exe läuft nicht. Spiel starten und erneut messen.' }}
$out = "$env:APPDATA\Dialed\bench-last.csv"
if (Test-Path $out) {{ Remove-Item $out -Force }}
& $exe --process_name '{name}.exe' --output_file $out --timed {seconds} --terminate_after_timed --no_console_stats --stop_existing_session | Out-Null
if (-not (Test-Path $out)) {{ throw 'PresentMon hat keine Daten geschrieben' }}"#
        );
        self.run_ps(&script)?;
        let csv = std::fs::read(self.sys.data_dir().join("bench-last.csv"))
            .map_err(|e| format!("Messdatei: {e}"))?;
        let frames = bench::frametimes(&String::from_utf8_lossy(&csv))?;
        let (avg_fps, low1_fps, low01_fps) = bench::stats(&frames)
            .ok_or("zu wenige Frames gemessen, läuft das Spiel im Vordergrund?")?;
        let run = BenchRun {
            at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            process: format!("{name}.exe"),
            seconds,
            frames: frames.len(),
            avg_fps,
            low1_fps,
            low01_fps,
            active,
        };
        let mut history = self.bench_history();
        history.push(run.clone());
        let start = history.len().saturating_sub(50);
        let text = serde_json::to_string_pretty(&history[start..]).map_err(|e| e.to_string())?;
        std::fs::write(self.sys.data_dir().join("bench.json"), text)
            .map_err(|e| format!("Verlauf: {e}"))?;
        Ok(run)
    }

    pub fn restore_point(&self) -> Result<(), String> {
        self.run_ps("Checkpoint-Computer -Description 'Dialed' -RestorePointType 'MODIFY_SETTINGS'")
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Simuliertes System: Registry im Speicher, bcdedit als Map.
    #[derive(Default)]
    struct Mock {
        reg: Mutex<HashMap<String, String>>,
        bcd: Mutex<BTreeMap<String, String>>,
        nv: Mutex<HashMap<u32, u32>>,
        dir: PathBuf,
    }

    fn k(h: &str, p: &str, n: &str) -> String {
        format!("{h}|{p}|{n}").to_lowercase()
    }

    impl Sys for Mock {
        fn reg_get(&self, h: &str, p: &str, n: &str, _: RegKind) -> Result<Option<String>, String> {
            Ok(self.reg.lock().unwrap().get(&k(h, p, n)).cloned())
        }
        fn reg_set(&self, h: &str, p: &str, n: &str, _: RegKind, v: &str) -> Result<(), String> {
            self.reg.lock().unwrap().insert(k(h, p, n), v.to_string());
            Ok(())
        }
        fn reg_delete(&self, h: &str, p: &str, n: &str) -> Result<(), String> {
            self.reg.lock().unwrap().remove(&k(h, p, n));
            Ok(())
        }
        fn reg_subkeys(&self, _: &str, _: &str) -> Result<Vec<String>, String> {
            Ok(Vec::new())
        }
        fn run(&self, exe: &str, args: &[&str]) -> Result<String, String> {
            let mut bcd = self.bcd.lock().unwrap();
            match (exe, args) {
                ("bcdedit", ["/enum", _]) => {
                    Ok(bcd.iter().map(|(k, v)| format!("{k}    {v}\r\n")).collect())
                }
                ("bcdedit", ["/set", _, el, v]) => {
                    bcd.insert(el.to_string(), v.to_string());
                    Ok(String::new())
                }
                ("bcdedit", ["/deletevalue", _, el]) => bcd
                    .remove(*el)
                    .map(|_| String::new())
                    .ok_or("nicht gefunden".into()),
                _ => Err("nicht simuliert".into()),
            }
        }
        fn ps(&self, _: &str) -> Result<String, String> {
            Ok(r#"{"hw":{"cpu":"Intel(R) Core(TM) i5","cpu_vendor":"GenuineIntel","build":26200,"gpus":["NVIDIA GeForce RTX 3060"],"chassis":[3],"battery":0},"detect":{}}"#.into())
        }
        fn env(&self, _: &str) -> Option<String> {
            None
        }
        fn data_dir(&self) -> PathBuf {
            std::fs::create_dir_all(&self.dir).unwrap();
            self.dir.clone()
        }
        fn nv_read(&self, ids: &[u32]) -> Result<Vec<Option<u32>>, String> {
            let nv = self.nv.lock().unwrap();
            Ok(ids.iter().map(|id| nv.get(id).copied()).collect())
        }
        fn nv_write(&self, id: u32, value: Option<u32>) -> Result<(), String> {
            let mut nv = self.nv.lock().unwrap();
            match value {
                Some(v) => nv.insert(id, v),
                None => nv.remove(&id),
            };
            Ok(())
        }
    }

    fn engine(name: &str) -> Engine<Mock> {
        let dir = std::env::temp_dir().join(format!("dialed-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Engine::new(Mock {
            dir,
            ..Default::default()
        })
    }

    fn status<'a>(r: &'a ScanResult, id: &str) -> &'a TweakView {
        r.tweaks.iter().find(|t| t.id == id).unwrap()
    }

    #[test]
    fn registry_roundtrip_restores_previous_value() {
        let e = engine("reg");
        let path = r"SYSTEM\CurrentControlSet\Control\PriorityControl";
        e.sys
            .reg_set(
                "HKLM",
                path,
                "Win32PrioritySeparation",
                RegKind::Dword,
                "24",
            )
            .unwrap();
        let scan = e.scan().unwrap();
        assert_eq!(status(&scan, "prio.win32_2a").status, "inactive");
        assert!(scan.hardware.flags.contains("gpu.nvidia"));

        let v = e.apply("prio.win32_2a").unwrap();
        assert_eq!(v.status, "active");
        assert!(v.managed);
        assert_eq!(
            e.sys
                .reg_get("HKLM", path, "Win32PrioritySeparation", RegKind::Dword)
                .unwrap()
                .as_deref(),
            Some("42")
        );

        // Fremde Änderung → Drift
        e.sys
            .reg_set("HKLM", path, "Win32PrioritySeparation", RegKind::Dword, "2")
            .unwrap();
        assert!(status(&e.scan().unwrap(), "prio.win32_2a").drift);

        // Zurücksetzen stellt den echten Vorher-Wert (24) wieder her, nicht den Standard (2)
        let v = e.revert("prio.win32_2a").unwrap();
        assert_eq!(v.status, "inactive");
        assert_eq!(
            e.sys
                .reg_get("HKLM", path, "Win32PrioritySeparation", RegKind::Dword)
                .unwrap()
                .as_deref(),
            Some("24")
        );
    }

    #[test]
    fn already_active_tweak_reverts_to_default() {
        let e = engine("pre");
        let path = r"SYSTEM\CurrentControlSet\Control\Session Manager\kernel";
        e.sys
            .reg_set(
                "HKLM",
                path,
                "GlobalTimerResolutionRequests",
                RegKind::Dword,
                "1",
            )
            .unwrap();
        let scan = e.scan().unwrap();
        assert_eq!(
            status(&scan, "kernel.global_timer_resolution").status,
            "active"
        );
        assert!(!status(&scan, "kernel.global_timer_resolution").managed);
        e.revert("kernel.global_timer_resolution").unwrap();
        assert_eq!(
            e.sys
                .reg_get(
                    "HKLM",
                    path,
                    "GlobalTimerResolutionRequests",
                    RegKind::Dword
                )
                .unwrap(),
            None
        );
    }

    #[test]
    fn bcd_roundtrip() {
        let e = engine("bcd");
        e.scan().unwrap();
        assert_eq!(
            e.apply("bcd.disable_dynamic_tick").unwrap().status,
            "active"
        );
        assert_eq!(
            e.sys
                .bcd
                .lock()
                .unwrap()
                .get("disabledynamictick")
                .map(String::as_str),
            Some("yes")
        );
        assert_eq!(
            e.revert("bcd.disable_dynamic_tick").unwrap().status,
            "inactive"
        );
        assert!(e.sys.bcd.lock().unwrap().is_empty());

        // "Erzwungenes HPET entfernen": aktiv, solange das Element fehlt
        e.sys
            .bcd
            .lock()
            .unwrap()
            .insert("useplatformclock".into(), "Yes".into());
        assert_eq!(
            status(&e.scan().unwrap(), "bcd.remove_useplatformclock").status,
            "inactive"
        );
        assert_eq!(
            e.apply("bcd.remove_useplatformclock").unwrap().status,
            "active"
        );
        e.revert("bcd.remove_useplatformclock").unwrap();
        assert_eq!(
            e.sys
                .bcd
                .lock()
                .unwrap()
                .get("useplatformclock")
                .map(String::as_str),
            Some("yes")
        );
    }

    #[test]
    fn requirements_and_journal_persist() {
        let e = engine("req");
        let scan = e.scan().unwrap();
        // Mock ist ein Desktop mit Intel-CPU → X3D-Check passt nicht
        assert_eq!(status(&scan, "hw.x3d_service").status, "unsupported");
        e.apply("prio.win32_2a").unwrap();
        let dir = e.sys.dir.clone();
        let again = Engine::new(Mock {
            dir,
            ..Default::default()
        });
        assert!(again
            .journal
            .lock()
            .unwrap()
            .entries
            .contains_key("prio.win32_2a"));
    }

    #[test]
    fn nvidia_profile_roundtrip() {
        let e = engine("nv");
        e.scan().unwrap();
        // Vorher im Treiber: adaptiv (0)
        e.sys.nv.lock().unwrap().insert(0x1057_EB71, 0);
        assert_eq!(e.apply("nv.power_max").unwrap().status, "active");
        assert_eq!(e.sys.nv.lock().unwrap().get(&0x1057_EB71), Some(&1));
        assert_eq!(e.revert("nv.power_max").unwrap().status, "inactive");
        assert_eq!(e.sys.nv.lock().unwrap().get(&0x1057_EB71), Some(&0));
        // Ohne Vorher-Wert geht es zurück auf den Treiberstandard
        e.apply("nv.prerender_1").unwrap();
        e.revert("nv.prerender_1").unwrap();
        assert!(!e.sys.nv.lock().unwrap().contains_key(&0x007B_A09E));
    }

    /// Schreibt einen Beispiel-Scan für die Browser-Vorschau der UI (ui/mock.json).
    #[test]
    fn dump_mock_scan() {
        let Ok(path) = std::env::var("DIALED_DUMP") else {
            return;
        };
        let e = engine("dump");
        e.scan().unwrap();
        for id in [
            "kernel.global_timer_resolution",
            "prio.win32_2a",
            "game.dvr_off",
            "debloat.telemetry",
        ] {
            e.apply(id).unwrap();
        }
        std::fs::write(path, serde_json::to_string(&e.scan().unwrap()).unwrap()).unwrap();
    }
}
