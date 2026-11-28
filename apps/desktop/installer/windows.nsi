Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"

!ifndef APP_VERSION
  !error "APP_VERSION is required"
!endif
!ifndef APP_EXE
  !error "APP_EXE is required"
!endif
!ifndef CORE_EXE
  !error "CORE_EXE is required"
!endif
!ifndef RESOURCE_DIR
  !error "RESOURCE_DIR is required"
!endif
!ifndef OUTPUT
  !error "OUTPUT is required"
!endif

Name "JARVIS"
Caption "JARVIS ${APP_VERSION}"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\JARVIS"
InstallDirRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JARVIS" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\jarvis-desktop.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Avvia JARVIS al termine"
!define MUI_FINISHPAGE_TITLE "Installazione completata"
!define MUI_FINISHPAGE_TEXT "JARVIS è stato installato. I tuoi dati restano nella cartella utente anche se disinstalli l'app."
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Italian"

!macro StopInstalledJarvis
  InitPluginsDir
  SetOutPath "$PLUGINSDIR"
  File /oname=stop-windows.ps1 "stop-windows.ps1"
  StrCpy $1 "$SYSDIR\WindowsPowerShell\v1.0\powershell.exe"
  ; NSIS is 32-bit. Use 64-bit PowerShell to inspect the paths of the x64 processes.
  ${If} ${FileExists} "$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
    StrCpy $1 "$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe"
  ${EndIf}
  nsExec::ExecToStack /TIMEOUT=30000 '"$1" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\stop-windows.ps1" -InstallDir "$INSTDIR\."'
  Pop $0
  Pop $2
  ${If} $0 != 0
    FileOpen $3 "$TEMP\JARVIS-setup-error.txt" w
    FileWrite $3 "Arresto processi ($0)$\r$\n$2"
    FileClose $3
    SetErrorLevel 10
    ${IfNot} ${Silent}
      MessageBox MB_ICONSTOP "Impossibile arrestare JARVIS. Chiudi l'app e riprova."
    ${EndIf}
    Abort
  ${EndIf}
!macroend

Section "JARVIS" Install
  ${If} ${FileExists} "$INSTDIR\jarvis-desktop.exe"
  ${OrIf} ${FileExists} "$INSTDIR\jarvis.exe"
  ${OrIf} ${FileExists} "$INSTDIR\jarvis-core.exe"
  ${OrIf} ${FileExists} "$INSTDIR\resources\jarvis-core.exe"
    !insertmacro StopInstalledJarvis
  ${EndIf}
  SetOutPath "$INSTDIR"
  File "${APP_EXE}"
  File "${CORE_EXE}"
  SetOutPath "$INSTDIR\resources"
  File /r "${RESOURCE_DIR}\*"
  CreateDirectory "$SMPROGRAMS\JARVIS"
  CreateShortCut "$SMPROGRAMS\JARVIS\JARVIS.lnk" "$INSTDIR\jarvis-desktop.exe" "" "$INSTDIR\resources\icon.ico"
  CreateShortCut "$DESKTOP\JARVIS.lnk" "$INSTDIR\jarvis-desktop.exe" "" "$INSTDIR\resources\icon.ico"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JARVIS" "DisplayName" "JARVIS"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JARVIS" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JARVIS" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JARVIS" "Publisher" "JARVIS"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JARVIS" "UninstallString" '"$INSTDIR\uninstall.exe"'
SectionEnd

Section "Uninstall"
  !insertmacro StopInstalledJarvis
  SetOutPath "$TEMP"
  ClearErrors
  Delete "$INSTDIR\jarvis-desktop.exe"
  Delete "$INSTDIR\jarvis.exe"
  Delete "$INSTDIR\jarvis-core.exe"
  ${If} ${FileExists} "$INSTDIR\resources\."
    RMDir /r "$INSTDIR\resources"
  ${EndIf}
  ${If} ${Errors}
    FileOpen $3 "$TEMP\JARVIS-setup-error.txt" w
    FileWrite $3 "Rimozione dei file del programma non riuscita."
    FileClose $3
    SetErrorLevel 11
    ${IfNot} ${Silent}
      MessageBox MB_ICONSTOP "Alcuni file di JARVIS sono ancora in uso. Riprova a disinstallare."
    ${EndIf}
    Abort
  ${EndIf}
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\JARVIS\JARVIS.lnk"
  RMDir "$SMPROGRAMS\JARVIS"
  Delete "$DESKTOP\JARVIS.lnk"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JARVIS"
SectionEnd
