!define MIVIA_EXIT_SYSTEM 1625
!define MIVIA_EXIT_RUNNING 1618
!define MIVIA_APP_KEY "Software\MiViA\Desktop"
!define MIVIA_POLICY_KEY "Software\Policies\MiViA\Desktop"
!define MIVIA_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define MIVIA_KEPT_RUN "RunKeptForReinstall"
!define MIVIA_CRED_FILTER "MiViA Desktop/v1/*"

!macro MiviaExitIfSystem
  System::Alloc 68
  Pop $R8
  System::Call 'advapi32::CreateWellKnownSid(i 22, p 0, p R8, *i 68) i .R9'
  StrCpy $R7 0
  ${If} $R9 <> 0
    System::Call 'advapi32::CheckTokenMembership(p 0, p R8, *i .R7) i .R9'
  ${EndIf}
  System::Free $R8
  ${If} $R7 <> 0
    SetErrorLevel ${MIVIA_EXIT_SYSTEM}
    Quit
  ${EndIf}
!macroend

!macro MiviaExitIfRunning
  ${If} ${Silent}
  ${AndIf} $UpdateMode <> 1
    nsis_tauri_utils::FindProcessCurrentUser "${MAINBINARYNAME}.exe"
    Pop $R9
    ${If} $R9 = 0
      SetErrorLevel ${MIVIA_EXIT_RUNNING}
      Quit
    ${EndIf}
  ${EndIf}
!macroend

!macro MiviaPolicySwitch ROOT
  ClearErrors
  ReadRegDWORD $R9 ${ROOT} "${MIVIA_POLICY_KEY}" "Autostart"
  ${If} ${Errors}
    ClearErrors
    ReadRegStr $R9 ${ROOT} "${MIVIA_POLICY_KEY}" "Autostart"
  ${EndIf}
  ${If} $R9 == "off"
    StrCpy $R9 0
  ${ElseIf} $R9 == "on"
    StrCpy $R9 1
  ${ElseIf} $R9 != 0
  ${AndIf} $R9 != 1
    StrCpy $R9 ""
  ${EndIf}
!macroend

!macro MiviaRunValue
  nsis_tauri_utils::StrReplace "$INSTDIR\${MAINBINARYNAME}.exe" "$LOCALAPPDATA\" "%LOCALAPPDATA%\"
  Pop $R9
  WriteRegExpandStr HKCU "${MIVIA_RUN_KEY}" "${PRODUCTNAME}" '"$R9" --minimized'
!macroend

!macro MiviaAutostartBeforeSetup
  ReadRegDWORD $R8 HKCU "${MIVIA_APP_KEY}" "SetupCompleted"
  ${If} $R8 <> 1
    !insertmacro MiviaPolicySwitch HKLM
    ${If} $R9 == ""
      !insertmacro MiviaPolicySwitch HKCU
    ${EndIf}
    ${If} $R9 != 0
      !insertmacro MiviaRunValue
    ${EndIf}
  ${EndIf}
!macroend

!macro MiviaRestoreKeptRun
  ClearErrors
  ReadRegStr $R8 HKCU "${MIVIA_APP_KEY}" "${MIVIA_KEPT_RUN}"
  ${IfNot} ${Errors}
    DeleteRegValue HKCU "${MIVIA_APP_KEY}" "${MIVIA_KEPT_RUN}"
    ClearErrors
    ReadRegStr $R9 HKCU "${MIVIA_RUN_KEY}" "${PRODUCTNAME}"
    ${If} ${Errors}
      WriteRegExpandStr HKCU "${MIVIA_RUN_KEY}" "${PRODUCTNAME}" $R8
    ${EndIf}
  ${EndIf}
!macroend

!macro MiviaKeepRun
  ClearErrors
  ReadRegStr $R9 HKCU "${MIVIA_RUN_KEY}" "${PRODUCTNAME}"
  ${IfNot} ${Errors}
    WriteRegExpandStr HKCU "${MIVIA_APP_KEY}" "${MIVIA_KEPT_RUN}" $R9
  ${EndIf}
!macroend

!macro MiviaConfirmDataDeletion
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    ${If} $LANGUAGE = 1031
      StrCpy $R9 "Die Anwendungsdaten werden endgültig gelöscht: Einstellungen, Verlauf, gespeicherter Zugriffsschlüssel und Protokolle. Fortfahren?"
    ${Else}
      StrCpy $R9 "The application data will be deleted for good: settings, history, the saved access key and logs. Continue?"
    ${EndIf}
    MessageBox MB_YESNO|MB_ICONEXCLAMATION|MB_DEFBUTTON2 $R9 /SD IDNO IDYES +2
    StrCpy $DeleteAppDataCheckboxState 0
  ${EndIf}
!macroend

!macro MiviaDeleteCredentials
  System::Call 'advapi32::CredEnumerateW(w "${MIVIA_CRED_FILTER}", i 0, *i .R7, *p .R8) i .R9'
  ${If} $R9 <> 0
    StrCpy $R6 0
    ${DoWhile} $R6 < $R7
      IntOp $R5 $R6 * ${NSIS_PTR_SIZE}
      IntPtrOp $R5 $R8 + $R5
      System::Call '*$R5(p .R4)'
      System::Call '*$R4(i, i, w .R3)'
      System::Call 'advapi32::CredDeleteW(w R3, i 1, i 0)'
      IntOp $R6 $R6 + 1
    ${Loop}
    System::Call 'advapi32::CredFree(p R8)'
  ${EndIf}
!macroend

!macro MiviaRemoveTree TREE
  nsExec::Exec '"$SYSDIR\cmd.exe" /c rd /s /q "${TREE}"'
  Pop $R9
!macroend

!macro MiviaRemoveDataWithoutFollowingLinks
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    !insertmacro MiviaRemoveTree "$APPDATA\${BUNDLEID}"
    !insertmacro MiviaRemoveTree "$LOCALAPPDATA\${BUNDLEID}"
  ${EndIf}
!macroend

!macro MiviaRemoveShellEntries
  SetShellVarContext current
  Delete "$SENDTO\Analyze with MiViA.lnk"
  Delete "$SENDTO\Mit MiViA auswerten.lnk"
  !insertmacro MiviaRemoveVerb "Directory"
  !insertmacro MiviaRemoveVerb "SystemFileAssociations\.jpg"
  !insertmacro MiviaRemoveVerb "SystemFileAssociations\.jpeg"
  !insertmacro MiviaRemoveVerb "SystemFileAssociations\.png"
  !insertmacro MiviaRemoveVerb "SystemFileAssociations\.tif"
  !insertmacro MiviaRemoveVerb "SystemFileAssociations\.tiff"
  !insertmacro MiviaRemoveVerb "SystemFileAssociations\.bmp"
  DeleteRegKey HKCU "Software\Classes\AppUserModelId\${BUNDLEID}"
  !insertmacro MiviaRemoveTree "$LOCALAPPDATA\${BUNDLEID}\inbox"
  Delete "$LOCALAPPDATA\${BUNDLEID}\instance.nonce"
!macroend

!macro MiviaRemoveVerb CLASS
  DeleteRegKey HKCU "Software\Classes\${CLASS}\shell\MiViA.Desktop"
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro MiviaExitIfSystem
  !insertmacro MiviaExitIfRunning
!macroend

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "${MANUPRODUCTKEY}" "Installer Language" $LANGUAGE
  !insertmacro MiviaRestoreKeptRun
  !insertmacro MiviaAutostartBeforeSetup
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    !insertmacro MiviaKeepRun
  ${EndIf}
  !insertmacro MiviaConfirmDataDeletion
  !insertmacro MiviaRemoveDataWithoutFollowingLinks
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    !insertmacro MiviaRemoveShellEntries
    ${If} $DeleteAppDataCheckboxState = 1
      !insertmacro MiviaDeleteCredentials
      DeleteRegKey HKCU "${MIVIA_APP_KEY}"
      DeleteRegKey /ifempty HKCU "Software\MiViA"
    ${EndIf}
  ${EndIf}
!macroend
