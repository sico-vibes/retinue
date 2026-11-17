//! Test-only: a test's own data folder, and the daemon's environment quiet.
//!
//! Three "fails on the owner's machine" entries in
//! `.claude/skills/retinue-development/references/verification.md` had one
//! cause: a test inherited the environment of the running app, so the folder or
//! the hook it found was the owner's and not a fixture. A test that only sees
//! what it created cannot fail that way.
//!
//! `TestEnv` gives a test a temporary data root it removes itself (even when the
//! test panics) and removes the variables the daemon reads for its lifetime,
//! restoring their earlier values on drop. Removal, not a fixed value, is what
//! makes it safe beside other tests: every user of the helper wants the quiet
//! default, so two tests running at once agree.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The environment variables the daemon itself reads.
pub const DAEMON_ENV: [&str; 4] = [
    "RETINUE_DB_PATH",
    "RETINUE_DATA_DIR",
    "RETINUE_REMOTE_WEB_DIR",
    "RETINUE_HOOK_BRIDGE",
];

/// A fresh data folder for one test, with [`DAEMON_ENV`] removed.
pub struct TestEnv {
    root: PathBuf,
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl TestEnv {
    /// `name` only makes a failure easier to read; the root is unique.
    pub fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("retinue-test-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("a test data folder");
        Self { root, saved: quiet(&DAEMON_ENV) }
    }

    /// This test's data folder: anything the daemon writes belongs here.
    pub fn data_root(&self) -> &Path {
        &self.root
    }

    /// An in-memory state whose data folder is this test's own.
    pub fn state(&self) -> (crate::AppState, tokio::sync::broadcast::Receiver<crate::EventEnvelope>) {
        let (st, events) = crate::phase2a_tests::state();
        *st.team.data_dir.lock().unwrap() = Some(self.root.clone());
        (st, events)
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        restore(std::mem::take(&mut self.saved));
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Remove `names`, returning what they held so [`restore`] can put it back.
fn quiet(names: &[&'static str]) -> Vec<(&'static str, Option<OsString>)> {
    names
        .iter()
        .map(|name| {
            let saved = std::env::var_os(name);
            std::env::remove_var(name);
            (*name, saved)
        })
        .collect()
}

fn restore(saved: Vec<(&'static str, Option<OsString>)>) {
    for (name, value) in saved {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A variable nothing reads, so this test cannot disturb one that does.
    const PROBE: &str = "RETINUE_TEST_ENV_PROBE";

    #[test]
    fn the_environment_is_quiet_while_the_folder_lives_and_comes_back_after() {
        std::env::set_var(PROBE, "before");
        let saved = quiet(&[PROBE]);
        assert_eq!(std::env::var_os(PROBE), None, "the variable is gone for the test");
        restore(saved);
        assert_eq!(std::env::var_os(PROBE).as_deref(), Some(std::ffi::OsStr::new("before")), "the earlier value comes back");
        std::env::remove_var(PROBE);

        let env = TestEnv::new("probe");
        assert!(env.data_root().exists(), "the data folder is created");
        let folder = env.data_root().to_path_buf();
        drop(env);
        assert!(!folder.exists(), "the data folder is removed on drop, even when a test fails");
    }

    #[test]
    fn a_state_writes_into_the_tests_own_folder() {
        let env = TestEnv::new("state");
        let (st, _events) = env.state();
        let folder = st.team.data_dir.lock().unwrap().clone().expect("a data folder");
        assert_eq!(folder, env.data_root());
    }
}
