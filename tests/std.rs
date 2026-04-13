use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

use io_maildir::{
    coroutines::{
        flags_add::{MaildirFlagsAdd, MaildirFlagsAddArg, MaildirFlagsAddResult},
        flags_remove::{MaildirFlagsRemove, MaildirFlagsRemoveArg, MaildirFlagsRemoveResult},
        flags_set::{MaildirFlagsSet, MaildirFlagsSetArg, MaildirFlagsSetResult},
        maildir_create::{MaildirCreate, MaildirCreateArg, MaildirCreateResult},
        maildir_delete::{MaildirDelete, MaildirDeleteArg, MaildirDeleteResult},
        maildir_list::{MaildirList, MaildirListArg, MaildirListResult},
        maildir_rename::{MaildirRename, MaildirRenameArg, MaildirRenameResult},
        message_copy::{MaildirMessageCopy, MaildirMessageCopyArg, MaildirMessageCopyResult},
        message_get::{MaildirMessageGet, MaildirMessageGetArg, MaildirMessageGetResult},
        message_list::{MaildirMessagesList, MaildirMessagesListArg, MaildirMessagesListResult},
        message_move::{MaildirMessageMove, MaildirMessageMoveArg, MaildirMessageMoveResult},
        message_store::{MaildirMessageStore, MaildirMessageStoreArg, MaildirMessageStoreResult},
    },
    flag::{Flag, Flags},
    maildir::{Maildir, MaildirSubdir},
};
use tempfile::tempdir;

fn dir_create<I: IntoIterator<Item = String>>(paths: I) {
    for path in paths {
        fs::create_dir(&path).unwrap();
    }
}

fn dir_remove<I: IntoIterator<Item = String>>(paths: I) {
    for path in paths {
        fs::remove_dir_all(&path).unwrap();
    }
}

fn dir_read<I: IntoIterator<Item = String>>(paths: I) -> BTreeMap<String, BTreeSet<String>> {
    let mut entries = BTreeMap::new();

    for path in paths {
        let mut children = BTreeSet::new();

        for entry in fs::read_dir(&path).unwrap() {
            let entry = entry.unwrap();
            children.insert(entry.path().to_string_lossy().into_owned());
        }

        entries.insert(path, children);
    }

    entries
}

fn file_create(files: BTreeMap<String, Vec<u8>>) {
    for (path, contents) in files {
        let mut f = File::create(&path).unwrap();
        f.write_all(&contents).unwrap();
    }
}

fn file_read<I: IntoIterator<Item = String>>(paths: I) -> BTreeMap<String, Vec<u8>> {
    let mut contents = BTreeMap::new();
    for path in paths {
        let data = fs::read(&path).unwrap();
        contents.insert(path, data);
    }
    contents
}

fn rename<I: IntoIterator<Item = (String, String)>>(pairs: I) {
    for (from, to) in pairs {
        fs::rename(&from, &to).unwrap();
    }
}

fn copy<I: IntoIterator<Item = (String, String)>>(pairs: I) {
    for (from, to) in pairs {
        fs::copy(&from, &to).unwrap();
    }
}

fn create_maildir(root: PathBuf) -> Maildir {
    let mut arg: Option<MaildirCreateArg> = None;
    let mut coroutine = MaildirCreate::new(root.clone());

    loop {
        match coroutine.resume(arg.take()) {
            MaildirCreateResult::Ok => break,
            MaildirCreateResult::WantsDirCreate(paths) => {
                dir_create(paths);
                arg = Some(MaildirCreateArg::DirCreate);
            }
            MaildirCreateResult::Err(err) => panic!("{err}"),
        }
    }

    Maildir::try_from(root).unwrap()
}

