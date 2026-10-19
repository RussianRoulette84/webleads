//! Tests for [`crate::machine`].

use super::*;

#[test]
fn the_user_id_is_read_off_the_status_file() {
    let status = "Name:\twebleads-server\nUmask:\t0022\nUid:\t1000\t1000\t1000\t1000\nGid:\t1000\n";
    assert_eq!(uid_from_status(status), Some(1000));
    assert_eq!(uid_from_status("Name:\tnothing\n"), None);
}

#[test]
fn a_user_id_is_named_from_the_password_file() {
    let passwd = "root:x:0:0:root:/root:/bin/bash\ncaller:x:1000:1000::/home/caller:/bin/zsh\n";
    assert_eq!(user_named(passwd, 0).as_deref(), Some("root"));
    assert_eq!(user_named(passwd, 1000).as_deref(), Some("caller"));
    assert_eq!(user_named(passwd, 42), None);
}

#[test]
fn the_home_folder_is_written_as_a_tilde() {
    assert_eq!(
        short_dir("/root/Projects/webleads", Some("/root")),
        "~/Projects/webleads"
    );
    assert_eq!(short_dir("/root", Some("/root/")), "~");
}

#[test]
fn a_folder_outside_home_is_written_in_full() {
    // Inside the Docker image the project is at /app.
    assert_eq!(short_dir("/app", Some("/root")), "/app");
    // A folder that merely starts with the same letters is not inside it.
    assert_eq!(short_dir("/rootless/app", Some("/root")), "/rootless/app");
    assert_eq!(short_dir("/app", None), "/app");
}
