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

#[cfg(test)]
mod tests {
    use super::*;

    quick_error! {
        #[derive(Debug)]
        enum TestingErrors {
            DummyError {}
        }
    }

    enum Should {
        Succeed,
        Fail,
    }

    struct DummyLauncher {
        always: Should,
    }

    impl RitLauncher for DummyLauncher {
        fn launch(&self, _name: &str, _args: &[String]) -> Result<(), Box<dyn Error>> {
            match &self.always {
                Should::Succeed => Ok(()),
                Should::Fail => Err(Box::new(TestingErrors::DummyError {})),
            }
        }
    }

    #[test]
    fn proclauncher_launches_processes() {
        let launcher = ProcLauncher { cmd_name: "true" };
        assert!(launcher.launch("whatever", &[]).is_ok());
    }

    #[test]
    fn proclauncher_fails_on_nonexistent() {
        let launcher = ProcLauncher {
            cmd_name: "not-a-real-command",
        };
        assert!(launcher.launch("this-shouldnt-exist", &[]).is_err());
    }

    #[test]
    fn liblauncher_launches_libs() {
        let launcher = LibLauncher {};
        assert!(launcher.launch("test", &[]).is_ok());
    }

    #[test]
    fn liblauncher_fails_on_nonexistent() {
        let launcher = LibLauncher {};
        assert!(launcher.launch("this-shouldnt-exist", &[]).is_err());
    }

    #[test]
    fn blacklistlauncher_works_on_others() {
        let launcher = BlacklistLauncher {
            launcher: Box::new(DummyLauncher {
                always: Should::Succeed,
            }),
            blacklist: &["blacklisted"],
        };
        assert!(launcher.launch("not-blacklisted", &[]).is_ok());
    }

    #[test]
    fn blacklistlauncher_fails_on_blacklisted() {
        let launcher = BlacklistLauncher {
            launcher: Box::new(DummyLauncher {
                always: Should::Succeed,
            }),
            blacklist: &["blacklisted"],
        };
        assert!(launcher.launch("blacklisted", &[]).is_err());
    }

    #[test]
    #[should_panic]
    fn fallbacklauncher_panics_on_no_launchers() {
        let launcher = FallbackLauncher { launchers: vec![] };
        launcher.launch("whatever", &[]).unwrap();
    }

    #[test]
    fn fallbacklauncher_falls_back() {
        let launcher = FallbackLauncher {
            launchers: vec![
                Box::new(DummyLauncher {
                    always: Should::Fail,
                }),
                Box::new(DummyLauncher {
                    always: Should::Succeed,
                }),
            ],
        };
        assert!(launcher.launch("whatever", &[]).is_ok());
    }

    #[test]
    fn fallbacklauncher_ultimately_fails() {
        let launcher = FallbackLauncher {
            launchers: vec![
                Box::new(DummyLauncher {
                    always: Should::Fail,
                }),
                Box::new(DummyLauncher {
                    always: Should::Fail,
                }),
            ],
        };
        assert!(launcher.launch("whatever", &[]).is_err());
    }

    #[test]
    fn dummy_launcher_combination_works() {
        let launcher = FallbackLauncher {
            launchers: vec![
                Box::new(BlacklistLauncher {
                    launcher: Box::new(DummyLauncher {
                        always: Should::Succeed,
                    }),
                    blacklist: &["help"],
                }),
                Box::new(LibLauncher {}),
            ],
        };
        assert!(launcher.launch("status", &[]).is_ok());
    }

    #[test]
    fn default_launcher_works() {
        let launcher = get_default_launcher();
        assert!(launcher.launch("help", &[]).is_ok());
    }

    #[test]
    fn default_launcher_fails_on_nonexistent() {
        let launcher = get_default_launcher();
        assert!(launcher.launch("not-a-real-command", &[]).is_err());
    }
}
