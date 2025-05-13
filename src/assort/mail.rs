use std::{hash::Hash, path::PathBuf};

use maildir::{MailEntry, MailEntryError};
use mailparse::{MailHeaderMap, ParsedMail};
use thiserror::Error;

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Type {
    New,
    Folder(usize),
}

pub struct Mail<'a> {
    pub typ: Type,
    pub id: String,
    pub maildir_id: String,
    pub parent: Option<String>,
    pub parsed: ParsedMail<'a>,
    pub path: PathBuf,
}

impl PartialEq for Mail<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl Eq for Mail<'_> where PathBuf: Eq {}

impl Hash for Mail<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.path.hash(state);
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("`{0}` is missing an `Message-ID` header.")]
    MissingID(PathBuf),
    #[error("`{1}` has {0} `Message-ID` headers and none are preferred.")]
    MultipleIDs(usize, PathBuf),
    #[error("`{1}` has {0} `In-Reply-To` headers.")]
    MultiReply(usize, PathBuf),
    #[error("could not parse mail: {0}")]
    MailEntry(#[from] MailEntryError),
}

pub fn parse<'a>(mail: &'a mut MailEntry, typ: Type, cfg: &Config) -> Result<Mail<'a>, Error> {
    let path = mail.path().to_owned();
    let maildir_id = mail.id().to_owned();
    let parsed = mail.parsed()?;
    let id = parsed.headers.get_all_headers("Message-ID");
    let id = match id.len() {
        0 => return Err(Error::MissingID(path)),
        1 => id[0].get_value(),
        len => {
            if let Some(id) = id
                .iter()
                .find(|id| cfg.quirks.prefer.contains(&id.get_value()))
            {
                id.get_value()
            } else {
                return Err(Error::MultipleIDs(len, path));
            }
        }
    };
    let id = id
        .trim_start_matches(|c| c != '<')
        .trim_end_matches(|c| c != '>')
        .to_owned();
    let parent = parsed.headers.get_all_headers("In-Reply-To");
    let parent = match parent.len() {
        0 => None,
        1 => Some(parent[0].get_value()),
        len => return Err(Error::MultiReply(len, path)),
    };
    let parent = parent.map(|p| {
        p.trim_start_matches(|c| c != '<')
            .trim_end_matches(|c| c != '>')
            .to_owned()
    });
    Ok(Mail {
        maildir_id,
        id,
        parsed,
        typ,
        parent,
        path,
    })
}
