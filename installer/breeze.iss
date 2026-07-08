; Breeze - instalador autocontenido (FASE 7). FR-09 / NFR-09.
; Compilar: & 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe' installer\breeze.iss
;
; Nota (e): el desinstalador NO borra %APPDATA%\Breeze - config y logs del
; usuario se conservan. Decision estandar (no se agrega entrada [UninstallDelete]
; para ese path).

#define MyAppName "Breeze"
#define MyAppVersion "1.0.0"
#define MyAppExeName "breeze.exe"
#define ReleaseDir "..\build\windows\x64\runner\Release"
#define CudaBin "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6\bin"
; I-3: app-local VC++ runtime. A clean machine (FR-09.AC-1 target) lacks the
; MSVC CRT that the dev machine gets from Visual Studio. breeze.exe imports
; MSVCP140/VCRUNTIME140/VCRUNTIME140_1 (verified with dumpbin /dependents), so
; these three x64 redist DLLs are bundled next to the exe (app-local CRT).
#define VcRedist "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Redist\MSVC\14.50.35710\x64\Microsoft.VC145.CRT"

[Setup]
AppId={{AD0259D5-44C0-46F9-8D16-6E56E59EEBDF}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher=Breeze
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
OutputDir=output
OutputBaseFilename=breeze-setup-{#MyAppVersion}
SetupIconFile=..\assets\app_icon_multi.ico
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
PrivilegesRequiredOverridesAllowed=commandline dialog
MinVersion=10.0
UninstallDisplayIcon={app}\{#MyAppExeName}

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
spanish.AutoStart=Iniciar Breeze al iniciar sesion en Windows
english.AutoStart=Start Breeze when you sign in to Windows

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "autostart"; Description: "{cm:AutoStart}"; Flags: unchecked

[Files]
Source: "{#ReleaseDir}\breeze.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ReleaseDir}\*.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#ReleaseDir}\data\*"; DestDir: "{app}\data"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#ReleaseDir}\models\ggml-large-v3-turbo-q5_0.bin"; DestDir: "{app}\models"; Flags: ignoreversion
Source: "{#CudaBin}\cudart64_12.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#CudaBin}\cublas64_12.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#CudaBin}\cublasLt64_12.dll"; DestDir: "{app}"; Flags: ignoreversion
; I-3: app-local VC++ CRT so Breeze runs on a clean machine without the
; VC++ Redistributable installed (the exe imports these three DLLs).
Source: "{#VcRedist}\msvcp140.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#VcRedist}\vcruntime140.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#VcRedist}\vcruntime140_1.dll"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#MyAppName}"; ValueData: """{app}\{#MyAppExeName}"""; Flags: uninsdeletevalue; Tasks: autostart

[UninstallDelete]
; T4 triage: the NVIDIA driver drops umdlogs under {app}\NVIDIA Corporation at
; runtime; that residue survives uninstall otherwise. Remove it on uninstall.
Type: filesandordirs; Name: "{app}\NVIDIA Corporation"

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent
