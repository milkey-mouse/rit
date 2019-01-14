/// Launcher for rit subcommands.

#[macro_use]
extern crate quick_error;

use std::error::Error;
use std::io::ErrorKind;
use std::process;

quick_error! {
    #[derive(Debug)]
    pub enum LaunchFailed {
        // TODO: figure out lifetimes and don't copy names
        NotFound(name: String) {
            description("This command was not found on the system")
            display(r#"The command "{}" was not found on the system"#, name)
        }
        Blacklisted(name: &'static str) {
            description("This command is blacklisted from this launcher")
            display(r#"The command "{}" is blacklisted from this launcher"#, name)
        }
        BadExitCode(name: String, status: process::ExitStatus) {
            description("The command ran, but returned a code indicating failure")
            display(r#"The command "{}" {}."#, name, match status.code() {
                Some(code) => format!("returned error code {}", code),
                // TODO: conditionally include std::os::unix & get signal name here
                None => "was terminated by a signal".to_string(),
            })
        }
    }
}

pub trait RitLauncher {
    fn launch(&self, name: &str, args: &[String]) -> Result<(), Box<dyn Error>>;
}

/// Launches rit subcommands as a separate process.
pub struct ProcLauncher<'a> {
    /// Name of the base command, nominally rit or git.
    cmd_name: &'a str,
}

impl<'a> RitLauncher for ProcLauncher<'a> {
    fn launch(&self, name: &str, args: &[String]) -> Result<(), Box<dyn Error>> {
        // note: to be closer to git's behavior we could use libc::execv() here
        match process::Command::new(self.cmd_name)
            .arg(name)
            .args(args)
            .status()
        {
            Ok(status) if status.code() == Some(0) => Ok(()),
            Ok(status) => Err(Box::new(LaunchFailed::BadExitCode(
                name.to_string(),
                status,
            ))),
            Err(e) => {
                if e.kind() == ErrorKind::NotFound {
                    Err(Box::new(LaunchFailed::NotFound(name.to_string())))
                } else {
                    Err(Box::new(e))
                }
            }
        }
    }
}

/// Launches rit subcommands by calling their run() function. If a function is actually implemented
/// in rit (as opposed to actually calling a git subcommand), this launcher should be preferred.
pub struct LibLauncher;

impl RitLauncher for LibLauncher {
    fn launch(&self, name: &str, args: &[String]) -> Result<(), Box<dyn Error>> {
        // TODO: actually launch stuff
        Ok(())
    }
}

/// Wraps another RitLauncher and immediately errors if command name is in a blacklist.
pub struct BlacklistLauncher {
    /// Launcher to be wrapped.
    launcher: Box<dyn RitLauncher>,
    /// Blacklist of forbidden command names.
    blacklist: &'static [&'static str],
}

impl RitLauncher for BlacklistLauncher {
    fn launch(&self, name: &str, args: &[String]) -> Result<(), Box<dyn Error>> {
        for forbidden_name in self.blacklist.iter() {
            if name == *forbidden_name {
                return Err(Box::new(LaunchFailed::Blacklisted(forbidden_name)));
            }
        }
        return self.launcher.launch(name, args);
    }
}

/// Wraps multiple RitLaunchers and falls back to the next if the launcher fails.
pub struct FallbackLauncher {
    launchers: Vec<Box<dyn RitLauncher>>,
}

impl RitLauncher for FallbackLauncher {
    fn launch(&self, name: &str, args: &[String]) -> Result<(), Box<dyn Error>> {
        let (last, firsts) = self
            .launchers
            .split_last()
            .expect("no launchers given to FallbackLauncher");
        for launcher in firsts.iter() {
            if let Ok(x) = launcher.launch(name, &args) {
                return Ok(x);
            }
        }
        return last.launch(name, args);
    }
}

pub fn get_default_launcher() -> impl RitLauncher {
    FallbackLauncher {
        launchers: vec![
            Box::new(BlacklistLauncher {
                launcher: Box::new(ProcLauncher { cmd_name: "git" }),
                // git help is part of the main launcher command for the OG git
                blacklist: &["help"],
            }),
            Box::new(LibLauncher {}),
        ],
    }
}
