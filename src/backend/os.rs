//! Backend OS classification and syscall lowering placeholders.
//!
//! The current backend still lowers debug I/O through libc declarations such as
//! `printf`/`puts`. This module is the central place for future direct syscall
//! lowering per target OS.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOs {
    Linux,
    Macos,
    Windows,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoLowering {
    Libc,
    LinuxSyscall,
    MacosSyscall,
    WindowsApi,
    Placeholder,
}

pub fn current() -> TargetOs {
    if cfg!(target_os = "linux") {
        TargetOs::Linux
    } else if cfg!(target_os = "macos") {
        TargetOs::Macos
    } else if cfg!(target_os = "windows") {
        TargetOs::Windows
    } else {
        TargetOs::Unknown
    }
}

pub fn log_lowering(target: TargetOs) -> IoLowering {
    match target {
        // Placeholder: keep using libc while the syscall ABI lowering matures.
        TargetOs::Linux => IoLowering::Libc,
        TargetOs::Macos => IoLowering::Libc,
        TargetOs::Windows => IoLowering::Libc,
        TargetOs::Unknown => IoLowering::Placeholder,
    }
}

pub fn inp_lowering(target: TargetOs) -> IoLowering {
    match target {
        // Placeholder: stdin syscall/API lowering is not implemented yet.
        TargetOs::Linux => IoLowering::Placeholder,
        TargetOs::Macos => IoLowering::Placeholder,
        TargetOs::Windows => IoLowering::Placeholder,
        TargetOs::Unknown => IoLowering::Placeholder,
    }
}
