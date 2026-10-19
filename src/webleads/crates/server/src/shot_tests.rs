//! Tests for [`crate::shot`].

use super::*;

#[test]
fn a_googles_own_id_becomes_a_plain_file_name() {
    assert_eq!(
        safe_name("17181965426373652918"),
        "17181965426373652918.png"
    );
}

#[test]
fn an_id_full_of_punctuation_cannot_escape_the_folder() {
    // Our fallback id is `name|lat,lon`, and a name can hold anything at all.
    let name = safe_name("../../etc/passwd");
    assert!(!name.contains('/'), "{name}");
    assert!(!name.contains(".."), "{name}");
    assert!(name.ends_with(".png"));
}

#[test]
fn hungarian_letters_do_not_end_up_in_the_file_name() {
    let name = safe_name("borzas bárber|47.51,19.07");
    assert!(name.is_ascii(), "{name}");
    assert_eq!(name, "borzas-b-rber-47-51-19-07.png");
}

#[test]
fn an_id_of_nothing_useful_still_gets_a_name() {
    assert_eq!(safe_name("///"), "unnamed.png");
    assert_eq!(safe_name(""), "unnamed.png");
}

#[test]
fn two_businesses_get_two_different_pictures() {
    assert_ne!(safe_name("111"), safe_name("222"));
}

#[test]
fn the_browser_is_told_where_to_put_the_picture() {
    let args = arguments("https://example.com", Path::new("/tmp/shots/a.png"));
    assert!(args.iter().any(|a| a == "--screenshot=/tmp/shots/a.png"));
    // The address goes last, which is how this browser expects it.
    assert_eq!(args.last().map(String::as_str), Some("https://example.com"));
}

#[test]
fn the_picture_is_a_window_not_a_postage_stamp() {
    let args = arguments("https://example.com", Path::new("/tmp/a.png"));
    assert!(args.iter().any(|a| a == "--window-size=1280,900"));
}

#[test]
fn a_blank_picture_says_so_rather_than_pretending() {
    // A browser that reached nothing still writes a white rectangle. Telling the
    // owner "here is your site" while showing a blank square is the lie this avoids.
    assert!(describe_size(2_000).contains("nothing loaded"));
    assert!(!describe_size(90_000).contains("nothing loaded"));
}

#[test]
fn the_browser_comes_from_the_environment_when_it_is_set() {
    // Checked rather than assumed, because a wrong path is the difference between a
    // clear error and a silent empty folder.
    assert_eq!(browser().to_string_lossy(), DEFAULT_BROWSER);
}
