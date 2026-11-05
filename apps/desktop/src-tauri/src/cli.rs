//! Command-line contract of `Upscalee.exe`.
//!
//! | Invocation                              | Meaning                                    |
//! |-----------------------------------------|--------------------------------------------|
//! | `Upscalee.exe`                          | Start (if needed) and open Settings        |
//! | `Upscalee.exe --background`             | Start in the tray only (Run-at-login)      |
//! | `Upscalee.exe --pick <files…>`          | Open the radial picker for these files     |
//! | `Upscalee.exe <files…>`                 | Same as `--pick` (drag & drop onto the exe)|
//! | `Upscalee.exe --settings`               | Open Settings                              |
//! | `Upscalee.exe --register`               | Install Explorer integration, then exit    |
//! | `Upscalee.exe --unregister`             | Remove Explorer integration, then exit     |

use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchCommand {
    Default,
    Background,
    Pick(Vec<PathBuf>),
    Settings,
    Register,
    Unregister,
}

pub fn parse<I: IntoIterator<Item = OsString>>(args: I) -> LaunchCommand {
    let mut background = false;
    let mut settings = false;
    let mut register = false;
    let mut unregister = false;
    let mut paths = Vec::new();

    for arg in args {
        match arg.to_str() {
            Some("--background") | Some("/background") => background = true,
            Some("--settings") => settings = true,
            Some("--register") => register = true,
            Some("--unregister") => unregister = true,
            Some("--pick") => {}
            Some(flag) if flag.starts_with("--") => {
                tracing::debug!(flag, "ignoring unknown flag");
            }
            _ => {
                if !arg.is_empty() {
                    paths.push(PathBuf::from(arg));
                }
            }
        }
    }

    if unregister {
        LaunchCommand::Unregister
    } else if register {
        LaunchCommand::Register
    } else if !paths.is_empty() {
        LaunchCommand::Pick(paths)
    } else if settings {
        LaunchCommand::Settings
    } else if background {
        LaunchCommand::Background
    } else {
        LaunchCommand::Default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> LaunchCommand {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn parses_every_mode() {
        assert_eq!(p(&[]), LaunchCommand::Default);
        assert_eq!(p(&["--background"]), LaunchCommand::Background);
        assert_eq!(p(&["--settings"]), LaunchCommand::Settings);
        assert_eq!(p(&["--register"]), LaunchCommand::Register);
        assert_eq!(p(&["--unregister", "--register"]), LaunchCommand::Unregister);
        assert_eq!(p(&["--pick", r"C:\a b\x.png"]), LaunchCommand::Pick(vec![PathBuf::from(r"C:\a b\x.png")]));
        assert_eq!(
            p(&[r"C:\x.png", r"C:\y.jpg"]),
            LaunchCommand::Pick(vec![PathBuf::from(r"C:\x.png"), PathBuf::from(r"C:\y.jpg")])
        );
    }

    #[test]
    fn unknown_flags_and_empty_args_are_ignored() {
        assert_eq!(p(&["--wat", ""]), LaunchCommand::Default);
        assert_eq!(p(&["--background", "--pick"]), LaunchCommand::Background);
    }
}
