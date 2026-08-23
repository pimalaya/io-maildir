---
cairn: log
change: coroutine-state-naming
landed: 2026-08-23
---

# Coroutine states are named after the action, not the wait

The fifteen private `State` enums of the crate lost their `Await` prefix. A state now names the action in flight: `ReadTime`, `ReadPid`, `ReadHostname`, `Copy`, `Rename`, `Create`, `CreateTmp`, `Remove`, `Write`, `Read`, `Probe`, `Scan`, beside the `Start` and `Locate` entry points that were already right. The `Display` phrases follow, "read time", "copy into tmp", "rename into place", "probe new and tmp", "scan cur", where they used to read "await time reply".

This aligns io-maildir with io-imap, the reference for coroutines across the Pimalaya libraries, and the rule is now naming-013 in the organisation guidelines rather than folklore.

No capability moved: the enums are private, no public item changed, and no behaviour changed. Recorded here because the crate's coroutines are read far more often than they are written, and a reader who knows io-imap should recognise them.

One thing this surfaced without touching it: none of the fifteen `Display` impls is called anywhere. io-imap uses its own through a `trace!` at the top of each resume loop, which is what the reference model and the logging guidelines ask for. io-maildir writes the impls and never traces them, so they are currently dead weight. Wiring the trace, or dropping the impls, is a decision for its own change.
