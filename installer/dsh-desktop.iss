; DSH Desktop — Windows installer (Inno Setup 6/7)
; Build with: ISCC.exe dsh-desktop.iss

#define AppName "DSH Desktop"
#define AppVersion "1.2.0"
#define AppExeName "dsh-desktop.exe"

[Setup]
AppId={{ECF52751-03C1-4407-8715-1DC38537B90D}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=DSH Desktop
DefaultDirName={localappdata}\Programs\DSH Desktop
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=DSH-Desktop-Setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupIconFile=../assets/dsh-desktop.ico
UninstallDisplayIcon={app}\{#AppExeName}
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "chinesesimp"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式(&D)"; GroupDescription: "附加任务："; Flags: unchecked

[Files]
Source: "..\target\release\{#AppExeName}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExeName}"; Description: "运行 {#AppName}"; Flags: nowait postinstall skipifsilent
