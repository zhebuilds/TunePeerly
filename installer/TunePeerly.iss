; Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
;
; Compile this file from installer\TunePeerly.iss. Paths below are relative to
; that script. {src} is only valid while Setup is running, so it is not used here.
; Output is installer\Output, which is gitignored.

#define MyAppName "TunePeerly"
#define MyAppVersion "0.8.0"
; Prerelease suffix used in the installer file name only. Keep it non-empty: the
; OutputBaseFilename line below joins the three defines with "-", so an empty value
; would produce "TunePeerly-0.8.0--x64-Setup".
#define MyAppRelease "alpha.1"
#define MyAppPublisher "zhebuilds"
#define MyAppURL "https://github.com/zhebuilds/TunePeerly"
#define MyAppExeName "TunePeerly.exe"
#define DoubleAmp(Value) StringChange(Value, "&", "&&")
#define EscapeConstArgument(Value) StringChange(StringChange(StringChange(Value, "%", "%25"), ",", "%2c"), "}", "%7d")

[Setup]
; NOTE: The value of AppId uniquely identifies this application. Do not use the same AppId value in installers for other applications.
; (To generate a new GUID, click Tools | Generate GUID inside the IDE.)
AppId={{1664919A-C312-43AD-9824-BB5760DC3978}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
;AppVerName={cm:NameAndVersion,{#EscapeConstArgument(MyAppName)},{#EscapeConstArgument(MyAppVersion)}}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
VersionInfoCompany={#MyAppPublisher}
VersionInfoProductName={#MyAppName} Setup
VersionInfoVersion={#MyAppVersion}
VersionInfoDescription={#MyAppName} {#MyAppVersion} setup
VersionInfoCopyright=Copyright (c) 2026 zhebuilds and contributors
DefaultDirName={autopf64}\{#MyAppName}
UninstallDisplayIcon={app}\{#MyAppExeName}
; 64-bit Setup avoids 32-bit registry and System32 redirection. x64compatible still includes Windows 11 on ARM.
SetupArchitecture=x64
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
; The running app holds Local\com.zhebuilds.tunepeerly-sim. Closing the window can leave it running in the tray.
CloseApplications=no
RestartApplications=no
AppMutex=Local\com.zhebuilds.tunepeerly-sim
DisableProgramGroupPage=yes
LicenseFile=..\LICENSE
InfoBeforeFile=ADDITIONAL_TERMS.txt
; lowest makes "current user" the default. The dialog still offers "all users".
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
OutputDir=Output
OutputBaseFilename={#MyAppName}-{#MyAppVersion}-{#MyAppRelease}-x64-Setup
SetupIconFile=..\src-tauri\icons\icon.ico
SolidCompression=yes
WizardStyle=modern dynamic

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "chinesesimplified"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"

[Messages]
english.SetupAppRunningError=TunePeerly is still running.%n%nClosing the window can leave it in the notification area. Right-click that icon and choose Quit. Then click OK to continue, or Cancel to exit.
chinesesimplified.SetupAppRunningError=TunePeerly 仍在运行。%n%n关闭窗口可能只是收进通知区域。请在该图标上右键并选择“退出”，然后单击“确定”继续，或单击“取消”退出。

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\src-tauri\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "verify-authenticode.ps1"; Flags: dontcopy
; NOTE: Don't use "Flags: ignoreversion" on any shared system files.

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#DoubleAmp(MyAppName)}}"; WorkingDir: "{app}"; Flags: nowait postinstall skipifsilent runasoriginaluser

[Code]
const
  WebView2Key = 'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';
  // Evergreen Standalone Installer x64. Microsoft redirects this link to the current file, so the hash is not pinned.
  // The download progress on this page is the runtime itself. The app is x64, including on Windows 11 ARM.
  WebView2Url = 'https://go.microsoft.com/fwlink/?linkid=2124701';
  WebView2File = 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe';
  { Documented command. /silent must be first. Other switches are not supported. }
  WebView2Params = '/silent /install';

var
  DownloadPage: TDownloadWizardPage;
  DependencyRestart: Boolean;

function IsChinese: Boolean;
begin
  Result := ActiveLanguage = 'chinesesimplified';
end;

function TextEnZh(const English, Chinese: String): String;
begin
  if IsChinese then
    Result := Chinese
  else
    Result := English;
end;

function WebView2VersionOk(const Version: String): Boolean;
var
  Value: String;
begin
  Value := Trim(Version);
  Result :=
    (Value <> '') and
    (CompareText(Value, '0.0.0.0') <> 0) and
    (Pos('\', Value) = 0) and
    (Pos('/', Value) = 0) and
    (Pos('..', Value) = 0);
end;

function ReadWebView2Version(const RootKey: Integer; var Version: String): Boolean;
begin
  Result := False;
  if not RegQueryStringValue(RootKey, WebView2Key, 'pv', Version) then
    Exit;
  Version := Trim(Version);
  Result := WebView2VersionOk(Version);
end;

function WebView2RuntimeFile(const Version, Root: String): String;
begin
  Result := Root + '\Microsoft\EdgeWebView\Application\' + Version + '\msedgewebview2.exe';
end;

function WebView2FilesPresent(const Version: String; PerUser: Boolean): Boolean;
var
  Candidate: String;
begin
  if PerUser then
  begin
    Candidate := WebView2RuntimeFile(Version, ExpandConstant('{localappdata}'));
    Result := FileExists(Candidate);
    if not Result then
      Log('WebView2 runtime file missing: ' + Candidate);
  end
  else
  begin
    Candidate := WebView2RuntimeFile(Version, ExpandConstant('{commonpf32}'));
    Result := FileExists(Candidate);
    if Result then
      Exit;
    Log('WebView2 runtime file missing: ' + Candidate);
    Candidate := WebView2RuntimeFile(Version, ExpandConstant('{commonpf64}'));
    Result := FileExists(Candidate);
    if not Result then
      Log('WebView2 runtime file missing: ' + Candidate);
  end;
end;

function WebView2At(const RootKey: Integer; PerUser: Boolean): Boolean;
var
  Version: String;
begin
  Result := False;
  if not ReadWebView2Version(RootKey, Version) then
    Exit;
  { A registry version alone is not enough. Windows Sandbox can keep pv while leaving out msedgewebview2.exe. }
  if not WebView2FilesPresent(Version, PerUser) then
    Exit;
  Log('WebView2 runtime ' + Version);
  Result := True;
end;

function WebView2Installed: Boolean;
begin
  { Per-machine registration on 64-bit Windows is in the 32-bit view. HKLM64 is checked separately. HKCU is a per-user install. }
  if WebView2At(HKLM32, False) then
    Result := True
  else if WebView2At(HKLM64, False) then
    Result := True
  else
    Result := WebView2At(HKCU, True);
  if not Result then
    Log('WebView2 runtime is not installed.');
end;

function PowerShellExe: String;
begin
  Result := ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe');
end;

function RunPowerShell(const ScriptName, Arguments: String; var Code: Integer): Boolean;
var
  Params: String;
begin
  Params := '-NoProfile -ExecutionPolicy Bypass -File "' + ExpandConstant('{tmp}\' + ScriptName) + '"';
  if Arguments <> '' then
    Params := Params + ' ' + Arguments;
  Result := Exec(PowerShellExe, Params, '', SW_HIDE, ewWaitUntilTerminated, Code);
end;

function MicrosoftSigned(const FileName: String; var Detail: String): Boolean;
var
  Code: Integer;
begin
  if not RunPowerShell('verify-authenticode.ps1', '-Path "' + FileName + '"', Code) then
  begin
    Detail := TextEnZh(
      'Could not check the Microsoft signature.',
      '无法检查 Microsoft 签名。');
    Result := False;
    Exit;
  end;
  Result := Code = 0;
  if not Result then
    Detail := TextEnZh(
      'The downloaded file does not have a valid Microsoft signature.',
      '下载的文件没有有效的 Microsoft 签名。');
end;

function InstallerSucceeded(const Code: Integer; AllowAlreadyInstalled: Boolean; var NeedsRestart: Boolean; var Detail: String): Boolean;
begin
  if (Code = 0) or (AllowAlreadyInstalled and (Code = 1638)) then
  begin
    Result := True;
    Exit;
  end;
  if (Code = 3010) or (Code = 1641) then
  begin
    NeedsRestart := True;
    Result := True;
    Exit;
  end;
  Detail := TextEnZh(
    'The Microsoft installer exited with code ' + IntToStr(Code) + '.',
    'Microsoft 安装程序退出代码为 ' + IntToStr(Code) + '。');
  { 0x80040902: the per-machine WebView2 package did not finish. }
  if Code = -2147219198 then
    Detail := Detail + #13#10 + TextEnZh(
      'The per-machine WebView2 install did not finish.',
      'WebView2 的全机安装没有完成。');
  Result := False;
end;

function RunDependency(const FileName, Params: String; AllowAlreadyInstalled: Boolean; var NeedsRestart: Boolean; var ExitCode: Integer; var Detail: String): Boolean;
var
  Code: Integer;
begin
  ExitCode := -1;
  if not Exec(FileName, Params, '', SW_SHOWNORMAL, ewWaitUntilTerminated, Code) then
  begin
    Detail := TextEnZh(
      'Could not start the Microsoft installer.',
      '无法启动 Microsoft 安装程序。');
    Result := False;
    Exit;
  end;
  ExitCode := Code;
  Result := InstallerSucceeded(Code, AllowAlreadyInstalled, NeedsRestart, Detail);
end;

function DependencyInstalled(const Kind: Integer): Boolean;
begin
  case Kind of
    1: Result := WebView2Installed;
  else
    Result := False;
  end;
end;

function AcceptInstalled(const DisplayName: String; Kind: Integer; RestartedNow: Boolean): Boolean;
begin
  if RestartedNow then
  begin
    DependencyRestart := True;
    Result := True;
    Exit;
  end;
  Result := DependencyInstalled(Kind);
  if not Result then
    SuppressibleMsgBox(TextEnZh(
      DisplayName + ' reported success, but it is still not available.',
      DisplayName + ' 报告成功，但系统中仍然不可用。'), mbCriticalError, MB_OK, IDOK);
end;

function InstallDownloaded(const DisplayName, BaseName, Params: String; Kind: Integer; AllowAlreadyInstalled: Boolean): Boolean;
var
  Downloaded, Detail: String;
  RestartedNow: Boolean;
  ExitCode: Integer;
begin
  Result := False;
  Detail := '';
  RestartedNow := False;
  ExitCode := -1;
  Downloaded := ExpandConstant('{tmp}\' + BaseName);
  try
    DownloadPage.Msg1Label.Caption := TextEnZh(
      'Checking ' + DisplayName + '...',
      '正在检查 ' + DisplayName + '...');
    DownloadPage.Msg2Label.Caption := TextEnZh(
      'Cancel on this page stops Setup before TunePeerly is copied.',
      '此页面上的取消会在复制 TunePeerly 之前停止安装。');
    if not MicrosoftSigned(Downloaded, Detail) then
    begin
      SuppressibleMsgBox(Detail, mbCriticalError, MB_OK, IDOK);
      Exit;
    end;
    DownloadPage.Msg1Label.Caption := TextEnZh(
      'Installing ' + DisplayName + '...',
      '正在安装 ' + DisplayName + '...');
    DownloadPage.Msg2Label.Caption := TextEnZh(
      'This installer has no Cancel button.',
      '这个安装程序没有取消按钮。');
    if not RunDependency(Downloaded, Params, AllowAlreadyInstalled, RestartedNow, ExitCode, Detail) then
    begin
      { Sandbox has a per-machine Edge updater, so an unelevated install is switched to per-machine and returns 0x80040902. }
      if (not IsAdminInstallMode) and (ExitCode = -2147219198) and
         (SuppressibleMsgBox(TextEnZh(
           'Microsoft Edge WebView2 Runtime has to be installed for all users. This needs an administrator account. Continue?',
           'Microsoft Edge WebView2 运行库需要为所有用户安装。这需要管理员权限。是否继续？'),
           mbConfirmation, MB_YESNO, IDNO) = IDYES) then
      begin
        DownloadPage.Msg2Label.Caption := TextEnZh(
          'Approve the administrator prompt to continue.',
          '请在管理员提示中允许继续。');
        RestartedNow := False;
        if not ShellExec('runas', Downloaded, Params, '', SW_SHOWNORMAL, ewWaitUntilTerminated, ExitCode) then
          Detail := TextEnZh(
            'The administrator prompt was declined.',
            '管理员提示被拒绝。')
        else if InstallerSucceeded(ExitCode, AllowAlreadyInstalled, RestartedNow, Detail) then
        begin
          Result := AcceptInstalled(DisplayName, Kind, RestartedNow);
          Exit;
        end;
      end;
      SuppressibleMsgBox(Detail, mbCriticalError, MB_OK, IDOK);
      Exit;
    end;
    Result := AcceptInstalled(DisplayName, Kind, RestartedNow);
  finally
    DeleteFile(Downloaded);
  end;
end;

function MissingDependencyPrompt(NeedWebView2: Boolean): Boolean;
var
  Missing: TStringList;
  Prompt: String;
begin
  Missing := TStringList.Create;
  try
    if NeedWebView2 then
      Missing.Add('Microsoft Edge WebView2 Runtime x64');
    Prompt := TextEnZh(
      'TunePeerly needs these Microsoft components:'#13#10#13#10 + Missing.Text + #13#10 +
      'Setup can download them from Microsoft. Cancel on the download page stops Setup before TunePeerly is copied.',
      'TunePeerly 需要这些 Microsoft 组件：'#13#10#13#10 + Missing.Text + #13#10 +
      '安装程序可以从 Microsoft 下载它们。下载页面上的取消会在复制 TunePeerly 之前停止安装。');
  finally
    Missing.Free;
  end;
  { A silent install with /SUPPRESSMSGBOXES goes ahead and downloads them. }
  Result := SuppressibleMsgBox(Prompt, mbConfirmation, MB_YESNO, IDYES) = IDYES;
end;

procedure InitializeWizard;
begin
  DependencyRestart := False;
  DownloadPage := CreateDownloadPage(
    SetupMessage(msgWizardPreparing),
    SetupMessage(msgPreparingDesc),
    nil);
  DownloadPage.ShowBaseNameInsteadOfUrl := True;
end;

function NextButtonClick(CurPageID: Integer): Boolean;
var
  NeedWebView2: Boolean;
begin
  Result := True;
  if CurPageID <> wpReady then
    Exit;

  NeedWebView2 := not WebView2Installed;
  if not NeedWebView2 then
    Exit;

  if not MissingDependencyPrompt(NeedWebView2) then
  begin
    Result := False;
    Exit;
  end;

  DownloadPage.Clear;
  if NeedWebView2 then
  begin
    Log('Downloading ' + WebView2File + ' to ' + ExpandConstant('{tmp}'));
    DownloadPage.Add(WebView2Url, WebView2File, '');
  end;

  ExtractTemporaryFile('verify-authenticode.ps1');
  DownloadPage.Show;
  try
    try
      DownloadPage.Download;
    except
      Result := False;
      if DownloadPage.AbortedByUser then
        Log('Dependency download cancelled.')
      else
        SuppressibleMsgBox(TextEnZh(
          'Could not download a Microsoft component. ' + GetExceptionMessage + #13#10 +
          'If this computer is offline, connect and try again. TunePeerly was not copied.',
          '无法下载 Microsoft 组件。' + GetExceptionMessage + #13#10 +
          '如果这台电脑处于离线状态，请联网后重试。TunePeerly 尚未复制。'),
          mbCriticalError, MB_OK, IDOK);
      Exit;
    end;

    if NeedWebView2 then
      Result := InstallDownloaded(
        'Microsoft Edge WebView2 Runtime x64', WebView2File,
        WebView2Params, 1, True);
  finally
    if not Result then
      DeleteFile(ExpandConstant('{tmp}\' + WebView2File));
    DownloadPage.Hide;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if DependencyRestart then
    NeedsRestart := True;
end;

function IsReparsePoint(const Path: String): Boolean;
var
  FindRec: TFindRec;
  Plain: String;
begin
  Result := False;
  Plain := RemoveBackslashUnlessRoot(Path);
  if not FindFirst(Plain, FindRec) then
    Exit;
  try
    Result := (FindRec.Attributes and FILE_ATTRIBUTE_REPARSE_POINT) <> 0;
  finally
    FindClose(FindRec);
  end;
end;

procedure DeleteOneDataDir(const Path: String);
var
  Plain: String;
begin
  Plain := RemoveBackslashUnlessRoot(Path);
  if not DirExists(Plain) then
    Exit;
  if CompareText(ExtractFileName(Plain), 'TunePeerly') <> 0 then
  begin
    Log('Refusing to delete ' + Plain);
    Exit;
  end;
  if IsReparsePoint(Plain) then
  begin
    SuppressibleMsgBox(TextEnZh(
      Plain + ' is a link, so it was not deleted.',
      Plain + ' 是链接，因此没有删除。'), mbError, MB_OK, IDOK);
    Exit;
  end;
  if not DelTree(Plain, True, True, True) then
    SuppressibleMsgBox(TextEnZh(
      'Could not delete ' + Plain + '.',
      '无法删除 ' + Plain + '。'), mbError, MB_OK, IDOK);
end;

{ A silent uninstall must not wait on a prompt. It keeps the user data, the same as the default button. }
function DeleteUserDataPrompt(const Roaming, Local: String): Boolean;
begin
  if UninstallSilent then
  begin
    Result := False;
    Exit;
  end;
  Result := SuppressibleMsgBox(TextEnZh(
    'Delete TunePeerly user data?'#13#10#13#10 +
    Roaming + #13#10 +
    Local + #13#10#13#10 +
    'Yes removes the database, covers, and the window cache in these folders. Music files are not in these folders.'#13#10#13#10 +
    'No is selected.',
    '是否删除 TunePeerly 的用户数据？'#13#10#13#10 +
    Roaming + #13#10 +
    Local + #13#10#13#10 +
    '选“是”会删除这两个文件夹里的数据库、封面和界面缓存。音乐文件不在这两个文件夹里。'#13#10#13#10 +
    '当前选中的是“否”。'), mbConfirmation, MB_YESNO or MB_DEFBUTTON2, IDNO) = IDYES;
end;

function InstallDirIsProtected(const Path: String): Boolean;
var
  Plain: String;
begin
  Plain := RemoveBackslashUnlessRoot(Path);
  Result :=
    (Length(Plain) <= 3) or
    (CompareText(Plain, RemoveBackslashUnlessRoot(ExpandConstant('{win}'))) = 0) or
    (CompareText(Plain, RemoveBackslashUnlessRoot(ExpandConstant('{sys}'))) = 0) or
    (CompareText(Plain, RemoveBackslashUnlessRoot(ExpandConstant('{commonpf32}'))) = 0) or
    (CompareText(Plain, RemoveBackslashUnlessRoot(ExpandConstant('{commonpf64}'))) = 0) or
    (CompareText(Plain, RemoveBackslashUnlessRoot(ExpandConstant('{localappdata}'))) = 0) or
    (CompareText(Plain, RemoveBackslashUnlessRoot(ExpandConstant('{userappdata}'))) = 0) or
    (CompareText(Plain, RemoveBackslashUnlessRoot(ExpandConstant('{localappdata}\Programs'))) = 0);
end;

procedure RemoveInstallDir;
var
  AppDir: String;
begin
  AppDir := RemoveBackslashUnlessRoot(ExpandConstant('{app}'));
  if not DirExists(AppDir) then
    Exit;
  if InstallDirIsProtected(AppDir) then
  begin
    Log('Refusing to remove install directory ' + AppDir);
    Exit;
  end;
  if RemoveDir(AppDir) then
    Log('Removed install directory ' + AppDir)
  else
    Log('Install directory was not removed: ' + AppDir);
end;

{ Silently, the rules are removed only when that needs no administrator prompt. }
function DeleteFirewallPrompt: Boolean;
begin
  if UninstallSilent then
  begin
    Result := IsAdminInstallMode;
    Exit;
  end;
  Result := SuppressibleMsgBox(TextEnZh(
    'Delete the firewall rules for this copy of TunePeerly?'#13#10#13#10 +
    'Windows created them when it asked whether to allow the program. Deleting them needs an administrator account.'#13#10#13#10 +
    'Yes is selected.',
    '是否删除这一份 TunePeerly 的防火墙规则？'#13#10#13#10 +
    '这些规则是 Windows 询问是否允许程序时创建的。删除需要管理员权限。'#13#10#13#10 +
    '当前选中的是「是」。'), mbConfirmation, MB_YESNO, IDYES) = IDYES;
end;

procedure RemoveFirewallRules;
var
  ExePath, Netsh, Params: String;
  Code: Integer;
  Started: Boolean;
begin
  ExePath := ExpandConstant('{app}\{#MyAppExeName}');
  Netsh := ExpandConstant('{sys}\netsh.exe');
  Params := 'advfirewall firewall delete rule name=all program="' + ExePath + '"';
  Log('Removing firewall rules for ' + ExePath);
  if IsAdminInstallMode then
    Started := Exec(Netsh, Params, '', SW_HIDE, ewWaitUntilTerminated, Code)
  else
    Started := ShellExec('runas', Netsh, Params, '', SW_HIDE, ewWaitUntilTerminated, Code);
  if not Started then
  begin
    SuppressibleMsgBox(TextEnZh(
      'The firewall rules for this copy of TunePeerly were not removed.',
      '这一份 TunePeerly 的防火墙规则没有删除。'), mbError, MB_OK, IDOK);
    Exit;
  end;
  Log('netsh exited with ' + IntToStr(Code));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Roaming, Local: String;
begin
  if CurUninstallStep <> usPostUninstall then
    Exit;
  RemoveInstallDir;
  Roaming := ExpandConstant('{userappdata}\TunePeerly');
  Local := ExpandConstant('{localappdata}\TunePeerly');
  if (DirExists(Roaming) or DirExists(Local)) and DeleteUserDataPrompt(Roaming, Local) then
  begin
    DeleteOneDataDir(Roaming);
    DeleteOneDataDir(Local);
  end;
  if DeleteFirewallPrompt then
    RemoveFirewallRules;
end;

procedure DeinitializeUninstall;
begin
  RemoveInstallDir;
end;