#[test]
fn std() {
    let _ = env_logger::try_init();

    let workdir = tempdir().unwrap();
    let root = workdir.path();

    // should list zero maildirs in empty root

    let mut arg: Option<MaildirListArg> = None;
    let mut coroutine = MaildirList::new(root.to_path_buf());

    let maildirs = loop {
        match coroutine.resume(arg.take()) {
            MaildirListResult::Ok(m) => break m,
            MaildirListResult::WantsDirRead(paths) => {
                arg = Some(MaildirListArg::DirRead(dir_read(paths)));
            }
            MaildirListResult::Err(err) => panic!("{err}"),
        }
    };

    assert!(maildirs.is_empty());

    // should create maildirs

    let inbox = create_maildir(root.join("inbox"));
    let drafts = create_maildir(root.join("drafts"));

    assert!(root.join("inbox").join("cur").is_dir());
    assert!(root.join("inbox").join("new").is_dir());
    assert!(root.join("inbox").join("tmp").is_dir());

    // should list two maildirs

    let mut arg: Option<MaildirListArg> = None;
    let mut coroutine = MaildirList::new(root.to_path_buf());

    let maildirs = loop {
        match coroutine.resume(arg.take()) {
            MaildirListResult::Ok(m) => break m,
            MaildirListResult::WantsDirRead(paths) => {
                arg = Some(MaildirListArg::DirRead(dir_read(paths)));
            }
            MaildirListResult::Err(err) => panic!("{err}"),
        }
    };

    assert_eq!(maildirs.len(), 2);

    // should store a message in /new

    let msg = b"From: alice@example.com\r\nSubject: Test\r\n\r\nBody\r\n".to_vec();

    let mut arg: Option<MaildirMessageStoreArg> = None;
    let mut coroutine =
        MaildirMessageStore::new(inbox.clone(), MaildirSubdir::New, Flags::default(), msg);

    let (id, msg_path) = loop {
        match coroutine.resume(arg.take()) {
            MaildirMessageStoreResult::Ok { id, path } => break (id, path),
            MaildirMessageStoreResult::WantsFileCreate(files) => {
                file_create(files);
                arg = Some(MaildirMessageStoreArg::FileCreate);
            }
            MaildirMessageStoreResult::WantsRename(pairs) => {
                rename(pairs);
                arg = Some(MaildirMessageStoreArg::Rename);
            }
            MaildirMessageStoreResult::Err(err) => panic!("{err}"),
        }
    };

    assert!(msg_path.is_file());
    assert!(msg_path.starts_with(inbox.new()));

    // should list messages

    let mut arg: Option<MaildirMessagesListArg> = None;
    let mut coroutine = MaildirMessagesList::new(inbox.clone());

    let messages = loop {
        match coroutine.resume(arg.take()) {
            MaildirMessagesListResult::Ok(m) => break m,
            MaildirMessagesListResult::WantsDirRead(paths) => {
                arg = Some(MaildirMessagesListArg::DirRead(dir_read(paths)));
            }
            MaildirMessagesListResult::WantsFileRead(paths) => {
                arg = Some(MaildirMessagesListArg::FileRead(file_read(paths)));
            }
            MaildirMessagesListResult::Err(err) => panic!("{err}"),
        }
    };

    assert_eq!(messages.len(), 1);

    // should get the message

    let mut arg: Option<MaildirMessageGetArg> = None;
    let mut coroutine = MaildirMessageGet::new(inbox.clone(), &id);

    let message = loop {
        match coroutine.resume(arg.take()) {
            MaildirMessageGetResult::Ok(m) => break m,
            MaildirMessageGetResult::WantsDirRead(paths) => {
                arg = Some(MaildirMessageGetArg::DirRead(dir_read(paths)));
            }
            MaildirMessageGetResult::WantsFileRead(paths) => {
                arg = Some(MaildirMessageGetArg::FileRead(file_read(paths)));
            }
            MaildirMessageGetResult::Err(err) => panic!("{err}"),
        }
    };

    assert_eq!(message.id(), Some(id.as_str()));

    // should set flags (message now lives in /new, flags are a no-op there)

    let mut arg: Option<MaildirFlagsSetArg> = None;
    let flags_seen = Flags::from_iter([Flag::Seen]);
    let mut coroutine = MaildirFlagsSet::new(inbox.clone(), &id, flags_seen);

    loop {
        match coroutine.resume(arg.take()) {
            MaildirFlagsSetResult::Ok => break,
            MaildirFlagsSetResult::WantsDirRead(paths) => {
                arg = Some(MaildirFlagsSetArg::DirRead(dir_read(paths)));
            }
            MaildirFlagsSetResult::WantsRename(pairs) => {
                rename(pairs);
                arg = Some(MaildirFlagsSetArg::Rename);
            }
            MaildirFlagsSetResult::Err(err) => panic!("{err}"),
        }
    }

    // should add flags (no-op for /new messages)

    let mut arg: Option<MaildirFlagsAddArg> = None;
    let flags_flagged = Flags::from_iter([Flag::Flagged]);
    let mut coroutine = MaildirFlagsAdd::new(inbox.clone(), &id, flags_flagged);

    loop {
        match coroutine.resume(arg.take()) {
            MaildirFlagsAddResult::Ok => break,
            MaildirFlagsAddResult::WantsDirRead(paths) => {
                arg = Some(MaildirFlagsAddArg::DirRead(dir_read(paths)));
            }
            MaildirFlagsAddResult::WantsRename(pairs) => {
                rename(pairs);
                arg = Some(MaildirFlagsAddArg::Rename);
            }
            MaildirFlagsAddResult::Err(err) => panic!("{err}"),
        }
    }

    // should remove flags (no-op for /new messages)

    let mut arg: Option<MaildirFlagsRemoveArg> = None;
    let flags_seen2 = Flags::from_iter([Flag::Seen]);
    let mut coroutine = MaildirFlagsRemove::new(inbox.clone(), &id, flags_seen2);

    loop {
        match coroutine.resume(arg.take()) {
            MaildirFlagsRemoveResult::Ok => break,
            MaildirFlagsRemoveResult::WantsDirRead(paths) => {
                arg = Some(MaildirFlagsRemoveArg::DirRead(dir_read(paths)));
            }
            MaildirFlagsRemoveResult::WantsRename(pairs) => {
                rename(pairs);
                arg = Some(MaildirFlagsRemoveArg::Rename);
            }
            MaildirFlagsRemoveResult::Err(err) => panic!("{err}"),
        }
    }

    // should copy message to drafts

    let mut arg: Option<MaildirMessageCopyArg> = None;
    let mut coroutine =
        MaildirMessageCopy::new(&id, inbox.clone(), drafts.clone(), Some(MaildirSubdir::New));

    loop {
        match coroutine.resume(arg.take()) {
            MaildirMessageCopyResult::Ok => break,
            MaildirMessageCopyResult::WantsDirRead(paths) => {
                arg = Some(MaildirMessageCopyArg::DirRead(dir_read(paths)));
            }
            MaildirMessageCopyResult::WantsCopy(pairs) => {
                copy(pairs);
                arg = Some(MaildirMessageCopyArg::Copy);
            }
            MaildirMessageCopyResult::Err(err) => panic!("{err}"),
        }
    }

    // inbox still has one message after copy
    let inbox_count = message_count(inbox.clone());
    assert_eq!(inbox_count, 1);

    // drafts now has one message
    let drafts_count = message_count(drafts.clone());
    assert_eq!(drafts_count, 1);

    // should move message from inbox to drafts

    let mut arg: Option<MaildirMessageMoveArg> = None;
    let mut coroutine =
        MaildirMessageMove::new(&id, inbox.clone(), drafts.clone(), Some(MaildirSubdir::New));

    loop {
        match coroutine.resume(arg.take()) {
            MaildirMessageMoveResult::Ok => break,
            MaildirMessageMoveResult::WantsDirRead(paths) => {
                arg = Some(MaildirMessageMoveArg::DirRead(dir_read(paths)));
            }
            MaildirMessageMoveResult::WantsRename(pairs) => {
                rename(pairs);
                arg = Some(MaildirMessageMoveArg::Rename);
            }
            MaildirMessageMoveResult::Err(err) => panic!("{err}"),
        }
    }

    // inbox should now be empty
    assert_eq!(message_count(inbox.clone()), 0);

    // should rename maildir

    let mut arg: Option<MaildirRenameArg> = None;
    let mut coroutine = MaildirRename::new(drafts.as_ref().to_path_buf(), "archive");

    loop {
        match coroutine.resume(arg.take()) {
            MaildirRenameResult::Ok => break,
            MaildirRenameResult::WantsRename(pairs) => {
                rename(pairs);
                arg = Some(MaildirRenameArg::Rename);
            }
            MaildirRenameResult::Err(err) => panic!("{err}"),
        }
    }

    assert!(!root.join("drafts").is_dir());
    assert!(root.join("archive").is_dir());

    // should delete maildirs

    for name in ["inbox", "archive"] {
        let mut arg: Option<MaildirDeleteArg> = None;
        let mut coroutine = MaildirDelete::new(root.join(name));

        loop {
            match coroutine.resume(arg.take()) {
                MaildirDeleteResult::Ok => break,
                MaildirDeleteResult::WantsDirRemove(paths) => {
                    dir_remove(paths);
                    arg = Some(MaildirDeleteArg::DirRemove);
                }
                MaildirDeleteResult::Err(err) => panic!("{err}"),
            }
        }
    }

    let mut arg: Option<MaildirListArg> = None;
    let mut coroutine = MaildirList::new(root.to_path_buf());

    let maildirs = loop {
        match coroutine.resume(arg.take()) {
            MaildirListResult::Ok(m) => break m,
            MaildirListResult::WantsDirRead(paths) => {
                arg = Some(MaildirListArg::DirRead(dir_read(paths)));
            }
            MaildirListResult::Err(err) => panic!("{err}"),
        }
    };

    assert!(maildirs.is_empty());
}

fn message_count(maildir: Maildir) -> usize {
    let mut arg: Option<MaildirMessagesListArg> = None;
    let mut c = MaildirMessagesList::new(maildir);

    loop {
        match c.resume(arg.take()) {
            MaildirMessagesListResult::Ok(m) => return m.len(),
            MaildirMessagesListResult::WantsDirRead(paths) => {
                arg = Some(MaildirMessagesListArg::DirRead(dir_read(paths)));
            }
            MaildirMessagesListResult::WantsFileRead(paths) => {
                arg = Some(MaildirMessagesListArg::FileRead(file_read(paths)));
            }
            MaildirMessagesListResult::Err(err) => panic!("{err}"),
        }
    }
}
