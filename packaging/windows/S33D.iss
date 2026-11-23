; Build with scripts/package-windows.ps1 after building both release executables.
; StageDir contains only the audited payload assembled by that script.
#ifndef AppVersion
  #define AppVersion "0.1.0-alpha.1"
#endif
#ifndef FileVersion
  #define FileVersion "0.1.0.0"
#endif
#ifndef BuildId
  #error BuildId is required; use scripts/package-windows.ps1.
#endif
#ifndef StageDir
  #error StageDir is required; use scripts/package-windows.ps1.
#endif
#ifndef PackageOutputDir
  #error PackageOutputDir is required; use scripts/package-windows.ps1.
#endif
#if !FileExists(StageDir + "\assets\icons\s33d-seed.ico")
  #error The final seed installer icon is required.
#endif
#if !FileExists(StageDir + "\assets\icons\s33d-sprouted.ico")
  #error The final sprouted application icon is required.
#endif
#if !FileExists(StageDir + "\source\S33D-" + AppVersion + "-source.zip")
  #error The corresponding source archive is required.
#endif

[Setup]
AppId={{C9E61EA5-2E48-451C-A949-AF22067508AB}
AppName=S33D Alpha Preview
AppVersion={#AppVersion}
AppVerName=S33D Alpha Preview {#AppVersion}
AppPublisher=YDZM
VersionInfoVersion={#FileVersion}
VersionInfoProductVersion={#FileVersion}
VersionInfoProductTextVersion={#AppVersion}
VersionInfoTextVersion={#AppVersion}
VersionInfoDescription=S33D Alpha Preview Setup
VersionInfoCompany=YDZM
DefaultDirName={localappdata}\Programs\S33D Alpha Preview
DefaultGroupName=S33D Alpha Preview
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64os
ArchitecturesInstallIn64BitMode=x64os
MinVersion=10.0.17763
WizardStyle=modern
SetupIconFile={#StageDir}\assets\icons\s33d-seed.ico
UninstallDisplayIcon={app}\assets\icons\s33d-sprouted.ico
UninstallDisplayName=S33D Alpha Preview {#AppVersion}
InfoBeforeFile={#StageDir}\INSTALL-README.txt
OutputDir={#PackageOutputDir}
OutputBaseFilename=S33D-{#AppVersion}-build-{#BuildId}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
CloseApplications=no
RestartApplications=no
Uninstallable=yes
SetupLogging=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#StageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\S33D Alpha Preview"; Filename: "{app}\s33d-app.exe"; WorkingDir: "{app}"; IconFilename: "{app}\assets\icons\s33d-sprouted.ico"
Name: "{autodesktop}\S33D Alpha Preview"; Filename: "{app}\s33d-app.exe"; WorkingDir: "{app}"; IconFilename: "{app}\assets\icons\s33d-sprouted.ico"; Tasks: desktopicon
Name: "{group}\Uninstall S33D Alpha Preview"; Filename: "{uninstallexe}"

; Deliberately no Run, Registry, InstallDelete, or UninstallDelete sections.
; Uninstall removes only installed files/shortcuts tracked by Inno Setup.
; User projects, %LOCALAPPDATA%\S33D, and external Agent registration survive.

[Code]
function OpenUpdateTarget(FileName: String; DesiredAccess, ShareMode: LongWord;
  SecurityAttributes: THandle; CreationDisposition, FlagsAndAttributes: LongWord;
  TemplateFile: THandle): THandle;
  external 'CreateFileW@kernel32.dll stdcall';

function CloseUpdateTarget(Handle: THandle): Boolean;
  external 'CloseHandle@kernel32.dll stdcall';

function CanUpdateExecutable(const FileName: String): Boolean;
var
  Handle: THandle;
begin
  Result := True;
  if not FileExists(FileName) then
    Exit;
  { Check write access without changing bytes. Sharing zero also detects a
    running executable or another process holding the file open. }
  Handle := OpenUpdateTarget(FileName, $C0000000, 0, 0, 3, $80, 0);
  if Handle = THandle(-1) then
    Result := False
  else
    CloseUpdateTarget(Handle);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if not CanUpdateExecutable(ExpandConstant('{app}\s33d-app.exe')) or
     not CanUpdateExecutable(ExpandConstant('{app}\s33d-agent.exe')) then begin
    Log('S33D upgrade preflight blocked before file copy.');
    Result := 'S33D cannot be updated while its program or Agent files are in use or not writable. ' +
      'Close S33D and its Agent connections, check access to the installation folder, then retry. ' +
      'No application files have been replaced.';
  end;
end;
