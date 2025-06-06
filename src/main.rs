//! # `lkml`
//!
//! A command line tool to download mailing list emails via `lei` and then assort them into
//! maildirs based on custom criteria.
//!
//! <div class="warning">
//!
//! **WARNING**: this program is still pretty experimental and might break your emails, use with
//! care!
//!
//! </div>
//!
//! ## Configuration
//!
//! See [`Config`] for the various configuration options. The location of the config file is
//! `~/.config/lkml/config.toml` on linux.

use std::{
    io,
    path::Path,
    process::{Command, ExitCode},
};

use anyhow::Result;
use clap::Parser;
use maildir::Maildir;
use thiserror::Error;
use tracing::debug;
use tracing_subscriber::{filter::EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{config::Config, lei::Interval};

mod assort;
mod config;
mod git;
mod lei;
mod util;

type BoxStr = Box<str>;
type BoxPath = Box<Path>;

#[derive(Parser, Debug)]
struct Args {
    /// The amount of time to scan back
    interval: Option<Interval>,
}

fn main() -> Result<ExitCode> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();
    let args = Args::parse();
    let config = config::load()?;
    debug!("loaded config: {config:#?}");
    run(
        args.interval.unwrap_or(Interval::Day),
        &config.path,
        &config,
    )
}

fn run(interval: Interval, store: &Path, config: &Config) -> Result<ExitCode> {
    if let Some(git) = &config.git {
        if !git::is_clean(store)? {
            eprintln!("git repository not clean, refusing to update emails.");
            return Ok(ExitCode::FAILURE);
        }
        if git.pull {
            git::pull(store)?;
        }
    }
    let new = lei::query(interval, &config.query, config.no_lei)?;
    assort::run(new, Maildir::from(store.to_owned()), config)?;
    let mut did_commit = false;
    if config.git.is_some() && !git::is_clean(store)? {
        git::add(store)?;
        git::commit("update", store)?;
        did_commit = true;
    }
    if let Some(cfg) = &config.client {
        client(&cfg.command, store)?;
    }
    if let Some(git) = &config.git {
        if !git::is_clean(store)? {
            git::add(store)?;
            git::commit("read", store)?;
            did_commit = true;
        }
        if git.push && did_commit {
            git::push(store)?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Debug, Error)]
enum ClientError {
    #[error("could not execute custom mail client: {0}")]
    Start(#[from] io::Error),
    #[error("custom mail client failed execution with error code {0}")]
    Code(i32),
    #[error("custom mail client execution unexpectedly terminated by signal.")]
    Signal,
}

fn client(cmd: &[String], store: &Path) -> Result<(), ClientError> {
    let res = Command::new(&cmd[0])
        .args(&cmd[1..])
        .current_dir(store)
        .status()?;
    if res.success() {
        Ok(())
    } else {
        Err(res
            .code()
            .map(ClientError::Code)
            .unwrap_or(ClientError::Signal))
    }
}
