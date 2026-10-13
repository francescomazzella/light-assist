//! How the app is installed, and what follows from it.
//!
//! There are three ways Light Assist can be running:
//!
//! - **From the Microsoft Store**, as an MSIX package. Windows installs, updates and removes it, and
//!   starting with Windows is the package's own startup task.
//! - **Installed by itself**: the downloadable exe copies itself to `%LOCALAPPDATA%\Programs\Light Assist`,
//!   with a Start menu entry and an entry in Settings → Apps. No administrator rights are needed.
//! - **Portable**: the downloaded exe, run from wherever it is.
//!
//! Outside the Store, starting with Windows is a value in the user's own Run key.
//!
//! On a Mac the app is a bundle the user drags to Applications and to the Trash, so there is nothing to
//! install or remove here. Opening at login is a launch agent in the user's own Library.

use std::path::PathBuf;

use serde::Serialize;

/// Given to the app when Windows starts it at login, so that it waits in the tray.
pub const TRAY_ARGUMENT: &str = "--tray";
/// Given by Settings → Apps to remove the self-installed copy.
pub const UNINSTALL_ARGUMENT: &str = "--uninstall";
/// Given to the freshly installed copy, which has to wait for the portable one to let go of its port.
pub const AFTER_INSTALL_ARGUMENT: &str = "--after-install";

#[cfg(windows)]
const STARTUP_TASK: &str = "LightAssistStartup";
const EXE_NAME: &str = "light-assist.exe";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Installation {
    Store,
    Installed,
    Portable,
}

/// What the settings page shows about this computer.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub installation: Installation,
    pub start_with_windows: bool,
    /// Why start-with-Windows cannot be changed from here, when it cannot.
    pub startup_note: Option<String>,
}

/// Where the self-installed copy lives.
fn install_folder() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Programs").join("Light Assist"))
}

pub fn installation() -> Installation {
    if os::packaged() {
        return Installation::Store;
    }
    if cfg!(target_os = "macos") {
        return if os::in_applications() { Installation::Installed } else { Installation::Portable };
    }
    let here = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|folder| folder.to_path_buf()));
    match (here, install_folder()) {
        (Some(here), Some(folder)) if here.eq_ignore_ascii_case_path(&folder) => Installation::Installed,
        _ => Installation::Portable,
    }
}

trait SamePath {
    fn eq_ignore_ascii_case_path(&self, other: &PathBuf) -> bool;
}

impl SamePath for PathBuf {
    /// Windows paths do not care about case.
    fn eq_ignore_ascii_case_path(&self, other: &PathBuf) -> bool {
        self.to_string_lossy().trim_end_matches('\\').eq_ignore_ascii_case(other.to_string_lossy().trim_end_matches('\\'))
    }
}

pub fn info() -> AppInfo {
    let installation = installation();
    let (start_with_windows, startup_note) = os::startup(installation);
    AppInfo { installation, start_with_windows, startup_note }
}

pub fn set_start_with_windows(on: bool) -> Result<AppInfo, String> {
    os::set_startup(installation(), on)?;
    Ok(info())
}

/// Whether Windows started the app at login, in which case it waits in the tray.
pub fn started_at_login() -> bool {
    std::env::args().any(|argument| argument == TRAY_ARGUMENT) || os::started_by_startup_task()
}

/// Copies this portable exe into place and registers it, then starts the installed copy. The caller
/// quits this one straight after, so that the new copy can take over.
pub fn install() -> Result<(), String> {
    if cfg!(target_os = "macos") {
        return Err(crate::lang::tr("On a Mac, drag Light Assist to the Applications folder.", "Su un Mac, trascina Light Assist nella cartella Applicazioni.", "En un Mac, arrastra Light Assist a la carpeta Aplicaciones.").to_owned());
    }
    if installation() != Installation::Portable {
        return Err(crate::lang::tr("Light Assist is already installed.", "Light Assist è già installato.", "Light Assist ya está instalado.").to_owned());
    }
    let folder = install_folder().ok_or(crate::lang::tr("The user's program folder could not be found.", "Non è stato possibile trovare la cartella dei programmi dell'utente.", "No se ha encontrado la carpeta de programas del usuario."))?;
    let installed = folder.join(EXE_NAME);
    let current = std::env::current_exe().map_err(|error| error.to_string())?;

    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    std::fs::copy(&current, &installed).map_err(|error| format!("Light Assist could not be copied: {error}"))?;
    os::register(&installed, &folder)?;
    // Starting with Windows follows the app to where it now lives.
    if os::run_key_set() {
        os::set_run_key(Some(&installed))?;
    }

    std::process::Command::new(&installed).arg(AFTER_INSTALL_ARGUMENT).spawn().map_err(|error| error.to_string())?;
    Ok(())
}

