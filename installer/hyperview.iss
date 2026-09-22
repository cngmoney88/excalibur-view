; The Windows installer.
;
; Deliberately plain. It puts three things on a machine — the program, the
; pdfium library it draws with, and a folder for tool chests — and registers
; Hyperview as something that can open a PDF without taking the association away
; from whatever the person already uses.

#ifndef Version
  #define Version "0.1.0"
#endif
#ifndef Stage
  #define Stage "stage"
#endif
; The company this installer is for. Usually left out: a seat finds its own
; company's server on the local network and nobody types an address at all.
;
; Set it for the case discovery cannot cover — a server reached over a VPN or
; across a routed network, where a broadcast never arrives:
;
;   iscc hyperview.iss /DServer=https://drawings.acme.com /DCompany="Acme Steel"
;
; Even then it is a default and not a lock: what somebody types wins from then
; on, because an office that moves its server should not need reinstalling on
; six machines.
#ifndef Server
  #define Server ""
#endif
#ifndef Company
  #define Company ""
#endif

[Setup]
AppId={{B4E2F7A1-6C3D-4F8E-9A21-7D5C1E0B3A94}
AppName=Excalibur Hyperview
AppVersion={#Version}
AppPublisher=Mesa Fab, Inc.
DefaultDirName={autopf}\Excalibur Hyperview
DefaultGroupName=Excalibur Hyperview
OutputDir=dist
#if Company != ""
  #define Suffix "-" + Company
#else
  #define Suffix ""
#endif
OutputBaseFilename=Hyperview-{#Version}{#Suffix}-Setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; Per-machine when it can be, per-user when it cannot, so a seat without
; administrator rights can still install it.
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesInstallIn64BitMode=x64compatible
ArchitecturesAllowed=x64compatible
; Never install over a running copy: somebody may be three hours into a takeoff.
CloseApplications=yes
RestartApplications=no

[Files]
Source: "{#Stage}\Hyperview.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\pdfium.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\profiles\*"; DestDir: "{app}\profiles"; Flags: ignoreversion recursesubdirs createallsubdirs
; The standalone server, put on every seat rather than only on the server.
; It is one self-contained file and the machine that ends up holding the
; drawings is rarely the machine somebody planned for — copying it off a desk
; when that day comes beats hunting for the original download.
Source: "{#Stage}\hyperview-server.exe"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist

; Where this copy's server is. Written by the installer so the first person to
; open it sees their own company's name and a password box, not a question
; about an address they have never heard of.
#if Server != ""
[INI]
#endif

[Icons]
Name: "{group}\Excalibur Hyperview"; Filename: "{app}\Hyperview.exe"
Name: "{autodesktop}\Excalibur Hyperview"; Filename: "{app}\Hyperview.exe"; Tasks: desktopicon
; The front door for whoever walks over to the server. It is in the Start menu
; on every machine because the person who ends up doing it is not always the
; person who installed it, and "run this with a flag" is not an instruction to
; leave somebody with.
Name: "{group}\Hyperview Server Setup"; Filename: "{app}\Hyperview.exe"; \
  Parameters: "--host-setup"; Comment: "Set this computer up to hold the company's drawings"
; And the one-file way, for a machine nobody wants to install a service on.
; It serves in a command window and stops when the window is closed, which is
; a real answer for a shop where the same computer is on all day anyway.
Name: "{group}\Start the drawing server"; Filename: "{app}\hyperview-server.exe"; \
  Comment: "Hold the company's drawings in a window, until it is closed"

[Tasks]
Name: "desktopicon"; Description: "Put a shortcut on the desktop"; GroupDescription: "Shortcuts:"
Name: "openwith"; Description: "Offer Hyperview in Windows' ""Open with"" list for PDFs"; GroupDescription: "Drawings:"

; Registered as *a* program that opens PDFs, not as *the* one. Taking somebody's
; PDF association without asking is how a tool gets uninstalled.
[Registry]
Root: HKA; Subkey: "Software\Classes\Applications\Hyperview.exe\shell\open\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\Hyperview.exe"" ""%1"""; \
  Flags: uninsdeletekey; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\Applications\Hyperview.exe\SupportedTypes"; \
  ValueType: string; ValueName: ".pdf"; ValueData: ""; \
  Flags: uninsdeletekey; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\.pdf\OpenWithProgids"; \
  ValueType: string; ValueName: "Hyperview.Drawing"; ValueData: ""; \
  Flags: uninsdeletevalue; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\Hyperview.Drawing"; \
  ValueType: string; ValueName: ""; ValueData: "PDF Drawing"; \
  Flags: uninsdeletekey; Tasks: openwith
Root: HKA; Subkey: "Software\Classes\Hyperview.Drawing\shell\open\command"; \
  ValueType: string; ValueName: ""; ValueData: """{app}\Hyperview.exe"" ""%1"""; \
  Flags: uninsdeletekey; Tasks: openwith

[Code]
// Written rather than listed as a file, so one installer script builds a
// per-company installer without a per-company file sitting in the repository.
procedure CurStepChanged(CurStep: TSetupStep);
var
  Config: String;
begin
  if CurStep = ssPostInstall then
  begin
    if '{#Server}' <> '' then
    begin
      Config := '{' + #13#10 +
                '  "server": "{#Server}",' + #13#10 +
                '  "name": "{#Company}",' + #13#10 +
                '  "note": ""' + #13#10 +
                '}' + #13#10;
      SaveStringToFile(ExpandConstant('{app}\hyperview-server.json'), Config, False);
    end;
  end;
end;

[Run]
Filename: "{app}\Hyperview.exe"; Description: "Start Excalibur Hyperview"; \
  Flags: nowait postinstall skipifsilent

; If this machine was the one hosting the company's drawings, stop doing that
; before the program is deleted — a service pointing at an exe that is no longer
; there is a thing somebody finds in the event log a month later. The drawings
; themselves are left exactly where they are: uninstalling a program is not
; somebody asking for the company's drawings to be deleted.
[UninstallRun]
Filename: "{app}\Hyperview.exe"; Parameters: "--remove-service"; \
  Flags: runhidden skipifdoesntexist; RunOnceId: "StopServing"

; Settings and cached drawings belong to the person, not to the installation,
; so uninstalling leaves them where they are. Somebody reinstalling next week
; should find their preferences and their server still set up.
[UninstallDelete]
Type: filesandordirs; Name: "{app}\profiles\README.txt"
Type: files; Name: "{app}\hyperview-server.json"
