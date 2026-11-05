//! `UpscaleeShell.dll` — the Explorer context-menu handler.
//!
//! Runs **inside Explorer** (classic menu, in-proc) or inside a COM surrogate
//! (`dllhost.exe`, Windows 11 modern menu via the sparse package). Rules:
//!
//! * never panic across the FFI boundary (every entry point is wrapped in
//!   `catch_unwind`),
//! * never block Explorer's UI thread (pipe I/O and process launch happen on a
//!   short-lived worker thread),
//! * never touch the file system in `GetState` (extension check only).
//!
//! The handler only collects the selected paths and the cursor position and
//! hands them to `Upscalee.exe` over the named pipe (see `upscalee-ipc`).

#![allow(non_snake_case)] // COM method names

mod command;
mod module;

use std::ffi::c_void;

use windows::core::{Interface, GUID, HRESULT};
use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, E_INVALIDARG, E_POINTER, S_FALSE, S_OK};
use windows::Win32::System::Com::IClassFactory;

pub use command::{UpscaleCommand, MENU_TITLE};

/// CLSID registered under `HKCU\Software\Classes\CLSID` for the classic menu
/// (`ExplorerCommandHandler` verb on Windows 10 and "Show more options").
pub const CLSID_CLASSIC: GUID = GUID::from_u128(0x58A04070_9783_4B15_AFFA_426AB1287F03);

/// CLSID declared in the sparse package manifest for the Windows 11 modern menu.
pub const CLSID_PACKAGED: GUID = GUID::from_u128(0xED8F549D_D8F5_460E_8938_D64A01BD7616);

fn is_known_clsid(clsid: &GUID) -> bool {
    *clsid == CLSID_CLASSIC || *clsid == CLSID_PACKAGED
}

/// Standard COM export: hand out our class factory.
///
/// # Safety
/// Called by COM with valid pointers (checked for null anyway).
#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    module::guard_hresult(|| {
        if ppv.is_null() {
            return E_POINTER;
        }
        // SAFETY: checked non-null above.
        unsafe { *ppv = std::ptr::null_mut() };
        if rclsid.is_null() || riid.is_null() {
            return E_INVALIDARG;
        }
        // SAFETY: checked non-null above.
        let clsid = unsafe { *rclsid };
        if !is_known_clsid(&clsid) {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = module::ClassFactory::new(clsid).into();
        // SAFETY: riid/ppv valid per COM contract.
        unsafe { factory.query(riid, ppv) }
    })
}

/// Standard COM export: allow unloading once no objects or worker threads remain.
#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if module::active_count() == 0 {
        S_OK
    } else {
        S_FALSE
    }
}

/// Remember our module handle (used to find `Upscalee.exe` next to the DLL).
///
/// # Safety
/// Called by the loader.
#[no_mangle]
pub unsafe extern "system" fn DllMain(
    instance: windows::Win32::Foundation::HINSTANCE,
    reason: u32,
    _reserved: *mut c_void,
) -> windows_core::BOOL {
    if reason == windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH {
        module::set_instance(instance);
    }
    windows_core::BOOL(1)
}
