//! A shell on the sharer's computer for the helper: PowerShell through Windows' pseudo-console
//! (ConPTY), or the sharer's own shell on a pseudo-terminal elsewhere.

use tokio::sync::mpsc;

const MAX_SIZE: u16 = 500;
/// Unsent output waits here; when it is full the shell is simply slowed down.
const MAX_QUEUED_CHUNKS: usize = 64;

pub enum TerminalEvent {
    /// What the shell printed, as the bytes a terminal emulator expects.
    Output(Vec<u8>),
    Exited,
}

/// One running shell. Dropping it ends the shell.
pub struct Terminal {
    events: mpsc::Receiver<TerminalEvent>,
    pty: os::Pty,
}

fn clamp(cells: u16) -> i16 {
    cells.clamp(1, MAX_SIZE) as i16
}

impl Terminal {
    /// Starts the shell in the sharer's home folder, as the sharer.
    pub fn open(cols: u16, rows: u16) -> Result<Self, String> {
        let (tx, events) = mpsc::channel(MAX_QUEUED_CHUNKS);
        let pty = os::Pty::spawn(clamp(cols), clamp(rows), tx)?;
        Ok(Self { events, pty })
    }

    pub async fn next(&mut self) -> TerminalEvent {
        self.events.recv().await.unwrap_or(TerminalEvent::Exited)
    }

    /// What the helper typed.
    pub fn write(&self, data: &[u8]) {
        self.pty.write(data);
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        self.pty.resize(clamp(cols), clamp(rows));
    }
}

#[cfg(windows)]
mod os {
    use std::{ffi::c_void, mem::size_of, os::windows::ffi::OsStrExt};

    use tokio::sync::mpsc;
    use windows::{
        core::{PCWSTR, PWSTR},
        Win32::{
            Foundation::{CloseHandle, HANDLE},
            Storage::FileSystem::{ReadFile, WriteFile},
            System::{
                Console::{ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD, HPCON},
                Pipes::CreatePipe,
                Threading::{
                    CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
                    UpdateProcThreadAttribute, WaitForSingleObject, EXTENDED_STARTUPINFO_PRESENT, INFINITE,
                    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
                    STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW,
                },
            },
        },
    };

    use super::TerminalEvent;

    /// PowerShell 7 if it is installed, otherwise the Windows PowerShell every Windows has.
    const SHELLS: [&str; 2] = ["pwsh.exe", "powershell.exe"];

    fn wide(text: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
        text.as_ref().encode_wide().chain([0]).collect()
    }

    pub struct Pty {
        console: HPCON,
        /// The end of the pipe that the shell reads its keyboard from.
        input: HANDLE,
    }

    // The handles are only ever used through the thread-safe calls below.
    unsafe impl Send for Pty {}

    /// Starts a shell attached to `console` and returns its process handle.
    unsafe fn start_shell(console: HPCON) -> windows::core::Result<HANDLE> {
        let mut size = 0;
        let _ = InitializeProcThreadAttributeList(None, 1, None, &mut size);
        let mut storage = vec![0u8; size];
        let attributes = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr().cast());
        InitializeProcThreadAttributeList(Some(attributes), 1, None, &mut size)?;
        let attached = UpdateProcThreadAttribute(
            attributes,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            Some(console.0 as *const c_void),
            size_of::<HPCON>(),
            None,
            None,
        );

        let startup = STARTUPINFOEXW {
            StartupInfo: STARTUPINFOW {
                cb: size_of::<STARTUPINFOEXW>() as u32,
                // With no standard handles of its own the shell uses the pseudo-console's. Without this
                // flag it would inherit the agent's, and write to the agent's output when that is redirected.
                dwFlags: STARTF_USESTDHANDLES,
                ..Default::default()
            },
            lpAttributeList: attributes,
        };
        let home = std::env::var_os("USERPROFILE").map(wide);
        let directory = home.as_ref().map_or(PCWSTR::null(), |home| PCWSTR(home.as_ptr()));

        let mut started = attached.map(|()| HANDLE::default());
        if started.is_ok() {
            for shell in SHELLS {
                let mut command = wide(shell);
                let mut process = PROCESS_INFORMATION::default();
                started = CreateProcessW(
                    None,
                    Some(PWSTR(command.as_mut_ptr())),
                    None,
                    None,
                    false,
                    EXTENDED_STARTUPINFO_PRESENT,
                    None,
                    directory,
                    &startup.StartupInfo,
                    &mut process,
                )
                .map(|()| {
                    let _ = CloseHandle(process.hThread);
                    process.hProcess
                });
                if started.is_ok() {
                    break;
                }
            }
        }

