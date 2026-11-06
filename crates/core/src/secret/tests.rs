use super::Secret;

#[test]
fn debug_never_prints_the_value() {
    let s = Secret::new("hunter2");
    let rendered = format!("{s:?}");
    assert!(
        !rendered.contains("hunter2"),
        "the secret leaked: {rendered}"
    );
    assert_eq!(rendered, "***");
}

#[test]
fn alternate_debug_also_redacts() {
    let s = Secret::new("hunter2");
    assert!(!format!("{s:#?}").contains("hunter2"));
}

#[test]
fn derived_debug_on_a_struct_redacts_the_field() {
    #[derive(Debug)]
    #[allow(dead_code)]
    struct Config {
        user: &'static str,
        password: Secret<&'static str>,
    }

    let rendered = format!(
        "{:?}",
        Config {
            user: "robert",
            password: Secret::new("hunter2")
        }
    );

    assert!(
        rendered.contains("robert"),
        "plain fields must stay visible"
    );
    assert!(
        !rendered.contains("hunter2"),
        "the secret leaked: {rendered}"
    );
}

#[test]
fn expose_returns_the_value() {
    assert_eq!(Secret::new("hunter2").expose(), "hunter2");
}
