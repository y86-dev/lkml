use std::{io, path::Path, process::Command};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("could not execute `git`: {0}")]
    Start(#[from] io::Error),
    #[error("`git` failed execution with error code {0}")]
    Code(i32),
    #[error("`git` execution unexpectedly terminated by signal.")]
    Signal,
}

type Result<T = ()> = core::result::Result<T, Error>;

fn git<'a>(cmd: impl IntoIterator<Item = &'a str>, dir: impl AsRef<Path>) -> Result {
    let res = Command::new("git").args(cmd).current_dir(dir).status()?;
    if res.success() {
        Ok(())
    } else {
        Err(res.code().map(Error::Code).unwrap_or(Error::Signal))
    }
}

pub fn add(dir: impl AsRef<Path>) -> Result {
    git(["add", "."], dir)
}

pub fn commit(message: &str, dir: impl AsRef<Path>) -> Result {
    git(["commit", "-m", message], dir)
}

pub fn push(dir: impl AsRef<Path>) -> Result {
    git(["push"], dir)
}

pub fn pull(dir: impl AsRef<Path>) -> Result {
    git(["pull"], dir)
}

pub fn is_clean(dir: impl AsRef<Path>) -> Result<bool> {
    let res = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .output()?;
    if res.status.success() {
        assert!(res.stderr.is_empty());
        Ok(res.stdout.is_empty())
    } else {
        Err(res.status.code().map(Error::Code).unwrap_or(Error::Signal))
    }
}
