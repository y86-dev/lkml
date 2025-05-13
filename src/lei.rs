use std::{io, process::Command};

use clap::ValueEnum;
use tempdir::TempDir;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("could not execute `lei`: {0}")]
    Start(#[from] io::Error),
    #[error("`lei` failed execution with error code {0}")]
    Code(i32),
    #[error("`lei` execution unexpectedly terminated by signal.")]
    Signal,
}

type Result<T = ()> = core::result::Result<T, Error>;

#[derive(ValueEnum, Clone, Copy, Debug)]
pub enum Interval {
    /// Searches mails up to 2 days ago.
    Day,
    /// Searches mails up to 2 weeks ago.
    Week,
    /// Searches mails up to 3 months ago.
    Month,
    /// Searches mails up to 1 year ago.
    Year,
}

pub fn query(interval: Interval, query: &str) -> Result<TempDir> {
    let interval = match interval {
        Interval::Day => "2.day.ago",
        Interval::Week => "2.week.ago",
        Interval::Month => "3.month.ago",
        Interval::Year => "1.year.ago",
    };
    let tmpdir = TempDir::new("lkml-lei")?;
    let res = Command::new("lei")
        .arg("q")
        .args([
            // don't store the query, as we're storing it in our config.
            "--no-save",
            // get all emails from the thread where a single one has matched.
            "--threads",
            "--include=https://lore.kernel.org/all",
        ])
        .arg(format!("--output={}", tmpdir.path().display()))
        .arg(format!("({query}) AND rt:{interval}.."))
        .status()?;
    if !res.success() {
        Err(res.code().map(Error::Code).unwrap_or(Error::Signal))
    } else {
        Ok(tmpdir)
    }
}
