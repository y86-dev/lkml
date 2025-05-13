use std::{collections::HashMap, io, rc::Rc};

use maildir::{MailEntry, MailEntryError, Maildir};
use mailparse::{MailHeaderMap, MailParseError};
use tempdir::TempDir;
use thiserror::Error;
use tracing::{error, info, trace};

use crate::{
    assort::{
        folder::{Action, Dest, Folder},
        mail::{Mail, Type},
    },
    config::Config,
};

mod folder;
mod mail;

#[derive(Debug, Error)]
pub enum Error {
    #[error("While trying to read a mail file from disk: {0}")]
    MailIO(io::Error),
    #[error("while trying to modify the filesystem: {0}")]
    Fs(io::Error),
    #[error("internal error")]
    Internal,
    #[error("While trying to parse a mail: {0}")]
    Mail(#[from] MailEntryError),
    #[error("{0}")]
    Mail2(#[from] mail::Error),
    #[error("TODO: {0}")]
    Mail3(#[from] MailParseError),
}

pub fn run(new_dir: TempDir, main: Maildir, cfg: &Config) -> Result<(), Error> {
    let new = Maildir::from(new_dir.path().to_owned());
    let Collected {
        folders,
        mut mails,
        new_count,
        rest,
    } = collect_mails(new, main, cfg)?;
    let Indexed {
        indexed,
        new,
        mut actions,
    } = index(new_count, &mut mails, cfg)?;
    for new in &new {
        assort(new, &indexed, &mut actions, &folders, cfg, rest)?;
    }
    fixup_thread_siblings(&new, &indexed, &mut actions, &folders, cfg)?;
    perform(actions, &folders)?;
    // keep it alive until at least here.
    drop(new_dir);
    Ok(())
}

struct Collected {
    folders: Vec<Folder>,
    mails: Vec<(MailEntry, Type)>,
    new_count: usize,
    rest: usize,
}

fn collect_mails(new: Maildir, main: Maildir, cfg: &Config) -> Result<Collected, Error> {
    let mut folders = cfg
        .folders
        .iter()
        .map(|f| Folder::new(f, main.path()))
        .collect::<Vec<_>>();
    folders.sort_by_key(|f| std::cmp::Reverse(f.priority));
    let rest = folders
        .iter()
        .position(|f| f.name == "INBOX")
        .unwrap_or_else(|| {
            folders.push(Folder::rest(main));
            folders.len() - 1
        });
    for folder in &folders {
        folder.maildir.create_dirs().map_err(Error::Fs)?;
    }
    let newmail = Maildir::from(new.path().to_owned());
    let mut mails = folders
        .iter()
        .enumerate()
        .flat_map(|(i, f)| {
            f.maildir
                .list_new()
                .chain(f.maildir.list_cur())
                .map(move |m| (m, i))
        })
        .map(|(m, i)| Ok::<_, Error>((m.map_err(Error::MailIO)?, Type::Folder(i))))
        .collect::<Result<Vec<_>, _>>()?;
    let mut dupe = Vec::with_capacity(100);
    let mut new_count = 0;
    for mail in newmail.list_cur().chain(newmail.list_new()) {
        new_count += 1;
        let mut mail = mail.map_err(Error::MailIO)?;
        if mail
            .headers()?
            .get_all_values("list-id")
            .iter()
            .any(|id| cfg.quirks.deduplicate.contains(id))
        {
            dupe.push((mail, Type::New));
        } else {
            mails.push((mail, Type::New));
        }
    }
    mails.extend(dupe);
    Ok(Collected {
        folders,
        mails,
        new_count,
        rest,
    })
}

struct Indexed<'a> {
    indexed: HashMap<String, Vec<Rc<Mail<'a>>>>,
    new: Vec<Rc<Mail<'a>>>,
    actions: HashMap<Rc<Mail<'a>>, Action>,
}

fn index<'a>(
    new_count: usize,
    mails: &'a mut [(MailEntry, Type)],
    cfg: &Config,
) -> Result<Indexed<'a>, Error> {
    let mut indexed: HashMap<String, Vec<Rc<Mail<'a>>>> = HashMap::with_capacity(mails.len());
    let mut new = Vec::with_capacity(new_count);
    let mut error = false;
    let mut actions = HashMap::with_capacity(new_count);
    for (mail, typ) in mails {
        let mail = Rc::new(mail::parse(mail, *typ, cfg)?);
        let mails = indexed.entry(mail.id.clone()).or_default();
        if !mails.is_empty() && mail.typ == Type::New {
            if mail
                .parsed
                .headers
                .get_all_values("list-id")
                .iter()
                .any(|id| cfg.quirks.deduplicate.contains(id))
            {
                trace!("dropping {} because of duplicate & wrong list", mail.id);
                actions.insert(mail.clone(), Action::delete());
            } else if mails
                .iter()
                .all(|m| mail.parsed.raw_bytes == m.parsed.raw_bytes)
                || mails
                    .iter()
                    .map(|m| Ok(mail.parsed.get_body()? == m.parsed.get_body()?))
                    .reduce(|a: Result<bool, Error>, b| Ok(a? || b?))
                    .unwrap()?
            {
                trace!("dropping verbatim copy {}", mail.id);
                actions.insert(mail.clone(), Action::delete());
            } else {
                error!(
                    "new email received with same id as existing, pls implement!\n{:#?} vs\n{}\n\n {:#?}",
                    mails.iter().map(|m| m.path.display()).collect::<Vec<_>>(),
                    mail.path.display(),
                    mail.parsed.headers.get_all_values("list-id")
                );
                error = true
            }
        } else if mails.iter().any(|m| m.typ != mail.typ) {
            error!(
                "duplicate mails aren't stored in the same directory! {:?}",
                mails
                    .iter()
                    .chain(std::iter::once(&mail))
                    .map(|m| (m.path.display(), m.typ))
                    .collect::<Vec<_>>()
            );
            error = true;
        }
        if *typ == Type::New {
            new.push(mail.clone());
        }
        trace!("{}", mail.id);
        mails.push(mail);
    }
    if error {
        eprintln!("An error occurred with duplicate emails above. If you report the error,");
        eprintln!("please include the offending email files.");
        eprintln!();
        eprintln!("Press enter to terminate the program & delete the temporary directory.");
        io::stdin()
            .read_line(&mut String::new())
            .expect("failed to read from stdin");
        return Err(Error::Internal);
    }
    Ok(Indexed {
        indexed,
        new,
        actions,
    })
}

