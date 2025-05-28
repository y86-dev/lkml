use std::{collections::HashSet, path::Path};

use maildir::Maildir;
use thiserror::Error;

use crate::{
    assort::mail::Type,
    config::{self, Keyword},
};

pub struct Folder {
    pub maildir: Maildir,
    pub priority: usize,
    pub keywords: HashSet<Keyword>,
    pub flagging_keywords: Option<HashSet<Keyword>>,
    pub name: String,
    pub mark_read: bool,
}

impl Folder {
    pub fn new(f: &config::Folder, parent: &Path) -> Self {
        let maildir = if f.name == "INBOX" {
            Maildir::from(parent.to_owned())
        } else {
            Maildir::from(parent.join(format!(".{}", f.name)))
        };
        Self {
            maildir,
            priority: f.priority,
            keywords: f.keywords.clone(),
            flagging_keywords: f.flagging_keywords.clone(),
            name: f.name.clone(),
            mark_read: f.mark_read,
        }
    }

    pub fn rest(maildir: Maildir) -> Self {
        Self {
            maildir,
            priority: usize::MAX,
            keywords: HashSet::new(),
            name: "INBOX".to_owned(),
            flagging_keywords: None,
            mark_read: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Action {
    mark_read: bool,
    mark_flagged: bool,
    dest: Dest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dest {
    Drop(DropReason),
    Folder(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    DuplicateQuirk,
    VerbatimCopy,
    Ignored,
}

impl Dest {
    pub fn max_prio(a: Self, b: Self) -> Option<Self> {
        match (a, b) {
            (Dest::Drop(_), _) | (_, Dest::Drop(_)) => None,
            (Dest::Folder(a), Dest::Folder(b)) => Some(Dest::Folder(a.min(b))),
        }
    }
}

#[derive(Debug, Error)]
#[error("cannot convert `Typ::New` into `Dest`")]
pub struct DestConvertError;

impl TryFrom<Type> for Dest {
    type Error = DestConvertError;
    fn try_from(value: Type) -> std::result::Result<Self, Self::Error> {
        Ok(match value {
            Type::New => return Err(DestConvertError),
            Type::Folder(id) => Dest::Folder(id),
        })
    }
}

impl From<Dest> for Option<Type> {
    fn from(value: Dest) -> Self {
        match value {
            Dest::Folder(id) => Some(Type::Folder(id)),
            Dest::Drop(_) => None,
        }
    }
}

impl Action {
    pub fn delete(reason: DropReason) -> Self {
        Self {
            dest: Dest::Drop(reason),
            mark_read: false,
            mark_flagged: false,
        }
    }

    pub fn folder(id: usize) -> Self {
        Self {
            dest: Dest::Folder(id),
            mark_read: false,
            mark_flagged: false,
        }
    }

    pub fn with_cleared_flags(&self) -> Self {
        let Self {
            mark_read: _,
            mark_flagged: _,
            dest,
        } = self;
        Self {
            dest: *dest,
            mark_read: false,
            mark_flagged: false,
        }
    }

    pub fn flags(&self) -> &'static str {
        match (self.mark_read, self.mark_flagged) {
            (true, true) => "FS",
            (true, false) => "S",
            (false, true) => "F",
            (false, false) => "",
        }
    }

    pub fn folder_idx(&self) -> Option<usize> {
        match self.dest {
            Dest::Drop(_) => None,
            Dest::Folder(id) => Some(id),
        }
    }

    pub fn flag(&mut self) {
        self.mark_flagged = true;
        self.mark_read = false;
    }

    pub fn read(&mut self) {
        self.mark_flagged = false;
        self.mark_read = true;
    }

    pub fn is_flagged(&self) -> bool {
        self.mark_flagged
    }

    pub fn dest(&self) -> Dest {
        self.dest
    }

    pub fn set_dest(&mut self, dest: Dest) {
        self.dest = dest;
    }
}
