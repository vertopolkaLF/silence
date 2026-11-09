// Elevated startup and the single-instance handover.
//
// Low-level hooks (WH_KEYBOARD_LL / WH_MOUSE_LL) are not called while a
// higher-integrity window is focused, unless this process is elevated too.
// "Run as administrator" does two jobs:
//
// - this session: relaunch the current exe with the `runas` verb, or drop back
//   to medium integrity through Explorer's IShellDispatch2;
// - next sign-in: a Task Scheduler logon task with HighestAvailable instead of
//   the HKCU Run key. The task is registered once, so later sign-ins do not
//   show UAC.
//
// ITaskService is the in-process API for the same task, and it is the worse
// fit here. RegisterTaskDefinition with TASK_RUNLEVEL_HIGHEST returns access
// denied from a medium-integrity process, and COM does not raise a UAC prompt
// by itself. Doing it in-process would still need an elevated helper.
// `schtasks /Create /XML` launched with ShellExecuteExW and the runas verb is
// that helper: one consent dialog, a process handle, and ERROR_CANCELLED when
// the user dismisses it. An already-elevated process skips runas and runs
// schtasks directly. The XML (not the schtasks flags) is what turns off the
// battery, idle, and 72-hour execution limits.

use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::sync::OnceLock;

use windows::core::{BSTR, VARIANT};
use windows::Win32::System::Com::{IDispatch, IServiceProvider};
use windows::Win32::{
        Foundation::{ERROR_CANCELLED, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT},
    Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
    System::{
        SystemInformation::GetSystemDirectoryW,
        Threading::{
            GetCurrentProcess, GetExitCodeProcess, OpenProcessToken, WaitForSingleObject,
        },
    },
    UI::{
        Shell::{
            IShellBrowser, IShellDispatch2, IShellFolderViewDual, IShellView, IShellWindows,
            SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, SID_STopLevelBrowser, SWC_DESKTOP,
            SWFO_NEEDDISPATCH, SVGIO_BACKGROUND, ShellExecuteExW, ShellWindows,
        },
        WindowsAndMessaging::{
            ChangeWindowMessageFilterEx, MB_ICONWARNING, MB_OK, MSGFLT_ALLOW, MessageBoxW, SW_HIDE,
            SW_SHOWNORMAL,
        },
    },
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

trait NoWindowCommand {
    fn creation_flags_no_window(&mut self) -> &mut Self;
}

impl NoWindowCommand for Command {
    fn creation_flags_no_window(&mut self) -> &mut Self {
        std::os::windows::process::CommandExt::creation_flags(self, CREATE_NO_WINDOW)
    }
}
const SCHTASKS_WAIT_MS: u32 = 60_000;
const REPLACE_WAIT_MS: u32 = 15_000;

#[derive(Debug)]
struct StartupCancelled;

impl std::fmt::Display for StartupCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Windows permission was cancelled")
    }
}

impl std::error::Error for StartupCancelled {}

pub(crate) fn startup_change_was_cancelled(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|cause| cause.downcast_ref::<StartupCancelled>().is_some())
}

/// Messages other processes post to the hidden tray window.
///
/// UIPI drops these when the sender is medium integrity and the tray is
/// elevated: a `silence://` toast, a non-elevated second instance, settings
/// saving `config.json`, and Explorer's `TaskbarCreated` broadcast.
pub(crate) fn allow_cross_process_window_messages(hwnd: HWND) {
    const MESSAGES: &[u32] = &[
        WM_TOGGLE_MUTE,
        WM_MUTE,
        WM_UNMUTE,
        WM_OPEN_SETTINGS,
        WM_UPDATE_NOW,
        WM_WHATS_NEW,
        WM_EXIT_ALL,
        WM_CONFIG_CHANGED,
        WM_VOLUME_OSD_PREVIEW,
        // Settings sends this with SendMessage while positioning the overlay.
        WM_OVERLAY_POSITIONING,
    ];
    for message in MESSAGES {
        allow_window_message(hwnd, *message);
    }
    allow_window_message(hwnd, *TASKBAR_CREATED_MESSAGE);
}

fn allow_window_message(hwnd: HWND, message: u32) {
    unsafe {
        let _ = ChangeWindowMessageFilterEx(hwnd, message, MSGFLT_ALLOW, None);
    }
}

/// True when the running tray is elevated.
///
/// Settings is started by the tray and normally inherits its token, so this
/// process's own token is the fallback. Prefer the hidden `SilenceV2Hidden`
/// window's process: that is the tray even if settings was opened on its own.
pub(crate) fn running_as_administrator() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| {
        tray_process_is_elevated().unwrap_or_else(current_process_is_elevated)
    })
}

