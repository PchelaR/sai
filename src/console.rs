use std::env;
use std::io::{self, IsTerminal, Write};

use anyhow::{Result, ensure};

const RESET_MODES: &str = concat!(
    "\x1b[?1000l",
    "\x1b[?1002l",
    "\x1b[?1003l",
    "\x1b[?1005l",
    "\x1b[?1006l",
    "\x1b[?1015l",
    "\x1b[?1004l",
    "\x1b[?2004l",
    "\x1b[>4;0m",
    "\x1b[<u",
    "\x1b[?1049l",
    "\x1b[?25h",
    "\x1b[0m",
);

pub fn ensure_supported() -> Result<()> {
    ensure!(
        !is_mintty(),
        "this terminal window (mintty from Git Bash or MSYS2) cannot run interactive programs, open sai in Windows Terminal or PowerShell"
    );
    Ok(())
}

pub fn is_msys() -> bool {
    cfg!(windows) && env::var_os("MSYSTEM").is_some()
}

pub const MSYS_ADVICE: &str = "you run sai from MSYS2 or Git Bash, where Ctrl+C and leftover terminal codes are known to misbehave with Windows programs. sai works most reliably from a PowerShell tab in Windows Terminal";

pub fn is_mintty() -> bool {
    cfg!(windows) && env::var("TERM_PROGRAM").is_ok_and(|program| program == "mintty")
}

pub struct Session {
    saved: platform::Saved,
}

pub fn begin_session() -> Session {
    Session {
        saved: platform::begin(),
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        platform::allow_escape_sequences();
        let mut stdout = io::stdout();
        if stdout.is_terminal() {
            stdout.write_all(RESET_MODES.as_bytes()).ok();
            stdout.flush().ok();
        }
        platform::end(&self.saved);
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::Console::{
        CONSOLE_MODE, CTRL_BREAK_EVENT, CTRL_C_EVENT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
        GetConsoleMode, GetStdHandle, STD_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
        SetConsoleCtrlHandler, SetConsoleMode,
    };
    use windows_sys::core::BOOL;

    pub struct Saved {
        input: Option<CONSOLE_MODE>,
        output: Option<CONSOLE_MODE>,
    }

    unsafe extern "system" fn swallow(event: u32) -> BOOL {
        BOOL::from(event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT)
    }

    fn handle(kind: STD_HANDLE) -> HANDLE {
        unsafe { GetStdHandle(kind) }
    }

    fn mode(kind: STD_HANDLE) -> Option<CONSOLE_MODE> {
        let mut mode = 0;
        (unsafe { GetConsoleMode(handle(kind), &mut mode) } != 0).then_some(mode)
    }

    fn set_mode(kind: STD_HANDLE, mode: CONSOLE_MODE) {
        unsafe {
            SetConsoleMode(handle(kind), mode);
        }
    }

    pub fn begin() -> Saved {
        unsafe {
            SetConsoleCtrlHandler(Some(swallow), 1);
        }
        Saved {
            input: mode(STD_INPUT_HANDLE),
            output: mode(STD_OUTPUT_HANDLE),
        }
    }

    pub fn allow_escape_sequences() {
        if let Some(current) = mode(STD_OUTPUT_HANDLE) {
            set_mode(
                STD_OUTPUT_HANDLE,
                current | ENABLE_VIRTUAL_TERMINAL_PROCESSING,
            );
        }
    }

    pub fn end(saved: &Saved) {
        if let Some(input) = saved.input {
            set_mode(STD_INPUT_HANDLE, input);
        }
        if let Some(output) = saved.output {
            set_mode(STD_OUTPUT_HANDLE, output);
        }
        unsafe {
            SetConsoleCtrlHandler(Some(swallow), 0);
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub struct Saved;

    pub fn begin() -> Saved {
        Saved
    }

    pub fn allow_escape_sequences() {}

    pub fn end(_: &Saved) {}
}
