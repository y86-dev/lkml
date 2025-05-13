// `Regex` contains interior mutability, but we don't depend on that for hashing/equality
#![expect(clippy::mutable_key_type)]

use std::{collections::HashSet, fs, hash::Hash, io, path::PathBuf};

use directories_next::BaseDirs;
use regex::Regex;
use serde::Deserialize;
use thiserror::Error;

/// Configuration for `lkml`.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Path to the main maildir directory.
    pub path: PathBuf,

    /// `lei q` query to run.
    ///
    /// # Examples
    ///
    /// ```toml
    /// query = "dfn:^rust/ OR l:rust-for-linux.vger.kernel.org"
    /// ```
    pub query: String,

    /// Quirk fixes for mail clients, mailing lists etc.
    #[serde(default)]
    pub quirks: Quirks,

    /// Your own name + mail addresses.
    ///
    /// All mails from these addresses will be marked as read, since you sent them yourself.
    pub addresses: HashSet<String>,

    /// Control which emails have the `Flagged` flag set.
    #[serde(default)]
    pub flagging: Flagging,

    /// Array of folders to categorize mails into.
    pub folders: Vec<Folder>,

    /// Mail client configuration.
    ///
    /// If not specified, no mail client will be opened.
    pub client: Option<Client>,

    /// Git integration.
    pub git: Option<Git>,

    pub ignore: Option<Ignore>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Folder {
    /// Name of the folder.
    ///
    /// This means that there should be a folder starting with a dot with this name (`.$name`)
    /// under the root maildir.
    pub name: String,

    /// Set of strings to scan the body for. If it matches, the email is moved to this folder.
    ///
    /// # Examples
    ///
    /// ```toml
    /// keywords = ["diff --git a/rust/kernel/lib.rs"]
    /// ```
    #[serde(default)]
    pub keywords: HashSet<Keyword>,

    /// Priority of this folder compared to other folders.
    ///
    /// Higher priority folders will be preferred if their `match-body` value matches.
    pub priority: usize,

    /// Mark all emails delivered to this folder as read.
    ///
    /// Useful for a "muted" or "not interested" folder.
    #[serde(rename = "mark-read", default)]
    pub mark_read: bool,

    /// Set of keywords used to mark mails with the `Flagged` flag.
    ///
    /// If this is set, it overrides the global [`flagging.keywords`](Flagging::keywords) configuration option.
    #[serde(rename = "flagging-keywords")]
    pub flagging_keywords: Option<HashSet<Keyword>>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Quirks {
    /// Set of `List-Id`'s used for special deduplication logic.
    ///
    /// Some mailing lists append a footer or prepend the subject line. This results in `lei q` not
    /// detecting that they are the same message via the `content` dedupe strategy (since the
    /// content isn't the same). For this reason, we employ additional dedupliaction logic that
    /// removes any duplicate emails that have a `List-Id` header with a value from this set.
    ///
    /// # Examples
    ///
    /// ```toml
    /// deduplicate = ["<linux-riscv.lists.infradead.org>"]
    /// ```
    pub deduplicate: HashSet<String>,

    /// Set of preferred `Message-ID`s.
    ///
    /// Some mail clients opt to send emails with multiple `Message-ID`s. In the case when the
    /// first `Message-ID`s is not unique, but a subsequent one is, you can specify the unique one
    /// in this list and we will prefer that `Message-ID` over the first one.
    pub prefer: HashSet<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Client {
    /// Mail-client command with arguments.
    pub command: Vec<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Git {
    /// Should `git push` be run if any new commits have been created?
    #[serde(default)]
    pub push: bool,

    /// Should `git pull` be run before updating the mails?
    #[serde(default)]
    pub pull: bool,
}

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct Flagging {
    /// Set of keywords to scan for and add the `Flagged` flag.
    pub keywords: HashSet<Keyword>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Ignore {
    /// Your name (or name + email) used to detect direct mentions in `CC` or `TO`.
    pub name: String,

    /// Set of `List-Id`'s to ignore emails from that aren't direct mentions.
    //
    /// Depending on the query, `lei q` might pick up messages from lists that aren't of interest
    /// to you. In this case, you can list them here and you won't receive emails from this list,
    /// except when they trigger your keywords or are sent directly to you.
    ///
    /// # Examples
    ///
    /// ```toml
    /// lists = ["<qemu-devel.nongnu.org>"]
    /// ```
    pub lists: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct Keyword(Regex);

impl Keyword {
    pub fn matches(&self, text: &str) -> bool {
        self.0.is_match(text)
    }
}

impl PartialEq for Keyword {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl Eq for Keyword {}

impl Hash for Keyword {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state)
    }
}

impl<'de> Deserialize<'de> for Keyword {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let regex = String::deserialize(deserializer)?;
        Regex::try_from(regex)
            .map(Keyword)
            .map_err(<D::Error as serde::de::Error>::custom)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("unable to locate user's home directory, is `$HOME` set?")]
    NoHome,
    #[error("failed to read config file `{1}`: {0}")]
    Read(io::Error, PathBuf),
    #[error("failed to parse config file `{1}`: {0}")]
    Parse(toml::de::Error, PathBuf),
}

pub fn load() -> Result<Config, Error> {
    let path = BaseDirs::new()
        .ok_or(Error::NoHome)?
        .config_dir()
        .join("lkml")
        .join("config.toml");
    let cfg = fs::read_to_string(&path).map_err(|e| Error::Read(e, path.clone()))?;
    toml::from_str(&cfg).map_err(|e| Error::Parse(e, path.clone()))
}