pub(crate) fn current_process_is_elevated() -> bool {
    token_is_elevated(unsafe { GetCurrentProcess() })
}

fn tray_process_is_elevated() -> Option<bool> {
    let hwnd = hidden_window()?;
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    if pid == 0 || pid == std::process::id() {
        return None;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let elevated = token_is_elevated(process);
    unsafe {
        let _ = CloseHandle(process);
    }
    Some(elevated)
}

fn token_is_elevated(process: HANDLE) -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut c_void),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}

/// `true` when applying this pair would show a UAC prompt.
pub(crate) fn startup_sync_needs_consent(launch: bool, admin: bool) -> bool {
    if current_process_is_elevated() {
        return false;
    }
    (launch && admin) || elevated_task_exists()
}

/// Make the Run key and the elevated logon task match the two settings.
///
/// Cancelling UAC leaves both registrations untouched and returns
/// [`StartupCancelled`].
pub(crate) fn sync_startup_registration(launch: bool, admin: bool) -> Result<()> {
    let want_task = launch && admin;
    if want_task {
        create_elevated_task()?;
        remove_run_key()?;
        return Ok(());
    }
    remove_elevated_task()?;
    if launch {
        write_run_key()?;
    } else {
        remove_run_key()?;
    }
    Ok(())
}

/// Tray startup. Never prompts. An elevated tray repairs the task itself;
/// a normal tray only fixes the Run key the installer rewrites on upgrade.
pub(crate) fn ensure_startup_registration() {
    let Ok(config) = load_config() else {
        return;
    };
    let launch = config.startup.launch_on_startup;
    let admin = config.startup.run_as_administrator;
    let result = if current_process_is_elevated() {
        sync_startup_registration(launch, admin)
    } else {
        reconcile_startup_without_elevation(launch, admin)
    };
    if let Err(err) = result {
        eprintln!("startup registration reconcile failed: {err:?}");
    }
}

fn reconcile_startup_without_elevation(launch: bool, admin: bool) -> Result<()> {
    if launch && admin {
        if elevated_task_exists() {
            remove_run_key()?;
        }
        return Ok(());
    }
    if launch {
        write_run_key()?;
    } else {
        remove_run_key()?;
    }
    Ok(())
}

/// Replace the running instance with an elevated one. The new process reads
/// `run_as_administrator` and registers the logon task without a second prompt.
pub(crate) fn relaunch_elevated() -> Result<()> {
    let exe = std::env::current_exe().context("locate current executable")?;
    shell_execute(&exe, ARG_REPLACE, Some("runas"), false, SW_SHOWNORMAL.0)?;
    Ok(())
}

/// Replace the running elevated instance with a medium-integrity one.
///
/// `CreateProcess` from an elevated process stays elevated. Explorer's desktop
/// is medium integrity, and its `IShellDispatch2::ShellExecute` runs there, so
/// the new process is the normal user token. No UAC prompt.
pub(crate) fn relaunch_unelevated() -> Result<()> {
    let exe = std::env::current_exe().context("locate current executable")?;
    launch_via_shell_dispatch(&exe, ARG_REPLACE)
}

/// Own `SilenceV2BackgroundApp`, or signal the instance that already owns it.
///
/// `Ok(None)` means this process asked the running tray to open settings (or
/// dispatched a protocol action) and should exit. `--replace` posts
/// `WM_EXIT_ALL` and waits until that tray releases the mutex.
pub(crate) fn acquire_background_mutex() -> Result<Option<HANDLE>> {
    let replace = std::env::args().any(|arg| arg == ARG_REPLACE);
    let mutex = unsafe { CreateMutexW(None, true, MAIN_INSTANCE_MUTEX)? };
    let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if !already_running {
        return Ok(Some(mutex));
    }
    if !replace {
        if let Some(action) = PENDING_NOTIFICATION_ACTION.lock().unwrap().take() {
            dispatch_notification_action(action);
        } else {
            dispatch_notification_action(NotificationAction::OpenSettings);
        }
        unsafe {
            let _ = CloseHandle(mutex);
        }
        return Ok(None);
    }

    // Post, don't send. The process that launched us may be blocked inside
    // ShellExecuteExW on the settings thread; SendMessage(WM_CLOSE) back to
    // that thread from the old tray would deadlock the handover.
    let _ = post_exit_to_running_instance();
    let wait = unsafe { WaitForSingleObject(mutex, REPLACE_WAIT_MS) };
    if wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED {
        return Ok(Some(mutex));
    }
    unsafe {
        let _ = CloseHandle(mutex);
    }
    show_message(
        "silence! could not replace the running instance. Quit it from the tray and start it again.",
    );
    std::process::exit(1);
}