fn assort<'a>(
    new: &Rc<Mail<'a>>,
    indexed: &HashMap<String, Vec<Rc<Mail<'a>>>>,
    actions: &mut HashMap<Rc<Mail<'a>>, Action>,
    folders: &[Folder],
    cfg: &Config,
    rest: usize,
) -> Result<Action, Error> {
    if let Some(action) = actions.get(new) {
        return Ok(*action);
    }
    let mut parent_is_new = false;
    let mut action = None;
    if let Some(parent) = new.parent.as_ref() {
        if let Some(parents) = indexed.get(parent) {
            let parent = &parents[0];
            match &parent.typ {
                Type::New => {
                    parent_is_new = true;
                    let parent_action =
                        actions.get(parent).copied().map(Ok).unwrap_or_else(|| {
                            assort(parent, indexed, actions, folders, cfg, rest)
                        })?;
                    action = Some(parent_action.with_cleared_flags());
                }
                Type::Folder(id) => {
                    action = Some(Action::folder(*id));
                }
            }
        }
    }
    let body = new.parsed.get_body()?;
    if action.is_none() || parent_is_new {
        let folders = if let Some(action) = action {
            folders
                .iter()
                .enumerate()
                .take(action.folder_idx().unwrap_or(0))
        } else {
            folders.iter().enumerate().take(folders.len())
        };
        for (i, folder) in folders {
            if folder.keywords.iter().any(|kw| kw.matches(&body)) {
                action = Some(Action::folder(i));
                break;
            }
        }
    }
    let mut action = action.unwrap_or_else(|| Action::folder(rest));

    if action.folder_idx() == Some(rest)
        && !action.is_flagged()
        && cfg
            .ignore
            .as_ref()
            .map(|ignore| {
                new.parsed
                    .headers
                    .get_all_values("List-Id")
                    .iter()
                    .any(|id| ignore.lists.contains(id))
                    && !new
                        .parsed
                        .headers
                        .get_all_values("to")
                        .iter()
                        .chain(new.parsed.headers.get_all_values("cc").iter())
                        .any(|recip| recip.contains(&ignore.name))
            })
            .unwrap_or(false)
    {
        action = Action::delete();
    }

    compute_flags(new, &mut action, folders, cfg)?;

    if new
        .parsed
        .headers
        .get_all_values("from")
        .iter()
        .any(|f| cfg.addresses.iter().any(|addr| f.contains(addr)))
    {
        action.read();
    }
    actions.insert(new.clone(), action);
    Ok(action)
}

