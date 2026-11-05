//! The `IExplorerCommand` implementation.

use std::time::Duration;

use upscalee_ipc::client::send_or_launch;
use upscalee_ipc::{has_supported_extension, Message, Origin, Point, Request};
use windows::core::{implement, Ref, Result, GUID, HSTRING, PWSTR};
use windows::Win32::Foundation::{E_NOTIMPL, POINT};
use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx};
use windows::Win32::UI::Shell::{
    IEnumExplorerCommand, IExplorerCommand, IExplorerCommand_Impl, IShellItemArray, SHStrDupW, ECF_DEFAULT,
    ECS_ENABLED, ECS_HIDDEN, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use crate::module::{self, guard, ActiveGuard};

pub const MENU_TITLE: &str = "Upscale with Upscalee";

/// Upper bound for how many selected items we inspect / forward.
const MAX_ITEMS: u32 = 2_000;

/// How long the worker waits for a freshly launched app to start listening.
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(20);

#[implement(IExplorerCommand)]
pub struct UpscaleCommand {
    clsid: GUID,
    _active: ActiveGuard,
}

impl UpscaleCommand {
    pub fn new(clsid: GUID) -> Self {
        Self { clsid, _active: ActiveGuard::new() }
    }
}

/// File-system paths of the selected items (non-file items are ignored).
pub fn selected_paths(items: &IShellItemArray, limit: u32) -> Vec<String> {
    // SAFETY: COM calls on a valid interface pointer.
    let count = unsafe { items.GetCount() }.unwrap_or(0).min(limit);
    let mut paths = Vec::with_capacity(count as usize);
    for i in 0..count {
        // SAFETY: index < count; returned PWSTR is freed with CoTaskMemFree.
        unsafe {
            let Ok(item) = items.GetItemAt(i) else { continue };
            let Ok(name) = item.GetDisplayName(SIGDN_FILESYSPATH) else {
                continue;
            };
            if let Ok(path) = name.to_string() {
                paths.push(path);
            }
            CoTaskMemFree(Some(name.0 as *const _));
        }
    }
    paths
}

fn dup_string(text: &str) -> Result<PWSTR> {
    // SAFETY: SHStrDupW copies into CoTaskMem that the caller frees.
    unsafe { SHStrDupW(&HSTRING::from(text)) }
}

fn cursor_position() -> Option<Point> {
    let mut point = POINT::default();
    // SAFETY: plain out-parameter.
    unsafe { GetCursorPos(&mut point) }.ok()?;
    Some(Point { x: point.x, y: point.y })
}

/// Forward a pick to the app on a worker thread (never blocks Explorer).
pub fn dispatch_pick(paths: Vec<String>, cursor: Option<Point>) {
    let guard_token = ActiveGuard::new();
    let spawned = std::thread::Builder::new().name("upscalee-shell-dispatch".into()).spawn(move || {
        let _keep_loaded = guard_token;
        let outcome = std::panic::catch_unwind(move || {
            let Some(exe) = module::app_executable() else {
                module::debug_log("Upscalee.exe not found next to the shell extension");
                return;
            };
            let message = Message::new(Request::Pick { paths, cursor, origin: Origin::ShellExtension });
            if let Err(err) = send_or_launch(&message, &exe, LAUNCH_TIMEOUT) {
                module::debug_log(&format!("could not reach Upscalee: {err}"));
            }
        });
        if outcome.is_err() {
            module::debug_log("panic in dispatch thread");
        }
    });
    if spawned.is_err() {
        module::debug_log("could not start dispatch thread");
    }
}

impl IExplorerCommand_Impl for UpscaleCommand_Impl {
    fn GetTitle(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        guard(|| dup_string(MENU_TITLE))
    }

    fn GetIcon(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        guard(|| match module::app_executable() {
            Some(exe) => dup_string(&format!("{},0", exe.display())),
            None => Err(E_NOTIMPL.into()),
        })
    }

    fn GetToolTip(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(self.clsid)
    }

    fn GetState(&self, items: Ref<'_, IShellItemArray>, _ok_to_be_slow: windows_core::BOOL) -> Result<u32> {
        guard(|| {
            let Some(items) = items.as_ref() else {
                return Ok(ECS_ENABLED.0 as u32);
            };
            let visible = selected_paths(items, MAX_ITEMS).iter().any(|p| has_supported_extension(p));
            Ok(if visible { ECS_ENABLED.0 as u32 } else { ECS_HIDDEN.0 as u32 })
        })
    }

    fn Invoke(&self, items: Ref<'_, IShellItemArray>, _bind: Ref<'_, IBindCtx>) -> Result<()> {
        guard(|| {
            let Some(items) = items.as_ref() else {
                return Ok(());
            };
            let paths: Vec<String> =
                selected_paths(items, MAX_ITEMS).into_iter().filter(|p| has_supported_extension(p)).collect();
            if !paths.is_empty() {
                dispatch_pick(paths, cursor_position());
            }
            Ok(())
        })
    }

    fn GetFlags(&self) -> Result<u32> {
        Ok(ECF_DEFAULT.0 as u32)
    }

    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        Err(E_NOTIMPL.into())
    }
}

// Silence "unused" for the interface import used only through `#[implement]`.
#[allow(dead_code)]
fn _assert_interface(_: &IExplorerCommand) {}
