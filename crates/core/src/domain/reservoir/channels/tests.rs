use crate::domain::reservoir::*;

#[test]
fn channel_and_target_names_round_trip() {
    for channel in ReservoirChannel::ALL {
        assert_eq!(channel.name().parse::<ReservoirChannel>(), Ok(channel));
    }
    for target in OverrideTarget::ALL {
        assert_eq!(target.name().parse::<OverrideTarget>(), Ok(target));
    }
    assert!("booster".parse::<OverrideTarget>().is_err());
}

#[test]
fn overrides_need_a_bounded_duration_to_switch_on() {
    assert_eq!(
        validate_override(true, None),
        Err("an ON override needs duration_s")
    );
    assert_eq!(
        validate_override(true, Some(0)),
        Err("an override lasts 1 to 600 s")
    );
    assert_eq!(
        validate_override(false, Some(601)),
        Err("an override lasts 1 to 600 s")
    );
    assert_eq!(validate_override(true, Some(600)), Ok(()));
    assert_eq!(validate_override(false, None), Ok(()));
}
