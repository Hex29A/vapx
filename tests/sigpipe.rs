//! Regression test for #52: writing to a pipe whose reader has gone must end the
//! process with SIGPIPE, as for any Unix tool, not with a panic. Needs no camera.

#![cfg(unix)]

use std::fs::File;
use std::os::fd::FromRawFd;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

#[test]
fn closed_stdout_pipe_does_not_abort() {
    let dir = std::env::temp_dir().join(format!("vapx-sigpipe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("cameras.yaml");
    std::fs::write(&config, "cameras:\n  cam1:\n    host: 192.0.2.10\n").unwrap();

    // A pipe whose read end is closed before vapx starts, so its first write
    // is certain to fail with EPIPE (the same as `vapx ... | head` after head exits).
    let mut fds = [0i32; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    unsafe { libc::close(fds[0]) };
    let write_end = unsafe { File::from_raw_fd(fds[1]) };

    let status = Command::new(env!("CARGO_BIN_EXE_vapx"))
        .args(["--config", config.to_str().unwrap(), "config", "list"])
        .stdout(Stdio::from(write_end))
        .stderr(Stdio::null())
        .status()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);

    // Test builds unwind instead of aborting (`panic = "abort"` is release-only),
    // so a panic on EPIPE shows up here as exit code 101 and not SIGABRT.
    assert_eq!(
        status.signal(),
        Some(libc::SIGPIPE),
        "expected death by SIGPIPE on a broken pipe, got: {status:?}"
    );
}