        DeleteProcThreadAttributeList(attributes);
        started
    }

    impl Pty {
        pub fn spawn(cols: i16, rows: i16, events: mpsc::Sender<TerminalEvent>) -> Result<Self, String> {
            unsafe {
                let (mut input_read, mut input, mut output, mut output_write) = Default::default();
                CreatePipe(&mut input_read, &mut input, None, 0).map_err(|error| error.to_string())?;
                CreatePipe(&mut output, &mut output_write, None, 0).map_err(|error| error.to_string())?;

                let console = CreatePseudoConsole(COORD { X: cols, Y: rows }, input_read, output_write, 0);
                // The pseudo-console keeps its own copies of the two ends it was given.
                let _ = CloseHandle(input_read);
                let _ = CloseHandle(output_write);

                let started = console.and_then(|console| {
                    start_shell(console).map(|process| (console, process)).inspect_err(|_| ClosePseudoConsole(console))
                });
                let (console, process) = match started {
                    Ok(started) => started,
                    Err(error) => {
                        let _ = CloseHandle(input);
                        let _ = CloseHandle(output);
                        return Err(error.to_string());
                    }
                };

                // Handles are plain numbers; they cross to the threads as such.
                let (output, process) = (output.0 as usize, process.0 as usize);
                let printed = events.clone();
                std::thread::spawn(move || {
                    let output = HANDLE(output as *mut c_void);
                    let mut buffer = vec![0u8; 8192];
                    loop {
                        let mut read = 0;
                        // Fails once the pseudo-console is closed, which is how this thread ends.
                        if ReadFile(output, Some(&mut buffer), Some(&mut read), None).is_err() || read == 0 {
                            break;
                        }
                        if printed.blocking_send(TerminalEvent::Output(buffer[..read as usize].to_vec())).is_err() {
                            break;
                        }
                    }
                    let _ = CloseHandle(output);
                });
                std::thread::spawn(move || {
                    let process = HANDLE(process as *mut c_void);
                    WaitForSingleObject(process, INFINITE);
                    let _ = CloseHandle(process);
                    let _ = events.blocking_send(TerminalEvent::Exited);
                });

                Ok(Self { console, input })
            }
        }

        pub fn write(&self, data: &[u8]) {
            unsafe {
                let _ = WriteFile(self.input, Some(data), None, None);
            }
        }

        pub fn resize(&self, cols: i16, rows: i16) {
            unsafe {
                let _ = ResizePseudoConsole(self.console, COORD { X: cols, Y: rows });
            }
        }
    }

    impl Drop for Pty {
        fn drop(&mut self) {
            unsafe {
                // Closing the pseudo-console ends the shell and everything started from it.
                ClosePseudoConsole(self.console);
                let _ = CloseHandle(self.input);
            }
        }
    }
}

#[cfg(unix)]
mod os {
    use std::{
        fs::File,
        io::{Read, Write},
        os::{
            fd::{AsRawFd, FromRawFd, OwnedFd},
            unix::process::CommandExt,
        },
        process::{Command, Stdio},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Duration,
    };

    use tokio::sync::mpsc;

    use super::TerminalEvent;

    /// What every Mac has, for a sharer whose `SHELL` says nothing.
    const DEFAULT_SHELL: &str = if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/sh" };
    /// How long a shell that ignores being hung up on is given before it is ended outright.
    const GRACE: Duration = Duration::from_secs(1);

    pub struct Pty {
        /// The terminal's side of the pseudo-terminal: what is written here, the shell reads as typed.
        terminal: File,
        /// The shell's process, which leads a process group of everything started from it.
        shell: libc::pid_t,
        exited: Arc<AtomicBool>,
    }

    fn size(cols: i16, rows: i16) -> libc::winsize {
        libc::winsize { ws_row: rows as u16, ws_col: cols as u16, ws_xpixel: 0, ws_ypixel: 0 }
    }

