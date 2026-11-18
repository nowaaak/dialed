//! NVIDIA-Treibereinstellungen (DRS) im Basisprofil über nvapi64.dll.
//! Kennungen und Strukturen stammen aus NVIDIAs öffentlichen Headern (github.com/NVIDIA/nvapi).
use std::ffi::c_void;
use std::mem::{size_of, transmute, zeroed};
use std::ptr::null_mut;

type Handle = *mut c_void;

const OK: i32 = 0;
const SETTING_NOT_FOUND: i32 = -160;
const EXECUTABLE_NOT_FOUND: i32 = -166;
const DWORD_TYPE: u32 = 0;
const CURRENT_PROFILE_LOCATION: u32 = 0;

// Funktionskennungen aus nvapi_interface.h
const ID_INITIALIZE: u32 = 0x0150_E828;
const ID_CREATE_SESSION: u32 = 0x0694_D52E;
const ID_DESTROY_SESSION: u32 = 0xDAD9_CFF8;
const ID_LOAD_SETTINGS: u32 = 0x375D_BD6B;
const ID_SAVE_SETTINGS: u32 = 0xFCBC_7E14;
const ID_GET_BASE_PROFILE: u32 = 0xDA84_66A0;
const ID_GET_SETTING: u32 = 0x73BF_8338;
const ID_SET_SETTING: u32 = 0x577D_D202;
const ID_RESTORE_DEFAULT: u32 = 0x53F0_381E;
const ID_FIND_APPLICATION: u32 = 0xEEE5_66B2;
const ID_DISPLAY_ID_BY_NAME: u32 = 0xAE45_7190;
const ID_GET_VRR_INFO: u32 = 0xDF8F_DA57;

/// NVDRS_SETTING_V1 (pack 4). Die beiden Unions sind je 4100 Bytes groß
/// (größtes Mitglied: NVDRS_BINARY_SETTING mit 4 + 4096 Bytes).
#[repr(C)]
struct Setting {
    version: u32,
    name: [u16; 2048],
    id: u32,
    kind: u32,
    location: u32,
    is_current_predefined: u32,
    is_predefined_valid: u32,
    predefined: [u8; 4100],
    current: [u8; 4100],
}

const _: () = assert!(size_of::<Setting>() == 12320);
/// MAKE_NVAPI_VERSION(NVDRS_SETTING_V1, 1)
const SETTING_VERSION: u32 = size_of::<Setting>() as u32 | (1 << 16);

#[repr(C)]
struct Application {
    version: u32,
    is_predefined: u32,
    app_name: [u16; 2048],
    friendly_name: [u16; 2048],
    launcher: [u16; 2048],
}

const _: () = assert!(size_of::<Application>() == 12296);
const APPLICATION_VERSION: u32 = size_of::<Application>() as u32 | (1 << 16);

#[repr(C)]
struct VrrInfo {
    version: u32,
    flags: u32,
    reserved: [u32; 4],
}

const _: () = assert!(size_of::<VrrInfo>() == 24);
const VRR_INFO_VERSION: u32 = size_of::<VrrInfo>() as u32 | (1 << 16);
const VRR_ENABLED: u32 = 1;
const VRR_POSSIBLE: u32 = 1 << 1;
const VRR_REQUESTED: u32 = 1 << 2;
const VRR_DISPLAY_IN_MODE: u32 = 1 << 4;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
}

type QueryFn = unsafe extern "C" fn(u32) -> *mut c_void;
type InitFn = unsafe extern "C" fn() -> i32;
type CreateFn = unsafe extern "C" fn(*mut Handle) -> i32;
type SessionFn = unsafe extern "C" fn(Handle) -> i32;
type BaseProfileFn = unsafe extern "C" fn(Handle, *mut Handle) -> i32;
type GetFn = unsafe extern "C" fn(Handle, Handle, u32, *mut Setting) -> i32;
type SetFn = unsafe extern "C" fn(Handle, Handle, *mut Setting) -> i32;
type RestoreFn = unsafe extern "C" fn(Handle, Handle, u32) -> i32;
type DisplayIdFn = unsafe extern "C" fn(*const u8, *mut u32) -> i32;
type VrrFn = unsafe extern "C" fn(u32, *mut VrrInfo) -> i32;
type FindAppFn = unsafe extern "C" fn(Handle, *const u16, *mut Handle, *mut Application) -> i32;

