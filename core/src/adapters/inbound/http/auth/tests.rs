use super::{bearer_from_header, constant_time_eq, token_from_query};

#[test]
fn equal_slices_match() {
    assert!(constant_time_eq(b"s3cret", b"s3cret"));
}

#[test]
fn different_slices_do_not_match() {
    assert!(!constant_time_eq(b"s3cret", b"s3crdt"));
}

#[test]
fn differing_lengths_do_not_match() {
    assert!(!constant_time_eq(b"s3cret", b"s3cre"));
    assert!(!constant_time_eq(b"", b"s3cret"));
}

#[test]
fn empty_slices_match() {
    assert!(constant_time_eq(b"", b""));
}

#[test]
fn first_byte_difference_is_not_short_circuited() {
    assert!(!constant_time_eq(b"Xs3cret", b"as3cret"));
    assert!(!constant_time_eq(b"s3cretX", b"s3creta"));
}

#[test]
fn extracts_bearer_token() {
    assert_eq!(bearer_from_header("Bearer s3cret"), Some("s3cret"));
}

#[test]
fn rejects_non_bearer_schemes() {
    assert_eq!(bearer_from_header("Basic s3cret"), None);
    assert_eq!(bearer_from_header("s3cret"), None);
    assert_eq!(bearer_from_header(""), None);
}

#[test]
fn bearer_scheme_is_case_sensitive_and_requires_a_space() {
    assert_eq!(bearer_from_header("bearer s3cret"), None);
    assert_eq!(bearer_from_header("Bearers3cret"), None);
}

#[test]
fn trims_whitespace_around_the_token() {
    assert_eq!(bearer_from_header("Bearer   s3cret  "), Some("s3cret"));
}

#[test]
fn extracts_token_from_query() {
    assert_eq!(token_from_query("token=s3cret"), Some("s3cret"));
    assert_eq!(token_from_query("a=1&token=s3cret&b=2"), Some("s3cret"));
}

#[test]
fn query_without_token_yields_none() {
    assert_eq!(token_from_query(""), None);
    assert_eq!(token_from_query("a=1&b=2"), None);
}

#[test]
fn similarly_named_query_params_are_not_mistaken_for_the_token() {
    assert_eq!(token_from_query("mytoken=s3cret"), None);
    assert_eq!(token_from_query("token_id=s3cret"), None);
}

#[test]
fn empty_token_value_is_returned_as_empty_not_none() {
    assert_eq!(token_from_query("token="), Some(""));
}
