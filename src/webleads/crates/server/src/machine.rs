//! Who the server runs as and where, for the prompt in the page's terminal.
//!
//! Read off the machine rather than written into the page: inside the development
//! container it says one thing, inside the Docker image another, and the prompt is
//! only worth having if it says which.

use std::path::Path;

use serde::Serialize;

/// The three things a shell prompt shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Machine {
    pub user: String,
    pub host: String,
    /// The project folder, with the home folder written as `~`.
    pub dir: String,
}

/// Who, on what, and where — read now.
pub fn read(repo: &Path) -> Machine {
    let host = std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|h| h.trim().to_owned())
        .filter(|h| !h.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "unknown-host".to_owned());

    let user = std::env::var("USER")
        .ok()
        .filter(|u| !u.is_empty())
        .or_else(|| {
            let status = std::fs::read_to_string("/proc/self/status").ok()?;
            let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
            user_named(&passwd, uid_from_status(&status)?)
        })
        .unwrap_or_else(|| "unknown-user".to_owned());

    let home = std::env::var("HOME").ok();
    Machine {
        user,
        host,
        dir: short_dir(&repo.display().to_string(), home.as_deref()),
    }
}

/// The real user id, from `/proc/self/status`. Pure.
pub fn uid_from_status(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|uid| uid.parse().ok())
}

/// The name `/etc/passwd` gives a user id. Pure.
pub fn user_named(passwd: &str, uid: u32) -> Option<String> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        let wanted = fields.nth(1)?.parse::<u32>().ok()? == uid;
        wanted.then(|| name.to_owned())
    })
}

/// A folder the way a prompt writes it: the home folder as `~`. Pure.
pub fn short_dir(dir: &str, home: Option<&str>) -> String {
    match home
        .map(|h| h.trim_end_matches('/'))
        .filter(|h| !h.is_empty())
    {
        Some(home) if dir == home => "~".to_owned(),
        Some(home) => match dir.strip_prefix(home).filter(|rest| rest.starts_with('/')) {
            Some(rest) => format!("~{rest}"),
            None => dir.to_owned(),
        },
        None => dir.to_owned(),
    }
}

#[cfg(test)]
#[path = "machine_tests.rs"]
mod tests;