fn check(status: i32, what: &str) -> Result<(), String> {
    if status == OK {
        Ok(())
    } else {
        Err(format!("NVIDIA-Treiber meldet Fehler {status} bei {what}"))
    }
}

struct Api {
    query: QueryFn,
}

impl Api {
    fn load() -> Result<Api, String> {
        let name: Vec<u16> = "nvapi64.dll\0".encode_utf16().collect();
        // SAFETY: gültige, nullterminierte Zeichenketten; Rückgaben werden auf null geprüft.
        unsafe {
            let module = LoadLibraryW(name.as_ptr());
            if module.is_null() {
                return Err("NVIDIA-Treiber (nvapi64.dll) nicht gefunden".into());
            }
            let query = GetProcAddress(module, b"nvapi_QueryInterface\0".as_ptr());
            if query.is_null() {
                return Err("nvapi64.dll ohne nvapi_QueryInterface".into());
            }
            Ok(Api {
                query: transmute::<*mut c_void, QueryFn>(query),
            })
        }
    }

    fn func(&self, id: u32) -> Result<*mut c_void, String> {
        // SAFETY: nvapi_QueryInterface nimmt eine Kennung und liefert einen Funktionszeiger oder null.
        let ptr = unsafe { (self.query)(id) };
        if ptr.is_null() {
            Err(format!("NVIDIA-Treiber kennt Funktion {id:#010x} nicht"))
        } else {
            Ok(ptr)
        }
    }
}

/// Öffnet eine Sitzung, lädt die Einstellungen und ruft `f` mit Sitzung und Basisprofil auf.
fn with_profile<T>(
    app: Option<&str>,
    save: bool,
    f: impl FnOnce(&Api, Handle, Option<Handle>) -> Result<T, String>,
) -> Result<T, String> {
    let api = Api::load()?;
    // SAFETY: Signaturen entsprechen den Deklarationen in nvapi.h (cdecl, x64).
    unsafe {
        let init: InitFn = transmute(api.func(ID_INITIALIZE)?);
        let create: CreateFn = transmute(api.func(ID_CREATE_SESSION)?);
        let destroy: SessionFn = transmute(api.func(ID_DESTROY_SESSION)?);
        let load: SessionFn = transmute(api.func(ID_LOAD_SETTINGS)?);
        let store: SessionFn = transmute(api.func(ID_SAVE_SETTINGS)?);

        check(init(), "Initialize")?;
        let mut session: Handle = null_mut();
        check(create(&mut session), "CreateSession")?;
        let result = (|| {
            check(load(session), "LoadSettings")?;
            let mut profile: Handle = null_mut();
            let found = match app {
                None => {
                    let base: BaseProfileFn = transmute(api.func(ID_GET_BASE_PROFILE)?);
                    check(base(session, &mut profile), "GetBaseProfile")?;
                    true
                }
                Some(name) => {
                    let find: FindAppFn = transmute(api.func(ID_FIND_APPLICATION)?);
                    let mut wide = [0u16; 2048];
                    for (slot, c) in wide.iter_mut().zip(name.encode_utf16().take(2047)) {
                        *slot = c;
                    }
                    let mut info: Box<Application> = Box::new(zeroed());
                    info.version = APPLICATION_VERSION;
                    match find(session, wide.as_ptr(), &mut profile, &mut *info) {
                        OK => true,
                        EXECUTABLE_NOT_FOUND => false,
                        status => check(status, "FindApplicationByName").map(|_| false)?,
                    }
                }
            };
            let out = f(&api, session, found.then_some(profile))?;
            if save {
                check(store(session), "SaveSettings")?;
            }
            Ok(out)
        })();
        destroy(session);
        result
    }
}

