//! Lädt die eingebetteten Tweak-Definitionen und prüft sie.
use crate::model::{Action, Kind, Tweak};
use serde::Deserialize;
use std::collections::HashSet;

const FILES: &[(&str, &str)] = &[
    ("windows.toml", include_str!("../tweaks/windows.toml")),
    ("power.toml", include_str!("../tweaks/power.toml")),
    ("gpu.toml", include_str!("../tweaks/gpu.toml")),
    ("storage.toml", include_str!("../tweaks/storage.toml")),
    ("debloat.toml", include_str!("../tweaks/debloat.toml")),
    ("fortnite.toml", include_str!("../tweaks/fortnite.toml")),
    ("network.toml", include_str!("../tweaks/network.toml")),
    ("games.toml", include_str!("../tweaks/games.toml")),
    ("hardware.toml", include_str!("../tweaks/hardware.toml")),
];

pub const CATEGORIES: &[&str] = &[
    "windows", "power", "gpu", "storage", "network", "debloat", "fortnite", "games", "hardware",
];

#[derive(Deserialize)]
struct File {
    tweak: Vec<Tweak>,
}

pub fn load() -> Result<Vec<Tweak>, String> {
    let mut all = Vec::new();
    let mut ids = HashSet::new();
    for (name, text) in FILES {
        let file: File = toml::from_str(text).map_err(|e| format!("{name}: {e}"))?;
        for t in file.tweak {
            let bad = |msg: &str| Err(format!("{name}: {}: {msg}", t.id));
            if !ids.insert(t.id.clone()) {
                return bad("doppelte id");
            }
            if !CATEGORIES.contains(&t.category.as_str()) {
                return bad("unbekannte Kategorie");
            }
            if !["A", "B", "C"].contains(&t.evidence.as_str()) {
                return bad("evidence muss A, B oder C sein");
            }
            if !["low", "medium", "high"].contains(&t.risk.as_str()) {
                return bad("risk muss low, medium oder high sein");
            }
            if t.actions.is_empty() {
                return bad("keine Aktionen");
            }
            for a in &t.actions {
                if let Action::Ps {
                    detect,
                    apply,
                    revert,
                    ..
                } = a
                {
                    let ok = match t.kind {
                        Kind::Toggle => {
                            !detect.is_empty()
                                && apply.is_some()
                                && (revert.is_some() || t.irreversible)
                        }
                        Kind::Check => !detect.is_empty(),
                        Kind::Oneshot => apply.is_some(),
                    };
                    if !ok {
                        return bad("PowerShell-Aktion unvollständig");
                    }
                } else if t.kind == Kind::Oneshot {
                    return bad("oneshot braucht PowerShell-Aktionen");
                }
            }
            all.push(t);
        }
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    #[test]
    fn catalog_is_valid() {
        let tweaks = super::load().expect("Katalog fehlerhaft");
        assert!(tweaks.len() >= 30, "nur {} Tweaks", tweaks.len());
    }
}
