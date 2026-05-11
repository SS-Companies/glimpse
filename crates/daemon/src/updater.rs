//! Daily GitHub-Releases auto-update poll.
//!
//! On a dedicated background thread we:
//!
//! 1. Sleep 30 seconds after daemon startup (so a transient launch slow-down
//!    is never the user's fault).
//! 2. Loop forever:
//!    a. Parse owner/repo from `CARGO_PKG_REPOSITORY`.
//!    b. Ask GitHub for the latest published release tag via the `self_update`
//!       crate.
//!    c. Compare against `env!("CARGO_PKG_VERSION")`. If the release is
//!       strictly greater, store its version in [`AVAILABLE_VERSION`] and log.
//!    d. Sleep 24 hours.
//!
//! What we deliberately do **not** do here:
//! - Silently replace the running binary. The user always opts in.
//! - Send any telemetry. The single outbound request is to api.github.com.
//! - Crash the daemon on any networking / parsing failure: every error path
//!   is `tracing::warn!` and we go back to sleep.
//!
//! Wire-up
//! =======
//!
//! Future tray code can read [`available_version`] on every menu open and
//! conditionally append "Update available — vX.Y.Z" item. v1 just logs.

use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use glimpse_core::Config;

const STARTUP_DELAY: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_secs(60 * 60 * 24);
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const REPOSITORY_URL: &str = env!("CARGO_PKG_REPOSITORY");

/// Last-known newer release tag, e.g. `"0.2.0"`. `None` when we are up to
/// date or the check has not yet run.
static AVAILABLE_VERSION: OnceLock<Mutex<Option<String>>> = OnceLock::new();

/// Most recent newer version detected by the background poll.
pub fn available_version() -> Option<String> {
    AVAILABLE_VERSION
        .get()
        .and_then(|m| m.lock().ok().and_then(|g| g.clone()))
}

/// Spawn the update-poll thread. Honours `config.auto_update_check` — if
/// the user has disabled the check, this function returns immediately
/// without spawning anything.
pub fn spawn(config: &Config) {
    AVAILABLE_VERSION.get_or_init(|| Mutex::new(None));

    if !config.auto_update_check {
        tracing::info!("auto-update check disabled via config");
        return;
    }

    let (owner, repo) = match parse_owner_repo(REPOSITORY_URL) {
        Some(pair) => pair,
        None => {
            tracing::warn!(
                repository = %REPOSITORY_URL,
                "could not parse repository URL; auto-update disabled"
            );
            return;
        }
    };

    thread::Builder::new()
        .name("glimpse-updater".into())
        .spawn(move || run(owner, repo))
        .expect("spawn updater thread");
}

fn run(owner: String, repo: String) {
    tracing::info!(%owner, %repo, "auto-update poll thread started");
    thread::sleep(STARTUP_DELAY);

    loop {
        match check_once(&owner, &repo) {
            Ok(Some(newer)) => {
                tracing::info!(
                    current = CURRENT_VERSION,
                    available = %newer,
                    "newer version available on GitHub Releases"
                );
                if let Some(slot) = AVAILABLE_VERSION.get() {
                    *slot.lock().unwrap() = Some(newer);
                }
            }
            Ok(None) => {
                tracing::debug!("update check: up to date");
            }
            Err(e) => {
                tracing::warn!(error = ?e, "update check failed");
            }
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Returns `Ok(Some(tag))` if the latest release on GitHub has a strictly
/// greater version than what we are running. The `self_update` crate handles
/// rate limits and basic JSON parsing.
fn check_once(owner: &str, repo: &str) -> anyhow::Result<Option<String>> {
    let release = self_update::backends::github::Update::configure()
        .repo_owner(owner)
        .repo_name(repo)
        .bin_name("glimpse")
        .show_download_progress(false)
        .current_version(CURRENT_VERSION)
        .build()?
        .get_latest_release()?;

    // `release.version` does not include the leading `v` from the tag.
    let latest = release.version.trim_start_matches('v').to_string();

    if self_update::version::bump_is_greater(CURRENT_VERSION, &latest).unwrap_or(false) {
        Ok(Some(latest))
    } else {
        Ok(None)
    }
}

/// Parse `https://github.com/<owner>/<repo>` (with or without `.git` or trailing
/// slash) into `(owner, repo)`.
fn parse_owner_repo(url: &str) -> Option<(String, String)> {
    let trimmed = url.trim();
    let after_host = trimmed.split("github.com/").nth(1)?;
    let mut parts = after_host
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .splitn(2, '/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty() || repo.is_empty() || owner == "CHANGE_ME" {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https_url() {
        let (o, r) = parse_owner_repo("https://github.com/foo/bar").unwrap();
        assert_eq!(o, "foo");
        assert_eq!(r, "bar");
    }

    #[test]
    fn parses_with_dot_git() {
        let (o, r) = parse_owner_repo("https://github.com/foo/bar.git").unwrap();
        assert_eq!(o, "foo");
        assert_eq!(r, "bar");
    }

    #[test]
    fn parses_with_trailing_slash() {
        let (o, r) = parse_owner_repo("https://github.com/foo/bar/").unwrap();
        assert_eq!(o, "foo");
        assert_eq!(r, "bar");
    }

    #[test]
    fn rejects_placeholder() {
        assert!(parse_owner_repo("https://github.com/CHANGE_ME/glimpse").is_none());
    }

    #[test]
    fn rejects_non_github() {
        assert!(parse_owner_repo("https://gitlab.com/foo/bar").is_none());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_owner_repo("").is_none());
        assert!(parse_owner_repo("not-a-url").is_none());
    }
}
