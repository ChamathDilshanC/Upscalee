//! Module bookkeeping: object/thread counting for `DllCanUnloadNow`, the class
//! factory, panic guards and locating `Upscalee.exe`.

use std::ffi::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

use windows::core::{implement, Interface, Ref, GUID, HRESULT, PCWSTR};
use windows::Win32::Foundation::{CLASS_E_NOAGGREGATION, E_POINTER, E_UNEXPECTED, HINSTANCE, HMODULE};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};
use windows::Win32::System::Diagnostics::Debug::OutputDebugStringW;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::UI::Shell::IExplorerCommand;

use crate::command::UpscaleCommand;

static INSTANCE: AtomicIsize = AtomicIsize::new(0);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

pub fn set_instance(instance: HINSTANCE) {
    INSTANCE.store(instance.0 as isize, Ordering::Relaxed);
}

/// Live COM objects + server locks + running worker threads.
pub fn active_count() -> usize {
    ACTIVE.load(Ordering::Acquire)
}

/// RAII token that keeps the DLL loaded while it exists.
pub struct ActiveGuard(());

impl ActiveGuard {
    pub fn new() -> Self {
        ACTIVE.fetch_add(1, Ordering::AcqRel);
        Self(())
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Run `f`, turning a panic into `E_UNEXPECTED` instead of unwinding into Explorer.
pub fn guard<T>(f: impl FnOnce() -> windows::core::Result<T>) -> windows::core::Result<T> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        debug_log("panic caught at COM boundary");
        Err(E_UNEXPECTED.into())
    })
}

pub fn guard_hresult(f: impl FnOnce() -> HRESULT) -> HRESULT {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(E_UNEXPECTED)
}

/// Diagnostics visible in DebugView / a debugger; never shown to users.
pub fn debug_log(message: &str) {
    let text: Vec<u16> = format!("[UpscaleeShell] {message}\n").encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: NUL-terminated buffer that outlives the call.
    unsafe { OutputDebugStringW(PCWSTR(text.as_ptr())) };
}

/// Full path of this DLL, if known.
pub fn module_path() -> Option<PathBuf> {
    let instance = INSTANCE.load(Ordering::Relaxed);
    let module = HMODULE(instance as *mut c_void);
    let mut buf = vec![0u16; 1024];
    loop {
        // SAFETY: buffer is valid for its length. A null module means "the exe",
        // which is only the case in tests.
        let len = unsafe { GetModuleFileNameW(Some(module), &mut buf) } as usize;
        if len == 0 {
            return None;
        }
        if len < buf.len() {
            return Some(PathBuf::from(String::from_utf16_lossy(&buf[..len])));
        }
        if buf.len() >= 32 * 1024 {
            return None;
        }
        buf.resize(buf.len() * 2, 0);
    }
}

/// `Upscalee.exe` lives next to `UpscaleeShell.dll` in the install folder.
pub fn app_executable() -> Option<PathBuf> {
    let dll = module_path()?;
    let exe = dll.parent()?.join("Upscalee.exe");
    exe.is_file().then_some(exe)
}

#[implement(IClassFactory)]
pub struct ClassFactory {
    clsid: GUID,
    _active: ActiveGuard,
}

impl ClassFactory {
    pub fn new(clsid: GUID) -> Self {
        Self { clsid, _active: ActiveGuard::new() }
    }
}

impl IClassFactory_Impl for ClassFactory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<'_, windows::core::IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> windows::core::Result<()> {
        guard(|| {
            if ppvobject.is_null() || riid.is_null() {
                return Err(E_POINTER.into());
            }
            // SAFETY: checked non-null.
            unsafe { *ppvobject = std::ptr::null_mut() };
            if outer.is_some() {
                return Err(CLASS_E_NOAGGREGATION.into());
            }
            let command: IExplorerCommand = UpscaleCommand::new(self.clsid).into();
            // SAFETY: COM guarantees riid/ppvobject validity.
            unsafe { command.query(riid, ppvobject) }.ok()
        })
    }

    fn LockServer(&self, lock: windows_core::BOOL) -> windows::core::Result<()> {
        if lock.as_bool() {
            ACTIVE.fetch_add(1, Ordering::AcqRel);
        } else {
            ACTIVE.fetch_sub(1, Ordering::AcqRel);
        }
        Ok(())
    }
}
