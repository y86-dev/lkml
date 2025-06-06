use std::{
    fs,
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use flate2::read::GzDecoder;
use thiserror::Error;
use tracing::info;

use super::PullCfg;
use crate::{
    BoxStr,
    lei::LeiLike,
    util::{self, ReadCounter},
};

#[derive(Debug, Error)]
pub enum Error {
    #[error("could not reach `{inbox}`: {error}")]
    Http { inbox: BoxStr, error: ureq::Error },
    #[error("could not create directory `{path}`: {error}")]
    CreateDirectory { path: PathBuf, error: io::Error },
    #[error("could not write new mail file `{path}`: {error}")]
    WriteMailFile { path: PathBuf, error: io::Error },
    #[error("failed to read from response {error}")]
    ReadInputStream { error: io::Error },
}

type Result<T = ()> = core::result::Result<T, Error>;

/// Every N bytes, post an update.
const MSG_INTERVAL: u64 = 10 * 1024;

const PATH_SEP: char = if cfg!(windows) { ';' } else { ':' };

/// `lei` interfaces using Rust implementations.
pub struct LeiLess;

impl LeiLike for LeiLess {
    fn download(&self, cfg: &PullCfg, dir: &Path) -> super::Result<()> {
        Ok(download_impl(cfg, dir)?)
    }
}

fn download_impl(cfg: &PullCfg, dir: &Path) -> Result<()> {
    let mut req = ureq::post(cfg.inbox).query("x", "m");
    if cfg.threads {
        req = req.query("t", "1");
    }
    req = req.query("q", cfg.query);

    info!("sending request {req:?}");
    let resp = req.send_empty().map_err(|e| Error::Http {
        inbox: cfg.inbox.into(),
        error: e,
    })?;

    // Lore doesn't seem to set `Content-Length`, but we still try
    let expected_len: Option<u64> = resp
        .headers()
        .get("Content-Length")
        .and_then(|hval| hval.to_str().ok())
        .and_then(|hstr| hstr.parse().ok());

    let status = resp.status();
    if let Some(l) = expected_len {
        info!("status {status}; receiving {l} bytes",);
    } else {
        info!("status {status}; unknown length",);
    };

    let body_reader = BufReader::new(ReadCounter::new(GzDecoder::new(ReadCounter::new(
        resp.into_body().into_reader(),
    ))));
    let cur = create_maildir(dir)?;

    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut msg_count = 0u64;
    let create_fname = || {
        let name = format!("{time:010}.{msg_count:05}.mbox{PATH_SEP}2,");
        msg_count += 1;
        name
    };

    let res = mbox2mdir(
        body_reader,
        &cur,
        create_fname,
        |r| r.get_ref().get_ref().get_ref().count(),
        |r| r.get_ref().count(),
    );
    println!();
    res
}

/// Given an input stream that produces `mbox`, split it into individual messages.
///
/// Takes callbacks so we can test this without the ureq stream. `println!()` should be called
/// after this to provide a newline flush.
///
/// <https://github.com/mindbit/mb2md/blob/52d9a9480f521a1e3dda83a0845e6ccfa84e54aa/mb2md.pl>
fn mbox2mdir<R: BufRead>(
    mut r: R,
    dst: &Path,
    mut create_fname: impl FnMut() -> String,
    get_downloaded: impl Fn(&R) -> u64,
    get_extracted: impl Fn(&R) -> u64,
) -> Result<()> {
    let mut buf = Vec::new();
    let mut msg = Vec::with_capacity(1024);
    let mut total_read = 0u64;

    loop {
        // Read a single line
        buf.clear();
        let read_count = r
            .read_until(b'\n', &mut buf)
            .map_err(|error| Error::ReadInputStream { error })?;
        let line = &buf[..read_count];

        let read_count_u64 = u64::try_from(read_count).unwrap();
        let new_total_read = total_read + read_count_u64;
        if new_total_read / MSG_INTERVAL > total_read / MSG_INTERVAL {
            let downloaded = get_downloaded(&r);
            let extracted = get_extracted(&r);
            print!(
                "\r{downloaded} B downloaded , {extracted} B extracted , {new_total_read} B read "
            );
        }

        total_read = new_total_read;

        /* This logic is taken from mb2md's `convert` function (not all functionality
         * is implemented) */

        // Start of a new message or EOF
        if line.starts_with(b"From ") || read_count == 0 {
            if !msg.is_empty() {
                let fname = dst.join(create_fname());

                // Ensure single trailing newline
                msg.truncate(msg.trim_ascii_end().len());
                msg.push(b'\n');
                let trimmed = msg.trim_ascii_start();

                // Write the file, failing if it exists
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&fname)
                    .and_then(|mut f| f.write_all(trimmed))
                    .map_err(|error| Error::WriteMailFile { path: fname, error })?;

                msg.clear();
            }

            if read_count == 0 {
                break;
            }

            continue;
        }

        // mbox uses `^From ` as the separator and quotes it as `^> From`. Account for this
        // without allocating an intermediate.
        match line.strip_prefix(b"> From ") {
            Some(stripped) => {
                msg.extend_from_slice(b"From ");
                msg.extend_from_slice(stripped);
            }
            None => msg.extend_from_slice(&line),
        }
    }

    let downloaded = get_downloaded(&r);
    let extracted = get_extracted(&r);
    println!("\rComplete: {downloaded} B downloaded, {extracted} B extracted, {total_read} B read");
    Ok(())
}

