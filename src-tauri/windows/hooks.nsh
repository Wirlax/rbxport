; Hooks the bundler splices into its NSIS installer (`bundle.windows.nsis.installerHooks`).

; Until 0.6.0 the app was called rekordbox-lite. The installer recognises an
; earlier install by product name, so an update from one of those would leave
; it in place beside rbxport: its Program Files directory, Start menu entry
; and Add/Remove row. Run its own uninstaller first. `/P` is its passive mode,
; which closes the app if it is still running and asks nothing; `_?=` runs it
; from its directory so ExecWait waits for it. Preferences and the session
; live under the bundle identifier, which did not change, and the uninstaller
; leaves them alone unless asked to clear app data.
!macro NSIS_HOOK_PREINSTALL
  ReadRegStr $R1 SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\rekordbox-lite" "UninstallString"
  ReadRegStr $R2 SHCTX "Software\chrisle\rekordbox-lite" ""
  ${If} $R1 != ""
  ${AndIf} $R2 != ""
    ExecWait '$R1 /P _?=$R2'
    ; Run in place, the uninstaller cannot remove itself; finish for it.
    Delete "$R2\uninstall.exe"
    RMDir "$R2"
  ${EndIf}
!macroend
