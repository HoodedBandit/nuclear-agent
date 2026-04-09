use super::{truncate_utf8, truncate_with_suffix};

#[test]
fn truncate_utf8_preserves_char_boundaries() {
    let text = "hello😀world";
    assert_eq!(truncate_utf8(text, 8), "hello");
}

#[test]
fn truncate_with_suffix_appends_suffix_after_safe_truncation() {
    let text = "hello😀world";
    assert_eq!(truncate_with_suffix(text, 8, " [cut]"), "hello [cut]");
}
