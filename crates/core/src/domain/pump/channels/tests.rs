use crate::domain::pump::*;

#[test]
fn pump_names_round_trip() {
    for id in PumpId::ALL {
        assert_eq!(id.name().parse::<PumpId>(), Ok(id));
    }

    assert!("filter".parse::<PumpId>().is_err());
    assert_eq!(PumpId::Return.override_slot(), Some(0));
    assert_eq!(PumpId::Uv.override_slot(), Some(1));
    assert_eq!(PumpId::Makeup.override_slot(), None);
    assert_eq!(PumpId::Drain.override_slot(), None);
}

#[test]
fn override_rules_differ_per_channel() {
    assert_eq!(
        validate_override(PumpId::Return, None),
        Err(OverrideError::Invalid(
            "a return pump override needs duration_s"
        ))
    );
    assert!(validate_override(PumpId::Return, Some(59)).is_err());
    assert!(validate_override(PumpId::Return, Some(MAX_RETURN_OVERRIDE_S + 1)).is_err());
    assert_eq!(validate_override(PumpId::Return, Some(600)), Ok(()));

    assert_eq!(validate_override(PumpId::Uv, None), Ok(()));
    assert!(validate_override(PumpId::Uv, Some(MAX_UV_OVERRIDE_S + 1)).is_err());
    assert_eq!(validate_override(PumpId::Uv, Some(86_400)), Ok(()));

    assert_eq!(
        validate_override(PumpId::Makeup, Some(600)),
        Err(OverrideError::NotOverridable)
    );
    assert_eq!(
        validate_override(PumpId::Drain, None),
        Err(OverrideError::NotOverridable)
    );
}

#[test]
fn only_the_normally_closed_circulation_channels_fail_safe_on() {
    assert!(PumpId::Return.fail_safe_on());
    assert!(PumpId::Uv.fail_safe_on());
    assert!(!PumpId::Makeup.fail_safe_on());
    assert!(!PumpId::Drain.fail_safe_on());
}