fn new_setting(id: u32) -> Box<Setting> {
    // SAFETY: Setting besteht nur aus Ganzzahlen und Byte-Feldern, alles-null ist gültig.
    let mut s: Box<Setting> = Box::new(unsafe { zeroed() });
    s.version = SETTING_VERSION;
    s.id = id;
    s
}

/// Liest DWORD-Einstellungen aus dem Basisprofil. None = im Profil nicht gesetzt.
pub fn read(app: Option<&str>, ids: &[u32]) -> Result<Vec<Option<u32>>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    with_profile(app, false, |api, session, profile| {
        let Some(profile) = profile else {
            return Ok(vec![None; ids.len()]);
        };
        // SAFETY: Signatur laut nvapi.h; der Puffer hat die im Versionsfeld gemeldete Größe.
        let get: GetFn = unsafe { transmute(api.func(ID_GET_SETTING)?) };
        let mut out = Vec::with_capacity(ids.len());
        for &id in ids {
            let mut s = new_setting(id);
            match unsafe { get(session, profile, id, &mut *s) } {
                OK if s.kind == DWORD_TYPE
                    && (app.is_none() || s.location == CURRENT_PROFILE_LOCATION) =>
                {
                    out.push(Some(u32::from_le_bytes([
                        s.current[0],
                        s.current[1],
                        s.current[2],
                        s.current[3],
                    ])))
                }
                OK | SETTING_NOT_FOUND => out.push(None),
                status => {
                    return Err(format!(
                        "NVIDIA-Treiber meldet Fehler {status} beim Lesen von {id:#010x}"
                    ))
                }
            }
        }
        Ok(out)
    })
}

/// Setzt eine DWORD-Einstellung (Some) oder stellt den Treiberstandard wieder her (None).
pub fn write(app: Option<&str>, id: u32, value: Option<u32>) -> Result<(), String> {
    with_profile(app, true, |api, session, profile| match (profile, value) {
        (None, Some(_)) => Err(format!(
            "Der NVIDIA-Treiber hat kein Profil für {}",
            app.unwrap_or("das Basisprofil")
        )),
        (None, None) => Ok(()),
        (Some(profile), Some(v)) => {
            // SAFETY: Signatur laut nvapi.h.
            let set: SetFn = unsafe { transmute(api.func(ID_SET_SETTING)?) };
            let mut s = new_setting(id);
            s.kind = DWORD_TYPE;
            s.current[..4].copy_from_slice(&v.to_le_bytes());
            check(unsafe { set(session, profile, &mut *s) }, "SetSetting")
        }
        (Some(profile), None) => {
            // SAFETY: Signatur laut nvapi.h.
            let restore: RestoreFn = unsafe { transmute(api.func(ID_RESTORE_DEFAULT)?) };
            match unsafe { restore(session, profile, id) } {
                OK | SETTING_NOT_FOUND => Ok(()),
                status => Err(format!(
                    "NVIDIA-Treiber meldet Fehler {status} beim Zurücksetzen"
                )),
            }
        }
    })
}

pub fn vrr_flags(displays: &[String]) -> Result<Vec<u32>, String> {
    let api = Api::load()?;
    unsafe {
        let init: InitFn = transmute(api.func(ID_INITIALIZE)?);
        check(init(), "Initialize")?;
        let by_name: DisplayIdFn = transmute(api.func(ID_DISPLAY_ID_BY_NAME)?);
        let get: VrrFn = transmute(api.func(ID_GET_VRR_INFO)?);
        let mut out = Vec::new();
        for name in displays {
            let mut raw = name.as_bytes().to_vec();
            raw.push(0);
            let mut id = 0u32;
            if by_name(raw.as_ptr(), &mut id) != OK {
                continue;
            }
            let mut info: VrrInfo = zeroed();
            info.version = VRR_INFO_VERSION;
            if get(id, &mut info) == OK {
                out.push(info.flags);
            }
        }
        Ok(out)
    }
}

pub fn vrr(displays: &[String]) -> Result<bool, String> {
    Ok(vrr_flags(displays)?.into_iter().any(|f| {
        f & VRR_POSSIBLE != 0 && f & (VRR_ENABLED | VRR_REQUESTED | VRR_DISPLAY_IN_MODE) != 0
    }))
}
