; silence! installer. Built by build.ps1:
;   makensis /DVERSION=2.4.1 /DARCH=x64 /DSOURCE_EXE=<path> /DOUT_FILE=<path> /DICON=<path> installer\silence.nsi
; Identifier, install path and registry keys match the former Dioxus/Tauri
; bundle (fun.silencemute) so existing installs upgrade in place.
Unicode true

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"

!ifndef VERSION
  !error "VERSION is required"
!endif
!ifndef SOURCE_EXE
  !error "SOURCE_EXE is required"
!endif
!ifndef OUT_FILE
  !error "OUT_FILE is required"
!endif
!ifndef ICON
  !error "ICON is required"
!endif

!define APP_NAME "silence!"
!define PUBLISHER "vertopolkaLF"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\fun.silencemute"
!define SILENCE_V1_UNINSTALL_KEY "{8E4D9F2A-3B7C-4E1F-A5D6-9C8B2E7F4A3D}_is1"

Name "${APP_NAME}"
OutFile "${OUT_FILE}"
Icon "${ICON}"
UninstallIcon "${ICON}"
InstallDir "$LOCALAPPDATA\Programs\silence!"
RequestExecutionLevel user

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APP_NAME}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${APP_NAME} installer"
VIAddVersionKey "CompanyName" "${PUBLISHER}"

!define MUI_ABORTWARNING
!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_FINISHPAGE_RUN "$INSTDIR\silence.exe"
!define MUI_FINISHPAGE_RUN_PARAMETERS "--post-install"
!define MUI_FINISHPAGE_RUN_TEXT "Launch silence!"

!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "Install"
    Call RemoveSilenceV1
    nsExec::ExecToLog 'taskkill.exe /f /im "silence.exe"'

    SetOutPath $INSTDIR
    ; The Dioxus builds shipped CSS/fonts/icons beside the exe; everything is
    ; embedded now.
    RMDir /r "$INSTDIR\assets"
    File "/oname=silence.exe" "${SOURCE_EXE}"
    File "/oname=app.ico" "${ICON}"

    WriteUninstaller "$INSTDIR\uninstall.exe"

    CreateDirectory "$SMPROGRAMS\silence!"
    CreateShortcut "$SMPROGRAMS\silence!\silence!.lnk" "$INSTDIR\silence.exe" "" "$INSTDIR\app.ico" 0
    CreateShortcut "$SMPROGRAMS\silence!\Uninstall silence!.lnk" "$INSTDIR\uninstall.exe" "" "$INSTDIR\app.ico" 0
    CreateShortcut "$DESKTOP\silence!.lnk" "$INSTDIR\silence.exe" "" "$INSTDIR\app.ico" 0

    WriteRegStr SHCTX "${UNINSTALL_KEY}" "DisplayName" "${APP_NAME}"
    WriteRegStr SHCTX "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\silence.exe,0"
    WriteRegStr SHCTX "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr SHCTX "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
    WriteRegStr SHCTX "${UNINSTALL_KEY}" "Publisher" "${PUBLISHER}"
    WriteRegStr SHCTX "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr SHCTX "${UNINSTALL_KEY}" "HelpLink" "https://github.com/vertopolkaLF/silence/issues"
    WriteRegStr SHCTX "${UNINSTALL_KEY}" "URLUpdateInfo" "https://github.com/vertopolkaLF/silence/releases/latest"

    ; Register launch with Windows.
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Run" "silence!" '"$INSTDIR\silence.exe"'

    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    IntFmt $0 "0x%08X" $0
    WriteRegDWORD SHCTX "${UNINSTALL_KEY}" "EstimatedSize" "$0"

    IfSilent 0 +2
    Exec '"$INSTDIR\silence.exe" --post-install'
SectionEnd

Function RunSilenceV1Uninstaller
    Pop $0
    ${If} $0 != ""
        nsExec::ExecToLog 'taskkill.exe /f /im "silence!.exe"'
        ExecWait '$0 /VERYSILENT /SUPPRESSMSGBOXES /NORESTART'
    ${EndIf}
FunctionEnd

!macro RemoveSilenceV1From ROOT
    ReadRegStr $0 ${ROOT} "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SILENCE_V1_UNINSTALL_KEY}" "QuietUninstallString"
    ${If} $0 == ""
        ReadRegStr $0 ${ROOT} "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SILENCE_V1_UNINSTALL_KEY}" "UninstallString"
    ${EndIf}
    ${If} $0 != ""
        Push $0
        Call RunSilenceV1Uninstaller
    ${EndIf}
!macroend

Function RemoveSilenceV1
    !insertmacro RemoveSilenceV1From HKCU
    SetRegView 64
    !insertmacro RemoveSilenceV1From HKLM
    SetRegView 32
    !insertmacro RemoveSilenceV1From HKLM
    SetRegView lastused
FunctionEnd

Section "Uninstall"
    nsExec::ExecToLog 'taskkill.exe /f /im "silence.exe"'
    RMDir /r "$INSTDIR"
    RMDir /r "$SMPROGRAMS\silence!"
    Delete "$DESKTOP\silence!.lnk"
    DeleteRegValue SHCTX "Software\Microsoft\Windows\CurrentVersion\Run" "silence!"
    DeleteRegKey SHCTX "${UNINSTALL_KEY}"
SectionEnd
