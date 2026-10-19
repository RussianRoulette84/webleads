//! Tests for the filter bar's wording.

use super::*;

#[test]
fn one_filter_is_said_in_the_singular() {
    assert_eq!(said_narrowing(1), "1 filter is narrowing the list");
    assert_eq!(said_narrowing(3), "3 filters are narrowing the list");
}
