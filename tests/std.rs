use std::path::PathBuf;

use io_fs::runtimes::std::handle;
use io_maildir::{
    coroutines::{
        flags_add::{MaildirFlagsAdd, MaildirFlagsAddResult},
        flags_remove::{MaildirFlagsRemove, MaildirFlagsRemoveResult},
        flags_set::{MaildirFlagsSet, MaildirFlagsSetResult},
        maildir_create::{MaildirCreate, MaildirCreateResult},
        maildir_delete::{MaildirDelete, MaildirDeleteResult},
        maildir_list::{MaildirList, MaildirListResult},
        maildir_rename::{MaildirRename, MaildirRenameResult},
        message_copy::{MaildirMessageCopy, MaildirMessageCopyResult},
        message_get::{MaildirMessageGet, MaildirMessageGetResult},
        message_list::{MaildirMessagesList, MaildirMessagesListResult},
        message_move::{MaildirMessageMove, MaildirMessageMoveResult},
        message_store::{MaildirMessageStore, MaildirMessageStoreResult},
    },
    flag::{Flag, Flags},
    maildir::{Maildir, MaildirSubdir},
};
use tempfile::tempdir;

fn create_maildir(root: PathBuf) -> Maildir {
    let mut arg = None;
    let mut coroutine = MaildirCreate::new(root.clone());

    loop {
        match coroutine.resume(arg.take()) {
            MaildirCreateResult::Ok => break,
            MaildirCreateResult::Io(input) => arg = Some(handle(input).unwrap()),
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

    let mut arg = None;
    let mut coroutine = MaildirList::new(root.to_path_buf());

    let maildirs = loop {
        match coroutine.resume(arg.take()) {
            MaildirListResult::Ok(m) => break m,
            MaildirListResult::Io(input) => arg = Some(handle(input).unwrap()),
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

    let mut arg = None;
    let mut coroutine = MaildirList::new(root.to_path_buf());

    let maildirs = loop {
        match coroutine.resume(arg.take()) {
            MaildirListResult::Ok(m) => break m,
            MaildirListResult::Io(input) => arg = Some(handle(input).unwrap()),
            MaildirListResult::Err(err) => panic!("{err}"),
        }
    };

    assert_eq!(maildirs.len(), 2);

    // should store a message in /new

    let msg = b"From: alice@example.com\r\nSubject: Test\r\n\r\nBody\r\n".to_vec();

    let mut arg = None;
    let mut coroutine =
        MaildirMessageStore::new(inbox.clone(), MaildirSubdir::New, Flags::default(), msg);

    let (id, msg_path) = loop {
        match coroutine.resume(arg.take()) {
            MaildirMessageStoreResult::Ok { id, path } => break (id, path),
            MaildirMessageStoreResult::Io(input) => arg = Some(handle(input).unwrap()),
            MaildirMessageStoreResult::Err(err) => panic!("{err}"),
        }
    };

    assert!(msg_path.is_file());
    assert!(msg_path.starts_with(inbox.new()));

    // should list messages

    let mut arg = None;
    let mut coroutine = MaildirMessagesList::new(inbox.clone());

    let messages = loop {
        match coroutine.resume(arg.take()) {
            MaildirMessagesListResult::Ok(m) => break m,
            MaildirMessagesListResult::Io(input) => arg = Some(handle(input).unwrap()),
            MaildirMessagesListResult::Err(err) => panic!("{err}"),
        }
    };

    assert_eq!(messages.len(), 1);

    // should get the message

    let mut arg = None;
    let mut coroutine = MaildirMessageGet::new(inbox.clone(), &id);

    let message = loop {
        match coroutine.resume(arg.take()) {
            MaildirMessageGetResult::Ok(m) => break m,
            MaildirMessageGetResult::Io(input) => arg = Some(handle(input).unwrap()),
            MaildirMessageGetResult::Err(err) => panic!("{err}"),
        }
    };

    assert_eq!(message.id(), Some(id.as_str()));

    // should set flags (message now lives in /new, flags are a no-op there)

    let mut arg = None;
    let flags_seen = Flags::from_iter([Flag::Seen]);
    let mut coroutine = MaildirFlagsSet::new(inbox.clone(), &id, flags_seen);

    loop {
        match coroutine.resume(arg.take()) {
            MaildirFlagsSetResult::Ok => break,
            MaildirFlagsSetResult::Io(input) => arg = Some(handle(input).unwrap()),
            MaildirFlagsSetResult::Err(err) => panic!("{err}"),
        }
    }

    // should add flags (no-op for /new messages)

    let mut arg = None;
    let flags_flagged = Flags::from_iter([Flag::Flagged]);
    let mut coroutine = MaildirFlagsAdd::new(inbox.clone(), &id, flags_flagged);

    loop {
        match coroutine.resume(arg.take()) {
            MaildirFlagsAddResult::Ok => break,
            MaildirFlagsAddResult::Io(input) => arg = Some(handle(input).unwrap()),
            MaildirFlagsAddResult::Err(err) => panic!("{err}"),
        }
    }

    // should remove flags (no-op for /new messages)

    let mut arg = None;
    let flags_seen2 = Flags::from_iter([Flag::Seen]);
    let mut coroutine = MaildirFlagsRemove::new(inbox.clone(), &id, flags_seen2);

    loop {
        match coroutine.resume(arg.take()) {
            MaildirFlagsRemoveResult::Ok => break,
            MaildirFlagsRemoveResult::Io(input) => arg = Some(handle(input).unwrap()),
            MaildirFlagsRemoveResult::Err(err) => panic!("{err}"),
        }
    }

    // should copy message to drafts

    let mut arg = None;
    let mut coroutine =
        MaildirMessageCopy::new(&id, inbox.clone(), drafts.clone(), Some(MaildirSubdir::New));

    loop {
        match coroutine.resume(arg.take()) {
            MaildirMessageCopyResult::Ok => break,
            MaildirMessageCopyResult::Io(input) => arg = Some(handle(input).unwrap()),
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

    let mut arg = None;
    let mut coroutine =
        MaildirMessageMove::new(&id, inbox.clone(), drafts.clone(), Some(MaildirSubdir::New));

    loop {
        match coroutine.resume(arg.take()) {
            MaildirMessageMoveResult::Ok => break,
            MaildirMessageMoveResult::Io(input) => arg = Some(handle(input).unwrap()),
            MaildirMessageMoveResult::Err(err) => panic!("{err}"),
        }
    }

    // inbox should now be empty
    assert_eq!(message_count(inbox.clone()), 0);

    // should rename maildir

    let mut arg = None;
    let mut coroutine = MaildirRename::new(drafts.as_ref().to_path_buf(), "archive");

    loop {
        match coroutine.resume(arg.take()) {
            MaildirRenameResult::Ok => break,
            MaildirRenameResult::Io(input) => arg = Some(handle(input).unwrap()),
            MaildirRenameResult::Err(err) => panic!("{err}"),
        }
    }

    assert!(!root.join("drafts").is_dir());
    assert!(root.join("archive").is_dir());

    // should delete maildirs

    for name in ["inbox", "archive"] {
        let mut arg = None;
        let mut coroutine = MaildirDelete::new(root.join(name));

        loop {
            match coroutine.resume(arg.take()) {
                MaildirDeleteResult::Ok => break,
                MaildirDeleteResult::Io(input) => arg = Some(handle(input).unwrap()),
                MaildirDeleteResult::Err(err) => panic!("{err}"),
            }
        }
    }

    let mut arg = None;
    let mut coroutine = MaildirList::new(root.to_path_buf());

    let maildirs = loop {
        match coroutine.resume(arg.take()) {
            MaildirListResult::Ok(m) => break m,
            MaildirListResult::Io(input) => arg = Some(handle(input).unwrap()),
            MaildirListResult::Err(err) => panic!("{err}"),
        }
    };

    assert!(maildirs.is_empty());
}

fn message_count(maildir: Maildir) -> usize {
    let mut arg = None;
    let mut c = MaildirMessagesList::new(maildir);
    loop {
        match c.resume(arg.take()) {
            MaildirMessagesListResult::Ok(m) => return m.len(),
            MaildirMessagesListResult::Io(input) => arg = Some(handle(input).unwrap()),
            MaildirMessagesListResult::Err(err) => panic!("{err}"),
        }
    }
}
