//pub mod mig_error;
use failure::ResultExt;
use log::{trace,debug};
use std::process::{Command, ExitStatus, Stdio};
use std::fmt::{self, Display, Formatter};

pub mod mig_error;
use mig_error::{MigErrCtx, MigError, MigErrorKind};

pub mod os_release;

pub mod config;

pub mod logger;

const MODULE: &str = "common";

#[derive(Debug)]
pub enum OSArch {
    AMD64,
    ARMHF,
    I386,
 /*   ARM64,
    ARMEL,
    MIPS,
    MIPSEL,
    Powerpc,
    PPC64EL,
    S390EX, */

}

impl Display for OSArch {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub(crate) struct CmdRes {
    pub stdout: String,
    pub stderr: String,
    pub status: ExitStatus,
}

pub(crate) fn call(cmd: &str, args: &[&str], trim_stdout: bool) -> Result<CmdRes, MigError> {
    trace!(
        "call(): '{}' called with {:?}, {}",
        cmd,
        args,
        trim_stdout
    );

    let output = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "{}::call: failed to execute: command {} '{:?}'",
                MODULE, cmd, args
            ),
        ))?;

    Ok(CmdRes {
        stdout: match trim_stdout {
            true => String::from(String::from_utf8_lossy(&output.stdout).trim()),
            false => String::from(String::from_utf8_lossy(&output.stdout)),
        },
        stderr: String::from(String::from_utf8_lossy(&output.stderr)),
        status: output.status,
    })
}

pub(crate) fn check_tcp_connect(host: &str, port: u16, timeout: u64) -> Result<(),MigError> {
    use std::time::Duration;
    use std::net::{TcpStream, ToSocketAddrs, Shutdown };
    let url = format!("{}:{}",host, port);
    let mut addrs_iter = url.to_socket_addrs()
        .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::check_tcp_connect: failed to resolve host address: '{}'", MODULE, url)))?;

    if let Some(ref sock_addr) = addrs_iter.next() {
        let tcp_stream = TcpStream::connect_timeout(sock_addr, Duration::new(timeout, 0))
            .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::check_tcp_connect: failed to connect to: '{}' with timeout: {}", MODULE, url, timeout)))?;

        let _res = tcp_stream.shutdown(Shutdown::Both);    
         Ok(())
    } else {
        Err(MigError::from_remark(MigErrorKind::InvState, &format!("{}::check_tcp_connect: no results from name resolution for: '{}", MODULE, url)))
    }
}
