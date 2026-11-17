//! Plattformunabhängige Parser (auf jedem OS testbar).
use std::collections::HashMap;

/// "0x2A" oder "42" → 42
pub fn parse_u64(s: &str) -> Option<u64> {
    let t = s.trim();
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()
    } else {
        t.parse().ok()
    }
}

/// `bcdedit /enum {current}` → element → wert (alles klein geschrieben).
pub fn parse_bcd(out: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in out.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('-') || line.starts_with(' ') {
            continue;
        }
        if let Some((k, v)) = line.split_once(char::is_whitespace) {
            let v = v.trim();
            if !v.is_empty() {
                map.insert(k.to_lowercase(), norm_bcd(v));
            }
        }
    }
    map
}

/// Vereinheitlicht lokalisierte Ja/Nein-Werte.
pub fn norm_bcd(v: &str) -> String {
    match v.trim().to_lowercase().as_str() {
        "ja" | "true" | "on" => "yes".into(),
        "nein" | "false" => "no".into(),
        other => other.to_string(),
    }
}

/// `powercfg /qh SCHEME_CURRENT <sub> <setting>` → AC-Index.
/// Sprachunabhängig: die letzten beiden Hex-Zeilen sind AC und DC.
pub fn parse_powercfg_ac(out: &str) -> Option<u32> {
    let vals: Vec<u32> = out
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let (_, hex) = l.rsplit_once("0x")?;
            if !l.contains(':') || hex.len() != 8 {
                return None;
            }
            u32::from_str_radix(hex, 16).ok()
        })
        .collect();
    if vals.len() >= 2 {
        Some(vals[vals.len() - 2])
    } else {
        None
    }
}

fn is_section(line: &str, section: &str) -> bool {
    let t = line.trim();
    t.len() >= 2
        && t.starts_with('[')
        && t.ends_with(']')
        && t[1..t.len() - 1].eq_ignore_ascii_case(section)
}

fn key_of(line: &str) -> Option<(&str, &str)> {
    let t = line.trim_start();
    if t.starts_with(';') || t.starts_with('[') {
        return None;
    }
    let (k, v) = t.split_once('=')?;
    Some((k.trim(), v.trim()))
}

pub fn ini_get(text: &str, section: &str, key: &str) -> Option<String> {
    let mut inside = false;
    for line in text.lines() {
        if line.trim_start().starts_with('[') {
            inside = is_section(line, section);
            continue;
        }
        if inside {
            if let Some((k, v)) = key_of(line) {
                if k.eq_ignore_ascii_case(key) {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// Setzt (Some) oder entfernt (None) einen Schlüssel; alle anderen Zeilen bleiben unverändert.
pub fn ini_set(text: &str, section: &str, key: &str, value: Option<&str>) -> String {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    let mut inside = false;
    let mut section_end: Option<usize> = None; // Index nach der letzten Zeile mit Inhalt in der Sektion
    let mut found: Option<usize> = None;
    for (i, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with('[') {
            inside = is_section(line, section);
            if inside {
                section_end = Some(i + 1);
            }
            continue;
        }
        if inside {
            if !line.trim().is_empty() {
                section_end = Some(i + 1);
            }
            if let Some((k, _)) = key_of(line) {
                if k.eq_ignore_ascii_case(key) {
                    found = Some(i);
                    break;
                }
            }
        }
    }
    match (found, value) {
        (Some(i), Some(v)) => lines[i] = format!("{key}={v}"),
        (Some(i), None) => {
            lines.remove(i);
        }
        (None, Some(v)) => match section_end {
            Some(end) => lines.insert(end, format!("{key}={v}")),
            None => {
                if lines.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
                    lines.push(String::new());
                }
                lines.push(format!("[{section}]"));
                lines.push(format!("{key}={v}"));
            }
        },
        (None, None) => {}
    }
    let mut out = lines.join(nl);
    if text.ends_with('\n') || text.is_empty() {
        out.push_str(nl);
    }
    out
}

/// Ersetzt %VAR% über die übergebene Lookup-Funktion.
pub fn expand_env(s: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match lookup(name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(parse_u64("0x2A"), Some(42));
        assert_eq!(parse_u64(" 38 "), Some(38));
        assert_eq!(parse_u64("x"), None);
    }

    #[test]
    fn bcd() {
        let out = "\r\nWindows-Startladeprogramm\r\n-------------------\r\nBezeichner              {current}\r\ndevice                  partition=C:\r\nhypervisorlaunchtype    Auto\r\ndisabledynamictick      Yes\r\nuseplatformclock        Ja\r\n";
        let m = parse_bcd(out);
        assert_eq!(m["hypervisorlaunchtype"], "auto");
        assert_eq!(m["disabledynamictick"], "yes");
        assert_eq!(m["useplatformclock"], "yes");
        assert!(!m.contains_key("useplatformtick"));
    }

    #[test]
    fn powercfg() {
        let de = "GUID des Energieschemas: 381b...  (Ausbalanciert)\n  GUID der Untergruppe: 5453...  (Prozessorenergieverwaltung)\n    GUID der Energieeinstellung: 0cc5...  (Parken)\n      Minimal mögliche Einstellung: 0x00000000\n      Maximal mögliche Einstellung: 0x00000064\n      Mögliche Einstellungsinkremente: 0x00000001\n      Mögliche Einstellungseinheiten: %\n    Index der aktuellen Wechselstromeinstellung: 0x00000064\n    Index der aktuellen Gleichstromeinstellung: 0x0000000a\n";
        assert_eq!(parse_powercfg_ac(de), Some(100));
        assert_eq!(parse_powercfg_ac("nix"), None);
    }

    #[test]
    fn ini_roundtrip() {
        let src = "[A]\r\nx=1\r\n\r\n[/Script/FortniteGame.FortGameUserSettings]\r\nbUseVSync=True\r\nbShowGrass=True\r\n\r\n[B]\r\ny=2\r\n";
        let sec = "/Script/FortniteGame.FortGameUserSettings";
        assert_eq!(ini_get(src, sec, "busevsync").as_deref(), Some("True"));
        let a = ini_set(src, sec, "bUseVSync", Some("False"));
        assert_eq!(ini_get(&a, sec, "bUseVSync").as_deref(), Some("False"));
        assert!(a.contains("\r\n[B]\r\ny=2\r\n"));
        let b = ini_set(&a, sec, "bMotionBlur", Some("False"));
        assert!(b.contains("bShowGrass=True\r\nbMotionBlur=False\r\n\r\n[B]"));
        let c = ini_set(&b, sec, "bMotionBlur", None);
        assert_eq!(c, a);
        let d = ini_set("", "S", "k", Some("v"));
        assert_eq!(ini_get(&d, "S", "k").as_deref(), Some("v"));
    }

    #[test]
    fn env() {
        let f = |n: &str| (n == "LOCALAPPDATA").then(|| "C:\\L".to_string());
        assert_eq!(expand_env("%LOCALAPPDATA%\\x", f), "C:\\L\\x");
        assert_eq!(expand_env("100% %NOPE% ok", f), "100% %NOPE% ok");
    }
}