fn compute_flags<'a>(
    mail: &Rc<Mail<'a>>,
    action: &mut Action,
    folders: &[Folder],
    cfg: &Config,
) -> Result<(), Error> {
    match action.dest() {
        Dest::Drop => {}
        Dest::Folder(i) => {
            let body = mail.parsed.get_body()?;
            if folders[i].mark_read {
                action.read();
            }
            if let Some(fkws) = &folders[i].flagging_keywords {
                if fkws.iter().any(|kw| kw.matches(&body)) {
                    action.flag();
                }
            } else if cfg.flagging.keywords.iter().any(|kw| kw.matches(&body)) {
                action.flag();
            }
        }
    }
    Ok(())
}

fn fixup_thread_siblings<'a>(
    new: &[Rc<Mail<'a>>],
    indexed: &HashMap<String, Vec<Rc<Mail<'a>>>>,
    actions: &mut HashMap<Rc<Mail<'a>>, Action>,
    folders: &[Folder],
    cfg: &Config,
) -> Result<(), Error> {
    let mut error = false;
    let mut changed = true;
    while changed {
        changed = false;
        for new in new {
            if let Some(parent) = &new.parent {
                if let Some(parents) = indexed.get(parent) {
                    for parent in parents {
                        if parent.typ == Type::New {
                            let ours = actions[new].dest();
                            let theirs = actions[parent].dest();
                            if ours != theirs {
                                if let Some(dest) = Dest::max_prio(ours, theirs) {
                                    let ours = actions.get_mut(new).unwrap();
                                    ours.set_dest(dest);
                                    compute_flags(new, ours, folders, cfg)?;
                                    let theirs = actions.get_mut(parent).unwrap();
                                    theirs.set_dest(dest);
                                    compute_flags(new, theirs, folders, cfg)?;
                                    changed = true;
                                }
                            }
                        } else {
                            let action = actions[new];
                            if let Some(typ) = Option::<Type>::from(action.dest()) {
                                if typ != parent.typ {
                                    error = true;
                                    error!(
                                        "moved into wrong folder with parent!\n\t{} ({:?})\n\t{} -> {:?}",
                                        parent.path.display(),
                                        parent.typ,
                                        new.path.display(),
                                        action
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if error {
        eprintln!("An error occurred with wanting to move emails into separate folders above.");
        eprintln!("If you report the error, please include the offending email files.");
        eprintln!();
        eprintln!("Press enter to terminate the program & delete the temporary directory.");
        io::stdin()
            .read_line(&mut String::new())
            .expect("failed to read from stdin");
        return Err(Error::Internal);
    }
    Ok(())
}

fn perform<'a>(actions: HashMap<Rc<Mail<'a>>, Action>, folders: &[Folder]) -> Result<(), Error> {
    for (mail, action) in actions {
        let id = &mail.maildir_id;
        let flags = action.flags();
        let dest = match action.dest() {
            Dest::Drop => {
                std::fs::remove_file(&mail.path).map_err(Error::Fs)?;
                info!("deleting `{id}`");
                continue;
            }
            Dest::Folder(idx) => &folders[idx].maildir,
        };
        let src = &mail.path;
        #[cfg(unix)]
        const INFORMATIONAL_SUFFIX_SEPARATOR: &str = ":";
        #[cfg(windows)]
        const INFORMATIONAL_SUFFIX_SEPARATOR: &str = ";";
        let dst = dest
            .path()
            .join("cur")
            .join(format!("{id}{INFORMATIONAL_SUFFIX_SEPARATOR}2,{flags}"));
        info!(
            "moving `{id}` to {} ({flags}) [{} -> {}]",
            dest.path().display(),
            src.display(),
            dst.display()
        );
        std::fs::copy(src, dst).map_err(Error::Fs)?;
        std::fs::remove_file(src).map_err(Error::Fs)?;
    }
    Ok(())
}