    impl Pty {
        pub fn spawn(cols: i16, rows: i16, events: mpsc::Sender<TerminalEvent>) -> Result<Self, String> {
            let failed = || std::io::Error::last_os_error().to_string();
            let (mut terminal, mut shell_side) = (0, 0);
            let size = size(cols, rows);
            if unsafe { libc::openpty(&mut terminal, &mut shell_side, std::ptr::null_mut(), std::ptr::null_mut(), &size as *const _ as *mut _) } != 0 {
                return Err(failed());
            }
            let (terminal, shell_side) = unsafe { (File::from_raw_fd(terminal), OwnedFd::from_raw_fd(shell_side)) };
            // The shell and what it starts must not be handed the terminal's side as well.
            unsafe { libc::fcntl(terminal.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) };

            let shell = std::env::var_os("SHELL").filter(|shell| !shell.is_empty()).unwrap_or_else(|| DEFAULT_SHELL.into());
            let stream = || shell_side.try_clone().map(Stdio::from).map_err(|error| error.to_string());
            let mut command = Command::new(&shell);
            command
                // A login shell, so that it reads the sharer's profile: an app started from the Dock
                // has none of what a terminal window would have set up.
                .arg("-l")
                .env("TERM", "xterm-256color")
                .stdin(stream()?)
                .stdout(stream()?)
                .stderr(stream()?);
            if let Some(home) = std::env::var_os("HOME") {
                command.current_dir(home);
            }
            // The same goes for the language: without one, accented letters come out as question marks.
            if ["LC_ALL", "LC_CTYPE", "LANG"].iter().all(|name| std::env::var_os(name).is_none()) {
                command.env("LANG", "en_US.UTF-8");
            }
            unsafe {
                command.pre_exec(|| {
                    // A session of its own, with the pseudo-terminal as the terminal that controls it,
                    // so that Ctrl+C and a hang-up reach the shell and everything it started.
                    if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let mut child = command.spawn().map_err(|error| format!("{}: {error}", shell.to_string_lossy()))?;
            drop(command);
            drop(shell_side);
            let shell = child.id() as libc::pid_t;

            let mut output = terminal.try_clone().map_err(|error| error.to_string())?;
            let printed = events.clone();
            std::thread::spawn(move || {
                let mut buffer = vec![0u8; 8192];
                // Fails once nothing has the shell's side open any more, which is how this thread ends.
                while let Ok(read @ 1..) = output.read(&mut buffer) {
                    if printed.blocking_send(TerminalEvent::Output(buffer[..read].to_vec())).is_err() {
                        break;
                    }
                }
            });
            let exited = Arc::new(AtomicBool::new(false));
            let gone = exited.clone();
            std::thread::spawn(move || {
                let _ = child.wait();
                gone.store(true, Ordering::Relaxed);
                let _ = events.blocking_send(TerminalEvent::Exited);
            });

            Ok(Self { terminal, shell, exited })
        }

        pub fn write(&self, data: &[u8]) {
            let _ = (&self.terminal).write_all(data);
        }

        pub fn resize(&self, cols: i16, rows: i16) {
            unsafe {
                libc::ioctl(self.terminal.as_raw_fd(), libc::TIOCSWINSZ as _, &size(cols, rows));
            }
        }
    }

    impl Drop for Pty {
        fn drop(&mut self) {
            if self.exited.load(Ordering::Relaxed) {
                return;
            }
            // Hung up on, as when a terminal window is closed: the shell and what it started end.
            let (shell, exited) = (self.shell, self.exited.clone());
            unsafe { libc::kill(-shell, libc::SIGHUP) };
            std::thread::spawn(move || {
                std::thread::sleep(GRACE);
                if !exited.load(Ordering::Relaxed) {
                    unsafe { libc::kill(-shell, libc::SIGKILL) };
                }
            });
        }
    }
}

/// No terminal anywhere else so far.
#[cfg(not(any(windows, unix)))]
mod os {
    use tokio::sync::mpsc;

    use super::TerminalEvent;

    pub struct Pty;

    impl Pty {
        pub fn spawn(_cols: i16, _rows: i16, _events: mpsc::Sender<TerminalEvent>) -> Result<Self, String> {
            Err("the terminal is not implemented on this platform".to_owned())
        }

        pub fn write(&self, _data: &[u8]) {}
        pub fn resize(&self, _cols: i16, _rows: i16) {}
    }
}