fn post_exit_to_running_instance() -> bool {
    let Some(hwnd) = hidden_window() else {
        return false;
    };
    unsafe { PostMessageW(hwnd, WM_EXIT_ALL, WPARAM(0), LPARAM(0)).is_ok() }
}

fn create_elevated_task() -> Result<()> {
    let xml = write_task_xml()?;
    let args = [
        "/Create".to_string(),
        "/F".to_string(),
        "/TN".to_string(),
        ELEVATED_TASK_NAME.to_string(),
        "/XML".to_string(),
        xml.0.display().to_string(),
    ];
    let result = run_schtasks(&args);
    drop(xml);
    result
}

fn remove_elevated_task() -> Result<()> {
    if !elevated_task_exists() {
        return Ok(());
    }
    run_schtasks(&[
        "/Delete".to_string(),
        "/F".to_string(),
        "/TN".to_string(),
        ELEVATED_TASK_NAME.to_string(),
    ])
}

fn elevated_task_exists() -> bool {
    Command::new(schtasks_exe())
        .args(["/Query", "/TN", ELEVATED_TASK_NAME])
        .creation_flags_no_window()
        .output()
        .is_ok_and(|output| output.status.success())
}

fn run_schtasks(args: &[String]) -> Result<()> {
    if current_process_is_elevated() {
        run_schtasks_direct(args)
    } else {
        run_schtasks_elevated(args)
    }
}

fn run_schtasks_direct(args: &[String]) -> Result<()> {
    let output = Command::new(schtasks_exe())
        .args(args)
        .creation_flags_no_window()
        .output()
        .context("run schtasks")?;
    if output.status.success() {
        return Ok(());
    }
    let detail = command_output_text(&output.stderr, &output.stdout);
    anyhow::bail!("schtasks failed ({}): {detail}", output.status)
}

fn run_schtasks_elevated(args: &[String]) -> Result<()> {
    let params = join_args(args);
    let code = shell_execute(
        &schtasks_exe(),
        &params,
        Some("runas"),
        true,
        SW_HIDE.0,
    )?;
    if code == 0 {
        return Ok(());
    }
    anyhow::bail!("schtasks exited with {code}")
}

struct TaskXmlFile(PathBuf);

impl Drop for TaskXmlFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn write_task_xml() -> Result<TaskXmlFile> {
    let user = current_user_sam().context("current user for the elevated startup task")?;
    let exe = std::env::current_exe().context("locate current executable for the elevated task")?;
    let xml = task_xml(&user, &exe.to_string_lossy());
    let path = std::env::temp_dir().join(format!(
        "silence-elevated-task-{}.xml",
        std::process::id()
    ));
    let mut bytes = vec![0xFF, 0xFE];
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(&path, bytes).context("write elevated startup task xml")?;
    Ok(TaskXmlFile(path))
}

fn task_xml(user: &str, exe: &str) -> String {
    let user = xml_escape(user);
    let exe = xml_escape(exe);
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Start silence! at sign-in with administrator rights so hotkeys work while an administrator app is focused.</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>4</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
    </Exec>
  </Actions>
</Task>
"#
    )
}

fn current_user_sam() -> Option<String> {
    let user = std::env::var("USERNAME").ok()?;
    let user = user.trim();
    if user.is_empty() {
        return None;
    }
    match std::env::var("USERDOMAIN") {
        Ok(domain) if !domain.trim().is_empty() => Some(format!("{}\\{user}", domain.trim())),
        _ => Some(user.to_string()),
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn write_run_key() -> Result<()> {
    let command = format!(
        "\"{}\"",
        std::env::current_exe()
            .context("locate current executable for startup registration")?
            .display()
    );
    let subkey = wide(STARTUP_RUN_SUBKEY);
    let value_name = wide(STARTUP_RUN_VALUE);
    let command_wide = wide(&command);
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value_name.as_ptr()),
            REG_SZ.0,
            Some(command_wide.as_ptr() as *const c_void),
            (command_wide.len() * size_of::<u16>()) as u32,
        )
    };
    anyhow::ensure!(
        status == ERROR_SUCCESS,
        "startup registration failed with status {status:?}"
    );
    Ok(())
}

fn remove_run_key() -> Result<()> {
    let subkey = wide(STARTUP_RUN_SUBKEY);
    let value_name = wide(STARTUP_RUN_VALUE);
    let status = unsafe {
        RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value_name.as_ptr()),
        )
    };
    anyhow::ensure!(
        status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND,
        "startup registration removal failed with status {status:?}"
    );
    Ok(())
}

