Unicode true
!include "MUI2.nsh"

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
RequestExecutionLevel user
SetCompressor /SOLID lzma
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Italian"

Section "JARVIS" Install
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
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JARVIS" "UninstallString" '"$INSTDIR\uninstall.exe"'
SectionEnd

Section "Uninstall"
  Delete "$SMPROGRAMS\JARVIS\JARVIS.lnk"
  RMDir "$SMPROGRAMS\JARVIS"
  Delete "$DESKTOP\JARVIS.lnk"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\JARVIS"
  RMDir /r "$INSTDIR"
SectionEnd
