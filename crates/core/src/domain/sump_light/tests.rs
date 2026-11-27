use super::validate_request;

#[test]
fn on_without_a_duration_is_accepted() {
    assert!(validate_request(true, None).is_ok());
}

#[test]
fn off_without_a_duration_is_accepted() {
    assert!(validate_request(false, None).is_ok());
}

#[test]
fn off_with_a_duration_is_rejected() {
    assert!(validate_request(false, Some(600)).is_err());
}

#[test]
fn durations_at_the_limits_are_accepted() {
    assert!(validate_request(true, Some(60)).is_ok());
    assert!(validate_request(true, Some(86_400)).is_ok());
}

#[test]
fn durations_outside_the_limits_are_rejected() {
    assert!(validate_request(true, Some(59)).is_err());
    assert!(validate_request(true, Some(86_401)).is_err());
}
