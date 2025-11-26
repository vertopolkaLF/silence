; Inno Setup Script for silence! - x64 version
; Compile with: iscc /DMyAppVersion=1.1 silence-x64.iss

#define MyAppName "silence!"
#ifndef MyAppVersion
  #define MyAppVersion "1.1"
#endif
#define MyAppPublisher "vertopolkaLF"
#define MyAppURL "https://github.com/vertopolkaLF/silence"
#define MyAppExeName "silence!.exe"
#define MyAppArch "x64"
#define SourcePath "..\releases\silence-v" + MyAppVersion + "-win-x64"

[Setup]
; NOTE: The value of AppId uniquely identifies this application.
AppId={{8E4D9F2A-3B7C-4E1F-A5D6-9C8B2E7F4A3D}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
; Output settings
OutputDir=..\releases
OutputBaseFilename=silence-v{#MyAppVersion}-{#MyAppArch}-setup
SetupIconFile=..\Assets\app.ico
; Compression
Compression=lzma2/ultra64
SolidCompression=yes
LZMAUseSeparateProcess=yes
; Visual settings
WizardStyle=modern
; Architecture
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Privileges
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
; Uninstaller
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "russian"; MessagesFile: "compiler:Languages\Russian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "startupicon"; Description: "Start with Windows"; GroupDescription: "Startup:"; Flags: unchecked

[Files]
; Main application files - copy everything from the release folder
Source: "{#SourcePath}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
; NOTE: Don't use "Flags: ignoreversion" on any shared system files

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Registry]
; Add to Windows startup if task selected
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#MyAppName}"; ValueData: """{app}\{#MyAppExeName}"""; Flags: uninsdeletevalue; Tasks: startupicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[Code]
// Check if app is running before uninstall
function InitializeUninstall(): Boolean;
var
  ResultCode: Integer;
begin
  Result := True;
  // Try to close the app gracefully
  if CheckForMutexes('silence_app_mutex') then
  begin
    if MsgBox('silence! is currently running. Close it to continue uninstall?', mbConfirmation, MB_YESNO) = IDYES then
    begin
      Exec('taskkill.exe', '/f /im "silence!.exe"', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
      Sleep(500);
    end
    else
      Result := False;
  end;
end;

