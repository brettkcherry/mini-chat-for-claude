; Custom NSIS hooks, wired up via bundle.windows.nsis.installerHooks in
; tauri.conf.json. Tauri inserts these macros into its own installer.nsi.

!macro NSIS_HOOK_PREUNINSTALL
  ; The Anthropic API key lives in Windows Credential Manager, not under
  ; %APPDATA%. The uninstaller's built-in "delete application data" checkbox
  ; only runs `RmDir /r` over $APPDATA\${BUNDLEID} and $LOCALAPPDATA\${BUNDLEID},
  ; so it cannot reach the credential. Without this hook, a user who ticks that
  ; box believes their data is gone while a live API key stays on the machine —
  ; with the app that managed it now uninstalled.
  ;
  ; Both conditions below mirror the template's own app-data deletion exactly,
  ; and both are load-bearing:
  ;
  ;   $DeleteAppDataCheckboxState = 1
  ;       The user explicitly asked for their data to be removed. Leaving it
  ;       unticked means "keep my setup for a reinstall", and the saved key is
  ;       part of that setup.
  ;
  ;   $UpdateMode <> 1
  ;       An application update re-runs THIS uninstaller with /UPDATE before
  ;       installing the new version. Deleting the key in that path would sign
  ;       the user out on every single auto-update.
  ;
  ; Removing the credential is delegated to the app binary rather than done
  ; inline with `cmdkey` on purpose: the exact target name is an internal detail
  ; of the keyring crate (currently "{user}.{service}", i.e.
  ; "anthropic-api-key.claude-mini"). Calling back into the same code that wrote
  ; the credential means this script cannot silently drift out of sync with it
  ; if that format ever changes.
  ;
  ; Note: $DeleteAppDataCheckboxState, $UpdateMode and ${MAINBINARYNAME} are
  ; supplied by Tauri's installer template. If a future Tauri release renames
  ; them, this file fails to compile during bundling — a loud build break rather
  ; than a silently skipped cleanup, which is the tradeoff we want here.
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
      ; Best effort — an uninstall must never fail because cleanup did, so the
      ; exit code is not captured at all. Passing an output register here would
      ; clobber it for the rest of the uninstall section for no benefit.
      ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-cleanup'
    ${EndIf}
  ${EndIf}
!macroend
