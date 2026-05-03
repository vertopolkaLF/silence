!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"

!define SILENCE_V1_UNINSTALL_KEY "{8E4D9F2A-3B7C-4E1F-A5D6-9C8B2E7F4A3D}_is1"

; Basic installer attributes
Name "silence!"
OutFile "{{output_path}}"
Unicode true
{{#if installer_icon}}
Icon "{{installer_icon}}"
UninstallIcon "{{installer_icon}}"
{{/if}}
{{#if install_mode_per_machine}}
InstallDir "$PROGRAMFILES\silence!"
{{else}}
InstallDir "$LOCALAPPDATA\Programs\silence!"
{{/if}}

; Request appropriate privileges
{{#if install_mode_per_machine}}
RequestExecutionLevel admin
{{else if install_mode_both}}
RequestExecutionLevel admin
{{else}}
RequestExecutionLevel user
{{/if}}

; Version information
VIProductVersion "{{version}}.0"
VIAddVersionKey "ProductName" "silence!"
VIAddVersionKey "FileVersion" "{{version}}"
VIAddVersionKey "ProductVersion" "{{version}}"
VIAddVersionKey "FileDescription" "{{short_description}}"
{{#if publisher}}
VIAddVersionKey "CompanyName" "{{publisher}}"
{{/if}}
{{#if copyright}}
VIAddVersionKey "LegalCopyright" "{{copyright}}"
{{/if}}

; MUI settings
!define MUI_ABORTWARNING
{{#if installer_icon}}
!define MUI_ICON "{{installer_icon}}"
{{/if}}
{{#if header_image}}
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP "{{header_image}}"
{{/if}}
{{#if sidebar_image}}
!define MUI_WELCOMEFINISHPAGE_BITMAP "{{sidebar_image}}"
{{/if}}
!define MUI_FINISHPAGE_RUN "$INSTDIR\{{main_binary_name}}"
!define MUI_FINISHPAGE_RUN_TEXT "Launch silence!"

; Pages
{{#if license}}
!insertmacro MUI_PAGE_LICENSE "{{license}}"
{{/if}}
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

; Uninstaller pages
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; Language
!insertmacro MUI_LANGUAGE "English"
{{#each additional_languages}}
!insertmacro MUI_LANGUAGE "{{this}}"
{{/each}}

; Installer section
Section "Install"
    Call RemoveSilenceV1

    SetOutPath $INSTDIR

    ; Install main binary
    File "{{main_binary_path}}"

    ; Install resources
    {{#each staged_files}}
    SetOutPath "$INSTDIR{{#if this.target_dir}}\{{this.target_dir}}{{/if}}"
    File "{{this.source}}"
    {{/each}}

    SetOutPath $INSTDIR

    ; Create uninstaller
    WriteUninstaller "$INSTDIR\uninstall.exe"

    ; Create Start Menu shortcuts
    CreateDirectory "$SMPROGRAMS\{{start_menu_folder}}"
    CreateShortcut "$SMPROGRAMS\{{start_menu_folder}}\silence!.lnk" "$INSTDIR\{{main_binary_name}}"
    CreateShortcut "$SMPROGRAMS\{{start_menu_folder}}\Uninstall silence!.lnk" "$INSTDIR\uninstall.exe"

    ; Create Desktop shortcut
    CreateShortcut "$DESKTOP\silence!.lnk" "$INSTDIR\{{main_binary_name}}"

    ; Write registry keys for Add/Remove Programs
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "DisplayName" "silence!"
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "DisplayIcon" "$INSTDIR\{{main_binary_name}},0"
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "DisplayVersion" "{{version}}"
    {{#if publisher}}
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "Publisher" "{{publisher}}"
    {{/if}}
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "InstallLocation" "$INSTDIR"
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "HelpLink" "https://github.com/vertopolkaLF/silence/issues"
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "URLUpdateInfo" "https://github.com/vertopolkaLF/silence/releases/latest"

    ; Register launch with Windows.
    WriteRegStr SHCTX "Software\Microsoft\Windows\CurrentVersion\Run" \
        "silence!" '"$INSTDIR\{{main_binary_name}}"'

    ; Get installed size
    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    IntFmt $0 "0x%08X" $0
    WriteRegDWORD SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}" \
        "EstimatedSize" "$0"

    {{#if install_webview}}
    ; WebView2 installation
    {{webview_install_code}}
    {{/if}}

SectionEnd

Function RunSilenceV1Uninstaller
    Pop $0
    ${If} $0 != ""
        ExecWait 'taskkill.exe /f /im "silence!.exe"'
        ExecWait '$0 /VERYSILENT /SUPPRESSMSGBOXES /NORESTART'
    ${EndIf}
FunctionEnd

Function RemoveSilenceV1
    ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SILENCE_V1_UNINSTALL_KEY}" "QuietUninstallString"
    ${If} $0 == ""
        ReadRegStr $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SILENCE_V1_UNINSTALL_KEY}" "UninstallString"
    ${EndIf}
    ${If} $0 != ""
        Push $0
        Call RunSilenceV1Uninstaller
    ${EndIf}

    SetRegView 64
    ReadRegStr $0 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SILENCE_V1_UNINSTALL_KEY}" "QuietUninstallString"
    ${If} $0 == ""
        ReadRegStr $0 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SILENCE_V1_UNINSTALL_KEY}" "UninstallString"
    ${EndIf}
    ${If} $0 != ""
        Push $0
        Call RunSilenceV1Uninstaller
    ${EndIf}

    SetRegView 32
    ReadRegStr $0 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SILENCE_V1_UNINSTALL_KEY}" "QuietUninstallString"
    ${If} $0 == ""
        ReadRegStr $0 HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${SILENCE_V1_UNINSTALL_KEY}" "UninstallString"
    ${EndIf}
    ${If} $0 != ""
        Push $0
        Call RunSilenceV1Uninstaller
    ${EndIf}
    SetRegView lastused
FunctionEnd

{{#if installer_hooks}}
!include "{{installer_hooks}}"
{{/if}}

; Uninstaller section
Section "Uninstall"
    ; Remove files
    RMDir /r "$INSTDIR"

    ; Remove Start Menu items
    RMDir /r "$SMPROGRAMS\{{start_menu_folder}}"

    ; Remove Desktop shortcut
    Delete "$DESKTOP\silence!.lnk"

    ; Remove registry keys
    DeleteRegValue SHCTX "Software\Microsoft\Windows\CurrentVersion\Run" "silence!"
    DeleteRegKey SHCTX "Software\Microsoft\Windows\CurrentVersion\Uninstall\{{bundle_id}}"
SectionEnd