/// Removes the self-installed copy, as Settings → Apps asks. Leaves nothing behind: the program
/// folder, its Start menu and Settings entries, starting with Windows, and the app's own data.
pub fn uninstall() {
    let Some(folder) = install_folder() else { return };
    os::stop_others_in(&folder);
    os::unregister();
    let _ = os::set_run_key(None);
    if let Some(data) = std::env::var_os("LOCALAPPDATA").map(|local| PathBuf::from(local).join("light-assist")) {
        let _ = std::fs::remove_dir_all(data);
    }
    // The running exe cannot delete itself; a short-lived command removes the folder once it has exited.
    os::delete_after_exit(&folder);
}

#[cfg(windows)]
mod os {
    use std::{os::windows::process::CommandExt, path::Path};

    use windows::{
        core::{w, Interface, HSTRING, PCWSTR, PWSTR},
        ApplicationModel::{Activation::ActivationKind, AppInstance, StartupTask, StartupTaskState},
        Win32::{
            Foundation::{CloseHandle, APPMODEL_ERROR_NO_PACKAGE, ERROR_FILE_NOT_FOUND},
            Storage::Packaging::Appx::GetCurrentPackageFullName,
            System::{
                Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER},
                Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS},
                Registry::{
                    RegDeleteKeyValueW, RegDeleteTreeW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD, REG_SZ,
                    RRF_RT_REG_SZ,
                },
                Threading::{
                    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, TerminateProcess, PROCESS_NAME_WIN32,
                    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
                },
            },
            UI::Shell::{IShellLinkW, ShellLink},
        },
    };

    use super::{Installation, STARTUP_TASK, TRAY_ARGUMENT, UNINSTALL_ARGUMENT};

    const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
    const RUN_VALUE: PCWSTR = w!("Light Assist");
    const UNINSTALL_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\LightAssist");
    /// Stops a helper process from flashing a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn in_applications() -> bool {
        false
    }

    pub fn packaged() -> bool {
        let mut length = 0;
        unsafe { GetCurrentPackageFullName(&mut length, None) != APPMODEL_ERROR_NO_PACKAGE }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    fn set_text(key: PCWSTR, name: PCWSTR, value: &str) -> Result<(), String> {
        let value = wide(value);
        unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, key, name, REG_SZ.0, Some(value.as_ptr().cast()), (value.len() * 2) as u32)
                .ok()
                .map_err(|error| error.message())
        }
    }

    fn set_number(key: PCWSTR, name: PCWSTR, value: u32) -> Result<(), String> {
        unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, key, name, REG_DWORD.0, Some((&value as *const u32).cast()), 4)
                .ok()
                .map_err(|error| error.message())
        }
    }

    pub fn run_key_set() -> bool {
        unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE, RRF_RT_REG_SZ, None, None, None).is_ok() }
    }

    /// Points the Run key at `exe`, or removes it.
    pub fn set_run_key(exe: Option<&Path>) -> Result<(), String> {
        match exe {
            Some(exe) => set_text(RUN_KEY, RUN_VALUE, &format!("\"{}\" {TRAY_ARGUMENT}", exe.display())),
            None => {
                let result = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE) };
                // Already absent is as good as removed.
                if result.is_ok() || result == ERROR_FILE_NOT_FOUND {
                    Ok(())
                } else {
                    Err(result.to_hresult().message())
                }
            }
        }
    }

    fn startup_task() -> windows::core::Result<StartupTask> {
        StartupTask::GetAsync(&HSTRING::from(STARTUP_TASK))?.join()
    }

    /// Whether the app starts with Windows, and why that cannot be changed here, if it cannot.
    pub fn startup(installation: Installation) -> (bool, Option<String>) {
        if installation != Installation::Store {
            return (run_key_set(), None);
        }
        match startup_task().and_then(|task| task.State()) {
            Ok(StartupTaskState::Enabled) => (true, None),
            Ok(StartupTaskState::EnabledByPolicy) => (true, Some(crate::lang::tr("Set by your organization.", "Impostato dalla tua organizzazione.", "Configurado por tu organización.").to_owned())),
            Ok(StartupTaskState::DisabledByPolicy) => (false, Some(crate::lang::tr("Turned off by your organization.", "Disattivato dalla tua organizzazione.", "Desactivado por tu organización.").to_owned())),
            Ok(StartupTaskState::DisabledByUser) => (
                false,
                Some(crate::lang::tr("Turned off in Task Manager's Startup apps. Turn it back on there.", "Disattivato in Gestione attività, App di avvio. Riattivalo da lì.", "Desactivado en Administrador de tareas, Aplicaciones de arranque. Vuelve a activarlo allí.").to_owned()),
            ),
            Ok(_) => (false, None),
            Err(error) => (false, Some(format!("Not available: {}", error.message()))),
        }
    }

    pub fn set_startup(installation: Installation, on: bool) -> Result<(), String> {
        if installation != Installation::Store {
            let exe = std::env::current_exe().map_err(|error| error.to_string())?;
            return set_run_key(on.then_some(exe.as_path()));
        }
        let task = startup_task().map_err(|error| error.message())?;
        if on {
            task.RequestEnableAsync().and_then(|request| request.join()).map(|_| ()).map_err(|error| error.message())
        } else {
            task.Disable().map_err(|error| error.message())
        }
    }

    pub fn started_by_startup_task() -> bool {
        packaged()
            && AppInstance::GetActivatedEventArgs()
                .and_then(|activation| activation.Kind())
                .is_ok_and(|kind| kind == ActivationKind::StartupTask)
    }

    /// A Start menu entry and an entry in Settings → Apps for the installed copy.
    pub fn register(exe: &Path, folder: &Path) -> Result<(), String> {
        let exe_text = exe.display().to_string();
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|error| error.message())?;
            link.SetPath(&HSTRING::from(exe_text.as_str())).map_err(|error| error.message())?;
            link.SetDescription(w!("Request assistance with this computer")).map_err(|error| error.message())?;
            let programs = std::env::var("APPDATA").map_err(|error| error.to_string())? + r"\Microsoft\Windows\Start Menu\Programs\Light Assist.lnk";
            link.cast::<IPersistFile>()
                .and_then(|file| file.Save(&HSTRING::from(programs), true))
                .map_err(|error| error.message())?;
        }

        let size_kb = std::fs::metadata(exe).map(|metadata| (metadata.len() / 1024) as u32).unwrap_or(0);
        set_text(UNINSTALL_KEY, w!("DisplayName"), "Light Assist")?;
        set_text(UNINSTALL_KEY, w!("DisplayIcon"), &exe_text)?;
        set_text(UNINSTALL_KEY, w!("DisplayVersion"), env!("CARGO_PKG_VERSION"))?;
        set_text(UNINSTALL_KEY, w!("Publisher"), "Francesco Mazzella")?;
        set_text(UNINSTALL_KEY, w!("InstallLocation"), &folder.display().to_string())?;
        set_text(UNINSTALL_KEY, w!("UninstallString"), &format!("\"{exe_text}\" {UNINSTALL_ARGUMENT}"))?;
        set_number(UNINSTALL_KEY, w!("EstimatedSize"), size_kb)?;
        set_number(UNINSTALL_KEY, w!("NoModify"), 1)?;
        set_number(UNINSTALL_KEY, w!("NoRepair"), 1)
    }

    pub fn unregister() {
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, UNINSTALL_KEY);
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, w!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall"), w!("LightAssist"));
        }
        if let Ok(roaming) = std::env::var("APPDATA") {
            let _ = std::fs::remove_file(roaming + r"\Microsoft\Windows\Start Menu\Programs\Light Assist.lnk");
        }
    }

    /// Ends any other Light Assist running from `folder`, which would keep its files in use.
    pub fn stop_others_in(folder: &Path) {
        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return };
            let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
            let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
            while more {
                if entry.th32ProcessID != GetCurrentProcessId() {
                    if let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE, false, entry.th32ProcessID) {
                        let mut path = [0u16; 1024];
                        let mut length = path.len() as u32;
                        if QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(path.as_mut_ptr()), &mut length).is_ok() {
                            let path = String::from_utf16_lossy(&path[..length as usize]).to_lowercase();
                            if path.starts_with(&folder.display().to_string().to_lowercase()) {
                                let _ = TerminateProcess(process, 0);
                            }
                        }
                        let _ = CloseHandle(process);
                    }
                }
                more = Process32NextW(snapshot, &mut entry).is_ok();
            }
            let _ = CloseHandle(snapshot);
        }
    }

    pub fn delete_after_exit(folder: &Path) {
        let command = format!("ping -n 3 127.0.0.1 > nul & rmdir /s /q \"{}\"", folder.display());
        let _ = std::process::Command::new("cmd")
            // Verbatim: cmd does not understand the escaping Rust would give the quotes around the path.
            .arg("/c")
            .raw_arg(&command)
            .current_dir(std::env::temp_dir())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}

