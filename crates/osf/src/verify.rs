//! What `osf check <name>` needs to run one check, shared with `osf verify`
//! at its call site in `main.rs`. The runner that used to live here moved
//! to `checkpoint.rs`: an in-process fallback is not allowed, so every
//! caller now reaches a check through moon.

use crate::config::Config;
use crate::exclude::Excluder;
use std::path::Path;

/// What `osf verify` needs to run: where the repository lives, the base to
/// diff against, an optional commit message file, the resolved config, the
/// exclude list already built into a matcher, and the checkpoint calling.
pub struct Options<'a> {
    pub dir: &'a Path,
    pub base: Option<String>,
    pub message_file: Option<&'a Path>,
    pub config: &'a Config,
    pub excluder: &'a Excluder,
    pub checkpoint: crate::checkpoint::Checkpoint,
}