fn launch_via_shell_dispatch(exe: &Path, args: &str) -> Result<()> {
    unsafe {
        let shell_windows: IShellWindows =
            CoCreateInstance(&ShellWindows, None, CLSCTX_ALL).context("create shell windows")?;
        let loc = VARIANT::new();
        let mut hwnd = 0i32;
        let dispatch = shell_windows
            .FindWindowSW(
                &loc,
                &loc,
                SWC_DESKTOP,
                &mut hwnd,
                SWFO_NEEDDISPATCH,
            )
            .context("find the desktop shell window")?;
        let provider: IServiceProvider = dispatch
            .cast()
            .context("desktop shell service provider")?;
        let browser: IShellBrowser = provider
            .QueryService(&SID_STopLevelBrowser)
            .context("desktop browser")?;
        let view: IShellView = browser
            .QueryActiveShellView()
            .context("desktop shell view")?;
        let background: IDispatch = view
            .GetItemObject(SVGIO_BACKGROUND)
            .context("desktop background")?;
        let folder: IShellFolderViewDual = background.cast().context("desktop folder view")?;
        let application = folder.Application().context("shell application")?;
        let shell: IShellDispatch2 = application.cast().context("shell dispatch")?;
        let file = path_bstr(exe);
        let vargs = VARIANT::from(args);
        let vdir = VARIANT::new();
        let operation = VARIANT::from("open");
        let show = VARIANT::from(SW_SHOWNORMAL.0);
        shell
            .ShellExecute(&file, &vargs, &vdir, &operation, &show)
            .context("start silence! without administrator rights")?;
    }
    Ok(())
}

fn path_bstr(path: &Path) -> BSTR {
    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    BSTR::from_wide(&wide).unwrap_or_else(|_| BSTR::new())
}

fn shell_execute(
    file: &Path,
    params: &str,
    verb: Option<&str>,
    wait: bool,
    show: i32,
) -> Result<u32> {
    let file_wide = wide_path(file);
    let params_wide = wide(params);
    let verb_wide = verb.map(wide);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: if wait { SEE_MASK_NOCLOSEPROCESS } else { 0 },
        lpVerb: verb_wide
            .as_ref()
            .map(|verb| PCWSTR(verb.as_ptr()))
            .unwrap_or_else(PCWSTR::null),
        lpFile: PCWSTR(file_wide.as_ptr()),
        lpParameters: PCWSTR(params_wide.as_ptr()),
        nShow: show,
        ..Default::default()
    };
    unsafe {
        if let Err(err) = ShellExecuteExW(&mut info) {
            if err.code() == HRESULT::from_win32(ERROR_CANCELLED.0) {
                return Err(StartupCancelled.into());
            }
            return Err(err).context("ShellExecuteExW");
        }
        if !wait {
            return Ok(0);
        }
        if info.hProcess.is_invalid() {
            anyhow::bail!("schtasks did not return a process handle");
        }
        let waited = WaitForSingleObject(info.hProcess, SCHTASKS_WAIT_MS);
        let mut code = 1u32;
        let read = if waited == WAIT_OBJECT_0 {
            GetExitCodeProcess(info.hProcess, &mut code).context("read schtasks exit code")
        } else {
            Ok::<(), anyhow::Error>(())
        };
        let _ = CloseHandle(info.hProcess);
        read?;
        if waited == WAIT_TIMEOUT {
            anyhow::bail!("schtasks timed out");
        }
        if waited != WAIT_OBJECT_0 {
            anyhow::bail!("waiting for schtasks failed ({waited:?})");
        }
        Ok(code)
    }
}

fn schtasks_exe() -> PathBuf {
    system_exe("schtasks.exe")
}

fn system_exe(name: &str) -> PathBuf {
    let mut buffer = [0u16; 260];
    let len = unsafe { GetSystemDirectoryW(Some(&mut buffer[..])) } as usize;
    if len == 0 || len >= buffer.len() {
        return PathBuf::from(r"C:\Windows\System32").join(name);
    }
    PathBuf::from(std::ffi::OsString::from_wide(&buffer[..len])).join(name)
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if arg.is_empty() || arg.chars().any(|ch| ch.is_whitespace()) {
                format!("\"{arg}\"")
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn command_output_text(stderr: &[u8], stdout: &[u8]) -> String {
    let text = decode_schtasks_output(stderr);
    if text.trim().is_empty() {
        decode_schtasks_output(stdout)
    } else {
        text
    }
}

fn decode_schtasks_output(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes).trim().to_string()
}

fn show_message(text: &str) {
    let text = wide(text);
    let title = wide("silence!");
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONWARNING,
        );
    }
}
