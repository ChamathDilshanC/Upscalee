; Upscalee hooks for Tauri's NSIS installer template.
; Referenced from apps/desktop/src-tauri/tauri.conf.json (bundle.windows.nsis.installerHooks).
; Available from the template: $INSTDIR, ${MAINBINARYNAME}, $UpdateMode,
; $DeleteAppDataCheckboxState and LogicLib (${If} ...).

!macro NSIS_HOOK_PREINSTALL
  ; Explorer keeps UpscaleeShell.dll loaded while a context menu has been shown.
  ; A loaded DLL cannot be overwritten, but it can be renamed: move it aside so
  ; the new version can be copied. The app deletes the .old file on next start.
  ${If} ${FileExists} "$INSTDIR\UpscaleeShell.dll"
    Delete "$INSTDIR\UpscaleeShell.dll.old"
    Rename "$INSTDIR\UpscaleeShell.dll" "$INSTDIR\UpscaleeShell.dll.old"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Register the context menu (classic + Windows 11 package when shipped),
  ; the COM class and Run-at-login. Per-user (HKCU); never fails the install.
  nsExec::Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --register'
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    nsExec::Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --unregister'
    Pop $0
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Explorer may still hold the DLL until it restarts; remove what we can.
  Delete "$INSTDIR\UpscaleeShell.dll.old"
  Delete "$INSTDIR\UpscaleeShell.dll"
  RmDir "$INSTDIR"
  ; Settings, logs and scratch live in %APPDATA% / %LOCALAPPDATA%\<bundle id>,
  ; which the template's "Delete the application data" checkbox already removes.
!macroend
