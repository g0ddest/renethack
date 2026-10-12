; The Windows installer (tools/dist/windows.sh runs makensis with):
;   -DVERSION=<shown>  -DVERSION4=<a.b.c.d>  -DSRC=<package folder>
;   -DICON=<icon.ico>  -DOUT=<installer.exe>
; Installs for every user in Program Files, with a Start menu entry and an
; uninstaller; the saves stay in the player's AppData.

Unicode true
!include "MUI2.nsh"
!include "x64.nsh"

!define APP "renethack"
!define PUBLISHER "Vitaliy Velikodniy"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP}"

Name "${APP}"
OutFile "${OUT}"
InstallDir "$PROGRAMFILES64\${APP}"
InstallDirRegKey HKLM "Software\${APP}" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
ManifestDPIAware true

VIProductVersion "${VERSION4}"
VIAddVersionKey "ProductName" "${APP}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION4}"
VIAddVersionKey "FileDescription" "${APP} installer"
VIAddVersionKey "CompanyName" "${PUBLISHER}"
VIAddVersionKey "LegalCopyright" "© 2026 ${PUBLISHER}"

!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP}.exe"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Russian"

Function .onInit
    ${IfNot} ${RunningX64}
        MessageBox MB_ICONSTOP "${APP} needs 64-bit Windows."
        Abort
    ${EndIf}
    SetRegView 64
FunctionEnd

Function un.onInit
    SetRegView 64
FunctionEnd

Section "${APP}"
    SetOutPath "$INSTDIR"
    ; an update replaces the old files whole
    RMDir /r "$INSTDIR\engine"
    RMDir /r "$INSTDIR\legal"
    File /r "${SRC}\*.*"
    WriteUninstaller "$INSTDIR\uninstall.exe"
    CreateShortcut "$SMPROGRAMS\${APP}.lnk" "$INSTDIR\${APP}.exe"
    WriteRegStr HKLM "Software\${APP}" "InstallDir" "$INSTDIR"
    WriteRegStr HKLM "${UNINST_KEY}" "DisplayName" "${APP}"
    WriteRegStr HKLM "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr HKLM "${UNINST_KEY}" "Publisher" "${PUBLISHER}"
    WriteRegStr HKLM "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\${APP}.exe"
    WriteRegStr HKLM "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKLM "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegDWORD HKLM "${UNINST_KEY}" "NoModify" 1
    WriteRegDWORD HKLM "${UNINST_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
    Delete "$SMPROGRAMS\${APP}.lnk"
    ; only what the installer put there: the folder's own files
    RMDir /r "$INSTDIR\engine"
    RMDir /r "$INSTDIR\legal"
    Delete "$INSTDIR\${APP}.exe"
    Delete "$INSTDIR\${APP}.console.exe"
    Delete "$INSTDIR\${APP}.pck"
    Delete "$INSTDIR\*.dll"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"
    DeleteRegKey HKLM "${UNINST_KEY}"
    DeleteRegKey HKLM "Software\${APP}"
SectionEnd
