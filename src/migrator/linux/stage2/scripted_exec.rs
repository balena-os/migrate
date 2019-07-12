// Stage 2 command execution - either call directly or gather to output as script
use failure::ResultExt;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use crate::{
    common::{MigErrCtx, MigError, MigErrorKind},
    linux::{EnsuredCmds, CHMOD_CMD},
};

// TODO: add some fancy script intro
const SCRIPT_TEMPLATE: &str = r###"
#!/bin/sh
set -e
"###;

struct ScriptifyInfo {
    buffer: String,
    chmod_cmd: String,
}

enum ExecMode {
    Scriptified(ScriptifyInfo),
    Direct,
}

pub(crate) struct ScriptedExec {
    exec_mode: ExecMode,
    // cmds: &'a EnsuredCmds,
}

impl ScriptedExec {
    pub fn new(scriptify: bool, cmds: &EnsuredCmds) -> Result<ScriptedExec, MigError> {
        Ok(ScriptedExec {
            exec_mode: if scriptify {
                ExecMode::Scriptified(ScriptifyInfo {
                    buffer: String::new(),
                    chmod_cmd: String::from(cmds.get(CHMOD_CMD)?),
                })
            } else {
                ExecMode::Direct
            },
        })
    }

    pub fn is_scripted(&self) -> bool {
        if let ExecMode::Scriptified(_) = self.exec_mode {
            true
        } else {
            false
        }
    }

    pub fn add_to_script(&mut self, cmd: &str) -> bool {
        if let ExecMode::Scriptified(ref mut info) = self.exec_mode {
            info.buffer.push_str(cmd);
            info.buffer.push('\n');
            true
        } else {
            false
        }
    }

    pub fn execute_with_stdin(
        &mut self,
        cmd: &str,
        args: &[&str],
        stdin: &str,
        capture_output: bool,
    ) -> Result<Option<Output>, MigError> {
        match self.exec_mode {
            ExecMode::Direct => {
                let mut command = Command::new(cmd);

                command.args(args);

                if capture_output {
                    command.stdout(Stdio::piped()).stderr(Stdio::piped());
                } else {
                    command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
                }

                let mut child = Command::new(cmd)
                    .stdin(Stdio::piped())
                    .args(args)
                    .spawn()
                    .context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("Failed to execute '{}' with args: {:?}", cmd, args),
                    ))?;

                if let Some(ref mut child_stdin) = child.stdin {
                    let _res =
                        child_stdin
                            .write(stdin.as_bytes())
                            .context(MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!("failed to write to command stdin"),
                            ))?;
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::NotFound,
                        "Could not retrieve stdout for command",
                    ));
                }

                Ok(Some(child.wait_with_output().context(
                    MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("Command failed to terminate"),
                    ),
                )?))
            }
            ExecMode::Scriptified(ref mut info) => {
                info.buffer.push_str(cmd);
                info.buffer.push(' ');
                for arg in args {
                    info.buffer.push_str(arg);
                    info.buffer.push(' ');
                }
                info.buffer.push_str("<< EOI\n");
                info.buffer.push_str(stdin);
                info.buffer.push_str("EOI\n");
                Ok(None)
            }
        }
    }

    pub fn execute(
        &mut self,
        cmd: &str,
        args: &Vec<&str>,
        capture_output: bool,
    ) -> Result<Option<Output>, MigError> {
        match self.exec_mode {
            ExecMode::Direct => {
                let mut command = Command::new(cmd);

                command.args(args);

                if capture_output {
                    command.stdout(Stdio::piped()).stderr(Stdio::piped());
                } else {
                    command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
                }

                Ok(Some(Command::new(cmd).args(args).output().context(
                    MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("Failed to execute '{}' with args: {:?}", cmd, args),
                    ),
                )?))
            }
            ExecMode::Scriptified(ref mut info) => {
                info.buffer.push_str(cmd);
                info.buffer.push(' ');
                for arg in args {
                    info.buffer.push_str(arg);
                    info.buffer.push(' ');
                }
                info.buffer.push('\n');
                Ok(None)
            }
        }
    }

    pub fn write_to<P: AsRef<Path>>(&self, file_name: P) -> Result<(), MigError> {
        let file_name = file_name.as_ref();
        match self.exec_mode {
            ExecMode::Direct => Err(MigError::from_remark(
                MigErrorKind::InvParam,
                "cannot write commands in direct mode",
            )),
            ExecMode::Scriptified(ref info) => {
                let mut file = OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .create(true)
                    .open(file_name)
                    .context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("Failed to write to script file: '{}'", file_name.display()),
                    ))?;

                let _res = file
                    .write(&format!("{}\n{}\n", SCRIPT_TEMPLATE, info.buffer).as_bytes())
                    .context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("Failed to write to file '{}'", file_name.display()),
                    ))?;

                let cmd_res = Command::new(&info.chmod_cmd)
                    .args(&["+x", &file_name.to_string_lossy()])
                    .output()
                    .context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "Failed to make command executable: '{}'",
                            file_name.display()
                        ),
                    ))?;

                if cmd_res.status.success() {
                    Ok(())
                } else {
                    Err(MigError::from_remark(
                        MigErrorKind::ExecProcess,
                        &format!(
                            "Failed to execute chmod command, stderr: {:?}",
                            cmd_res.stderr
                        ),
                    ))
                }
            }
        }
    }
}
