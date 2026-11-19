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

/// Prompts hide the cursor while they wait for a choice and leave it hidden when they end
/// with an error, for example on Ctrl+C. sai shows it again on the way out
pub fn show_cursor() {
    let mut stdout = io::stdout();
    if stdout.is_terminal() {
        platform::allow_escape_sequences();
        stdout.write_all(b"\x1b[?25h").ok();
        stdout.flush().ok();
    }
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
        GetConsoleCP, GetConsoleMode, GetConsoleOutputCP, GetStdHandle, STD_HANDLE,
        STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetConsoleCP, SetConsoleCtrlHandler, SetConsoleMode,
        SetConsoleOutputCP,
    };

    // The agent talks UTF-8. A console left on an old code page (437, 866), as in MSYS2,
    // shows its text as mojibake, so sessions switch the console to UTF-8 and back
    const UTF8: u32 = 65001;
    use windows_sys::core::BOOL;

    pub struct Saved {
        input: Option<CONSOLE_MODE>,
        output: Option<CONSOLE_MODE>,
        input_page: u32,
        output_page: u32,
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
        let saved = Saved {
            input: mode(STD_INPUT_HANDLE),
            output: mode(STD_OUTPUT_HANDLE),
            input_page: unsafe { GetConsoleCP() },
            output_page: unsafe { GetConsoleOutputCP() },
        };
        unsafe {
            SetConsoleCtrlHandler(Some(swallow), 1);
            if saved.input_page != 0 {
                SetConsoleCP(UTF8);
            }
            if saved.output_page != 0 {
                SetConsoleOutputCP(UTF8);
            }
        }
        saved
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
            if saved.input_page != 0 {
                SetConsoleCP(saved.input_page);
            }
            if saved.output_page != 0 {
                SetConsoleOutputCP(saved.output_page);
            }
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