#[cfg(target_os = "macos")]
mod os {
    use std::path::{Path, PathBuf};

    use super::{Installation, TRAY_ARGUMENT};

    /// The launch agent's name, and its file's.
    const AGENT: &str = "com.light-assist.LightAssist";

    fn home() -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }

    fn launch_agent() -> Option<PathBuf> {
        Some(home()?.join("Library").join("LaunchAgents").join(format!("{AGENT}.plist")))
    }

    /// The app bundle this is running from, if it is in one: `Light Assist.app/Contents/MacOS/light-assist`.
    fn bundle() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let bundle = exe.parent()?.parent()?.parent()?;
        (exe.parent()?.ends_with("Contents/MacOS") && bundle.extension().is_some_and(|extension| extension == "app")).then(|| bundle.to_path_buf())
    }

    /// Whether the app is where a Mac keeps its apps: the Applications folder, or the user's own.
    pub fn in_applications() -> bool {
        let folder = bundle().and_then(|bundle| bundle.parent().map(Path::to_path_buf));
        folder.is_some_and(|folder| folder == Path::new("/Applications") || home().is_some_and(|home| folder == home.join("Applications")))
    }

    /// What starts the app at login, as a launch agent's arguments: through Launch Services when
    /// it is a bundle, so that it starts as it does from the Dock, without coming to the front.
    fn arguments() -> Option<Vec<String>> {
        let text = |path: PathBuf| path.to_str().map(str::to_owned);
        Some(match bundle() {
            Some(bundle) => vec!["/usr/bin/open".to_owned(), "-g".to_owned(), "-a".to_owned(), text(bundle)?, "--args".to_owned(), TRAY_ARGUMENT.to_owned()],
            None => vec![text(std::env::current_exe().ok()?)?, TRAY_ARGUMENT.to_owned()],
        })
    }

    fn property_list() -> Option<String> {
        let escape = |text: &str| text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let arguments: String = arguments()?.iter().map(|argument| format!("        <string>{}</string>\n", escape(argument))).collect();
        Some(format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{AGENT}</string>
    <key>ProgramArguments</key>
    <array>
{arguments}    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>LimitLoadToSessionType</key>
    <string>Aqua</string>
</dict>
</plist>
"#
        ))
    }

    /// Whether the app opens at login. A launch agent left behind by a copy of the app that has
    /// since been moved or removed does not count: it would start nothing.
    pub fn startup(_installation: Installation) -> (bool, Option<String>) {
        let written = launch_agent().and_then(|path| std::fs::read_to_string(path).ok());
        (written.is_some() && written == property_list(), None)
    }

    pub fn set_startup(_installation: Installation, on: bool) -> Result<(), String> {
        let path = launch_agent().ok_or(crate::lang::tr("Your home folder could not be found.", "Non è stato possibile trovare la tua cartella Inizio.", "No se ha encontrado tu carpeta de inicio."))?;
        if !on {
            return match std::fs::remove_file(&path) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
                _ => Ok(()),
            };
        }
        let contents = property_list().ok_or(crate::lang::tr("Light Assist could not tell where it is running from.", "Light Assist non è riuscito a capire da dove è in esecuzione.", "Light Assist no ha podido saber desde dónde se está ejecutando."))?;
        path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, contents)).map_err(|error| error.to_string())
    }

    pub fn packaged() -> bool {
        false
    }
    pub fn run_key_set() -> bool {
        false
    }
    pub fn set_run_key(_exe: Option<&Path>) -> Result<(), String> {
        Err("not available on this platform".to_owned())
    }
    pub fn started_by_startup_task() -> bool {
        false
    }
    pub fn register(_exe: &Path, _folder: &Path) -> Result<(), String> {
        Err(crate::lang::tr("On a Mac, drag Light Assist to the Applications folder.", "Su un Mac, trascina Light Assist nella cartella Applicazioni.", "En un Mac, arrastra Light Assist a la carpeta Aplicaciones.").to_owned())
    }
    pub fn unregister() {}
    pub fn stop_others_in(_folder: &Path) {}
    pub fn delete_after_exit(_folder: &Path) {}
}

/// No installer and no starting at login anywhere else so far.
#[cfg(not(any(windows, target_os = "macos")))]
mod os {
    use std::path::Path;

    use super::Installation;

    pub fn in_applications() -> bool {
        false
    }
    pub fn packaged() -> bool {
        false
    }
    pub fn run_key_set() -> bool {
        false
    }
    pub fn set_run_key(_exe: Option<&Path>) -> Result<(), String> {
        Err("not available on this platform".to_owned())
    }
    pub fn startup(_installation: Installation) -> (bool, Option<String>) {
        (false, Some("Not available on this platform.".to_owned()))
    }
    pub fn set_startup(_installation: Installation, _on: bool) -> Result<(), String> {
        Err("Not available on this platform.".to_owned())
    }
    pub fn started_by_startup_task() -> bool {
        false
    }
    pub fn register(_exe: &Path, _folder: &Path) -> Result<(), String> {
        Err("Not available on this platform.".to_owned())
    }
    pub fn unregister() {}
    pub fn stop_others_in(_folder: &Path) {}
    pub fn delete_after_exit(_folder: &Path) {}
}
