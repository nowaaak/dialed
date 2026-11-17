use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Lässt sich an- und ausschalten.
    #[default]
    Toggle,
    /// Nur Erkennung (z. B. BIOS-Einstellungen), kein Schalter.
    Check,
    /// Einmalige Aktion ohne Zustand.
    Oneshot,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RegKind {
    Dword,
    Qword,
    Sz,
}

/// Wert aus der TOML-Datei: Zahl oder Text (auch "0x2A").
#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Val {
    Int(i64),
    Str(String),
}

impl Val {
    pub fn text(&self) -> String {
        match self {
            Val::Int(i) => i.to_string(),
            Val::Str(s) => s.clone(),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    /// Registry-Wert. `apply`/`default` weggelassen = Wert soll fehlen.
    Registry {
        hive: String,
        path: String,
        name: String,
        kind: RegKind,
        #[serde(default)]
        apply: Option<Val>,
        #[serde(default)]
        default: Option<Val>,
    },
    /// Element im BCD-Store des aktuellen Boot-Eintrags.
    Bcd {
        element: String,
        #[serde(default)]
        apply: Option<Val>,
        #[serde(default)]
        default: Option<Val>,
    },
    /// Einstellung im aktiven Energieplan (Netzbetrieb).
    Powercfg {
        sub: String,
        setting: String,
        apply: u32,
        #[serde(default)]
        default: Option<u32>,
    },
    /// PowerShell: `detect` liefert Text, der mit `expect` verglichen wird.
    Ps {
        #[serde(default)]
        detect: String,
        #[serde(default)]
        expect: String,
        #[serde(default)]
        apply: Option<String>,
        #[serde(default)]
        revert: Option<String>,
    },
    /// Schlüssel in einer INI-Datei (z. B. GameUserSettings.ini).
    Ini {
        file: String,
        section: String,
        key: String,
        apply: String,
        #[serde(default)]
        default: Option<String>,
    },
    /// NVIDIA-Treibereinstellung (DWORD) im Basisprofil. `setting` ist die Kennung aus NvApiDriverSettings.h.
    NvidiaDrs { setting: Val, apply: Val },
    /// DWORD unter jedem PCI-Gerät einer Geräteklasse (z. B. MSI-Mode der GPU).
    DeviceRegistry {
        class_guid: String,
        subpath: String,
        name: String,
        apply: u32,
    },
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Tweak {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    #[serde(default)]
    pub note: Option<String>,
    /// A = gemessen, B = plausibel, C = umstritten
    pub evidence: String,
    /// low | medium | high
    pub risk: String,
    #[serde(default)]
    pub reboot: bool,
    /// Flags wie "win11", "gpu.nvidia", "!laptop"
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub kind: Kind,
    #[serde(default)]
    pub irreversible: bool,
    pub actions: Vec<Action>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ActionView {
    pub label: String,
    pub target: String,
    pub current: String,
    /// applied | open | unsupported | error
    pub state: &'static str,
}

#[derive(Serialize, Clone, Debug)]
pub struct TweakView {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub note: Option<String>,
    pub evidence: String,
    pub risk: String,
    pub reboot: bool,
    pub kind: Kind,
    pub irreversible: bool,
    /// active | partial | inactive | unsupported | error | action
    pub status: &'static str,
    pub detail: String,
    /// Von Dialed gesetzt, aber inzwischen wieder abweichend.
    pub drift: bool,
    /// Von Dialed gesetzt (Journal-Eintrag vorhanden).
    pub managed: bool,
    pub actions: Vec<ActionView>,
}
