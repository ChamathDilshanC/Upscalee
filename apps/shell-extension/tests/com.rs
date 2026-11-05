//! Exercises the COM surface exactly as Explorer would, without registering
//! anything: DllGetClassObject → IClassFactory → IExplorerCommand.

use std::ffi::c_void;

use upscalee_shell::{DllCanUnloadNow, DllGetClassObject, CLSID_CLASSIC, CLSID_PACKAGED, MENU_TITLE};
use windows::core::{Interface, GUID, HSTRING};
use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, S_FALSE, S_OK};
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, IClassFactory, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{
    IExplorerCommand, IShellItem, IShellItemArray, SHCreateItemFromParsingName, SHCreateShellItemArrayFromShellItem,
    ECS_ENABLED, ECS_HIDDEN,
};

fn init_com() {
    // SAFETY: per-thread COM init; repeated calls return S_FALSE.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
}

fn create_command(clsid: GUID) -> IExplorerCommand {
    let mut factory: *mut c_void = std::ptr::null_mut();
    // SAFETY: valid pointers to stack values.
    let hr = unsafe { DllGetClassObject(&clsid, &IClassFactory::IID, &mut factory) };
    assert_eq!(hr, S_OK);
    // SAFETY: DllGetClassObject returned an AddRef'd IClassFactory.
    let factory = unsafe { IClassFactory::from_raw(factory) };
    // SAFETY: standard CreateInstance call.
    unsafe { factory.CreateInstance::<Option<&windows::core::IUnknown>, IExplorerCommand>(None) }.unwrap()
}

fn item_array(path: &std::path::Path) -> IShellItemArray {
    // SAFETY: valid path string; COM initialised.
    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None).unwrap();
        SHCreateShellItemArrayFromShellItem(&item).unwrap()
    }
}

#[test]
fn rejects_unknown_classes() {
    let mut out: *mut c_void = std::ptr::null_mut();
    let bogus = GUID::from_u128(0x1234);
    // SAFETY: valid pointers.
    let hr = unsafe { DllGetClassObject(&bogus, &IClassFactory::IID, &mut out) };
    assert_eq!(hr, CLASS_E_CLASSNOTAVAILABLE);
    assert!(out.is_null());
}

#[test]
fn command_reports_title_name_and_visibility() {
    init_com();
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("photo.png");
    let txt = dir.path().join("notes.txt");
    std::fs::write(&png, b"not really a png, extension is all that matters").unwrap();
    std::fs::write(&txt, b"hello").unwrap();

    for clsid in [CLSID_CLASSIC, CLSID_PACKAGED] {
        let command = create_command(clsid);
        // SAFETY: COM calls on a live object; returned strings freed below.
        unsafe {
            let title = command.GetTitle(None).unwrap();
            assert_eq!(title.to_string().unwrap(), MENU_TITLE);
            CoTaskMemFree(Some(title.0 as *const _));

            assert_eq!(command.GetCanonicalName().unwrap(), clsid);
            assert_eq!(command.GetState(&item_array(&png), false).unwrap(), ECS_ENABLED.0 as u32);
            assert_eq!(command.GetState(&item_array(&txt), false).unwrap(), ECS_HIDDEN.0 as u32);
            assert_eq!(command.GetState(None, false).unwrap(), ECS_ENABLED.0 as u32);
            assert!(command.EnumSubCommands().is_err());
        }
    }
}

#[test]
fn unload_is_refused_while_objects_are_alive() {
    init_com();
    let command = create_command(CLSID_CLASSIC);
    assert_eq!(DllCanUnloadNow(), S_FALSE);
    drop(command);
    // Other tests may run concurrently and hold objects, so only assert the
    // "alive" half deterministically.
    let _ = DllCanUnloadNow();
}

#[test]
fn query_interface_works_for_iunknown() {
    init_com();
    let command = create_command(CLSID_CLASSIC);
    let unknown: windows::core::IUnknown = command.cast().unwrap();
    let back: IExplorerCommand = unknown.cast().unwrap();
    drop(back);
}
