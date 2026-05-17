#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
#[cfg(windows)]
use windows_sys::Win32::System::Console::{
    ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_OUTPUT_HANDLE,
    SetConsoleMode,
};

#[cfg(windows)]
const TARGET_COLS: u16 = 115;

#[cfg(windows)]
pub fn enable_ansi_support() {
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return;
        }

        let mut mode: u32 = 0;
        if GetConsoleMode(handle, &mut mode) == 0 {
            return;
        }

        let new_mode = mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
        SetConsoleMode(handle, new_mode);
    }
}

#[cfg(not(windows))]
pub fn enable_ansi_support() {}

#[cfg(windows)]
pub fn widen_window_if_needed() {
    use std::io::Write;

    if !is_launched_from_explorer() {
        return;
    }

    // xterm window manipulation: CSI 8 ; rows ; cols t. rows = 0 keeps current rows.
    // Windows Terminal honours this and resizes the host window; conhost ignores it.
    // Requires ENABLE_VIRTUAL_TERMINAL_PROCESSING — caller must enable ANSI first.
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b[8;0;{TARGET_COLS}t");
    let _ = out.flush();
}

#[cfg(not(windows))]
pub fn widen_window_if_needed() {}

#[cfg(windows)]
fn is_launched_from_explorer() -> bool {
    use std::mem;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;

    unsafe {
        let current_pid = GetCurrentProcessId();
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return false;
        }

        let mut entry: PROCESSENTRY32W = mem::zeroed();
        entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;

        let mut parent_pid = 0u32;
        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                if entry.th32ProcessID == current_pid {
                    parent_pid = entry.th32ParentProcessID;
                    break;
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        let mut is_explorer = false;
        if parent_pid != 0 {
            entry = mem::zeroed();
            entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;

            if Process32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    if entry.th32ProcessID == parent_pid {
                        let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(260);
                        let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
                        is_explorer = name.eq_ignore_ascii_case("explorer.exe");
                        break;
                    }
                    if Process32NextW(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }
        }

        CloseHandle(snapshot);
        is_explorer
    }
}