/// Create a new mail directory structure, returning the `cur` path.
fn create_maildir(dir: &Path) -> Result<PathBuf> {
    let cur = dir.join("cur");
    create_dir_if_not_exists(&dir)?;
    create_dir_if_not_exists(&dir.join("tmp"))?;
    create_dir_if_not_exists(&dir.join("new"))?;
    create_dir_if_not_exists(&cur)?;
    Ok(cur)
}

/// Wrap the function in `util` with a specific error type.
fn create_dir_if_not_exists(path: &Path) -> Result<()> {
    util::create_dir_if_not_exists(path).map_err(|error| Error::CreateDirectory {
        path: path.to_owned(),
        error,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use indoc::indoc;
    use pretty_assertions::assert_eq;
    use tempdir::TempDir;

    use super::*;

    const MBOX: &str = indoc! {r#"
        From mboxrd@z Thu Jan  1 00:00:00 1970
        X-Some-Header: HeaderValue
        From: Benno Lossin <b@l.org>
        To: Trevor Gross <t@g.org>
        Subject: Subject

        Body1

        From mboxrd@z Thu Jan  1 00:00:00 1970
        From: Trevor Gross <t@g.org>
        To: Benno Lossin <b@l.org>
        Subject: Subject

        Body2
        This is mbox quoting, should turn into `From `:
        > From hi

        From mboxrd@z Thu Jan  1 00:00:00 1970
        From: mail@email.org
        To: List <list@mail.org>
        Subject: Subject3

        Body3
    "#};

    #[test]
    fn basic_split() {
        let dir = TempDir::new("test-mbox").unwrap();
        let mut idx = 0;

        mbox2mdir(
            Cursor::new(MBOX),
            dir.path(),
            || {
                let name = format!("name{idx}");
                idx += 1;
                name
            },
            |_| 0,
            |_| 0,
        )
        .unwrap();

        for entry in fs::read_dir(dir.path()).unwrap() {
            let entry = entry.unwrap();
            let fname = entry.file_name();

            let expected = if fname == "name0" {
                indoc! {r#"
                    X-Some-Header: HeaderValue
                    From: Benno Lossin <b@l.org>
                    To: Trevor Gross <t@g.org>
                    Subject: Subject

                    Body1
                "#}
            } else if fname == "name1" {
                indoc! {r#"
                    From: Trevor Gross <t@g.org>
                    To: Benno Lossin <b@l.org>
                    Subject: Subject

                    Body2
                    This is mbox quoting, should turn into `From `:
                    From hi
                "#}
            } else if fname == "name2" {
                indoc! {r#"
                    From: mail@email.org
                    To: List <list@mail.org>
                    Subject: Subject3

                    Body3
                "#}
            } else {
                panic!("unexpected file {}", entry.path().display());
            };

            let contents = fs::read_to_string(entry.path()).unwrap();
            assert_eq!(contents, expected, "file: {fname:?}");
        }
    }
}
