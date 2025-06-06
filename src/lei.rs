use std::{io, path::Path, process::Command};

use clap::ValueEnum;
use tempdir::TempDir;
use thiserror::Error;
use tracing::debug;

use crate::lei::leiless::LeiLess;
mod leiless;

const DEFAULT_INBOX: &str = "https://lore.kernel.org/all/";

#[derive(Debug, Error)]
pub enum Error {
    #[error("could not execute `lei`: {0}")]
    Start(#[from] io::Error),
    #[error("`lei` failed execution with error code {0}")]
    Code(i32),
    #[error("`lei` execution unexpectedly terminated by signal.")]
    Signal,
    #[error("failed to produce mdir from public-inbox: {0}")]
    NoLei(#[from] leiless::Error),
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

pub fn query(interval: Interval, query: &str, no_lei: bool) -> Result<TempDir> {
    let interval = match interval {
        Interval::Day => "2.day.ago",
        Interval::Week => "2.week.ago",
        Interval::Month => "3.month.ago",
        Interval::Year => "1.year.ago",
    };
    let tmpdir = TempDir::new("lkml-lei")?;
    let q = format!("({query}) AND rt:{interval}..");
    debug!("query: `{q}`");

    let cfg = PullCfg {
        inbox: DEFAULT_INBOX,
        threads: true,
        query: &q,
    };

    let lei: &dyn LeiLike = if no_lei { &LeiLess } else { &LeiCli };

    lei.download(&cfg, tmpdir.path()).map(|()| tmpdir)
}

#[derive(Debug)]
pub struct PullCfg<'a> {
    /// The lore server address.
    pub inbox: &'a str,
    /// True to retrieve an entire thread if a middle query is matched.
    pub threads: bool,
    /// public-inbox query.
    pub query: &'a str,
}

/// Abstraction over CLI `lei` and our implementation.
trait LeiLike {
    /// Download  a query to a given directory.
    fn download(&self, cfg: &PullCfg, dir: &Path) -> Result<()>;
}

/// `LeiLike` interfaces using the `lei` CLI.
struct LeiCli;

impl LeiLike for LeiCli {
    fn download(&self, cfg: &PullCfg, dir: &Path) -> Result<()> {
        let mut cmd = Command::new("lei");
        cmd.arg("q")
            .args([
                // don't store the query, as we're storing it in our config.
                "--no-save",
                // get all emails from the thread where a single one has matched.
                "--threads",
                "--include",
                cfg.inbox,
            ])
            .arg(format!("--output={}", dir.display()))
            .arg(cfg.query);
        debug!("{cmd:?}");
        let res = cmd.status()?;
        if !res.success() {
            Err(res.code().map(Error::Code).unwrap_or(Error::Signal))
        } else {
            Ok(())
        }
    }
}
