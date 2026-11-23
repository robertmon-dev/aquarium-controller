use crate::application::reservoir::fixtures::*;
use crate::domain::reservoir::{
    ChannelMode, FillFault, OverrideTarget, ReservoirBlocker, ReservoirChannel, ReservoirSettings,
};
use crate::ports::inbound::{
    FaultClearError, FillStartError, ReservoirControl, ReservoirOverrideError,
};

use ReservoirChannel::{Mixer, RoValve};

#[test]
fn an_off_override_masks_the_fill_and_on_overrides_wait_for_it_to_end() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");
    rig.set_override(OverrideTarget::Mixer, false, None)
        .expect("mixer off");
    rig.run(5);
    assert!(rig.relays.pin(RoValve));
    assert!(!rig.relays.pin(Mixer));

    assert_eq!(
        rig.set_override(OverrideTarget::Tap, true, Some(30)),
        Err(ReservoirOverrideError::Busy)
    );
}

#[test]
fn an_on_override_expires_on_time() {
    let mut rig = Rig::new(RO_ONLY);
    rig.set_override(OverrideTarget::Mixer, true, Some(10))
        .expect("mixer on");
    rig.run(2);
    assert!(rig.relays.pin(Mixer));

    rig.run(10);
    assert!(!rig.relays.pin(Mixer));
    assert_eq!(rig.status().channels.mixer.mode, ChannelMode::Auto);
}

#[test]
fn a_leak_stops_everything_and_its_fault_clears_only_when_dry() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");
    rig.run(5);

    rig.leak = Some(true);
    rig.run(1);
    assert_eq!(rig.relays.pins(), [false; 4]);
    assert_eq!(
        rig.status().fault.map(|record| record.fault),
        Some(FillFault::Leak)
    );
    assert_eq!(
        rig.service.clear_fault(),
        Err(FaultClearError::Blocked(ReservoirBlocker::Leak))
    );

    rig.leak = Some(false);
    rig.run(1);
    assert_eq!(rig.service.clear_fault(), Ok(()));
}

#[test]
fn a_manual_start_reports_the_first_refusal_in_order() {
    let mut rig = Rig::new(TARGETED);
    rig.levels = barrel(&FULL);
    rig.water_change = true;
    rig.tick();
    assert_eq!(
        rig.start_fill(),
        Err(FillStartError::Blocked(ReservoirBlocker::ReservoirFull))
    );

    rig.levels = barrel(&HALF);
    rig.tick();
    assert_eq!(
        rig.start_fill(),
        Err(FillStartError::Blocked(ReservoirBlocker::WaterChange))
    );

    rig.water_change = false;
    rig.tick();
    assert_eq!(
        rig.start_fill(),
        Err(FillStartError::Blocked(ReservoirBlocker::TdsUnavailable))
    );

    rig.tds = Some(150.0);
    rig.tick();
    assert_eq!(rig.start_fill(), Ok(()));
    assert_eq!(rig.start_fill(), Err(FillStartError::Conflict));
}

#[test]
fn a_duty_rest_refuses_an_inlet_override_but_not_a_manual_start() {
    let settings = ReservoirSettings {
        inlet_max_on_s: 600,
        inlet_rest_s: 60,
        ..RO_ONLY
    };
    let mut rig = Rig::new(settings);
    rig.start_fill().expect("started");
    rig.run(602);
    rig.cancel_fill().expect("cancelled");

    assert_eq!(
        rig.set_override(OverrideTarget::Ro, true, Some(30)),
        Err(ReservoirOverrideError::Blocked(ReservoirBlocker::DutyRest))
    );
    assert_eq!(rig.start_fill(), Ok(()));
}

#[test]
fn inlet_overrides_exclude_each_other_and_the_mixer_refuses_an_empty_barrel() {
    let mut rig = Rig::new(RO_ONLY);
    rig.set_override(OverrideTarget::Ro, true, Some(30))
        .expect("ro on");
    assert_eq!(
        rig.set_override(OverrideTarget::Tap, true, Some(30)),
        Err(ReservoirOverrideError::Blocked(ReservoirBlocker::Interlock))
    );
    assert!(rig.set_override(OverrideTarget::Tap, false, None).is_ok());

    rig.levels = barrel(&FULL);
    rig.tick();
    assert_eq!(
        rig.set_override(OverrideTarget::Ro, true, Some(30)),
        Err(ReservoirOverrideError::Blocked(
            ReservoirBlocker::ReservoirFull
        ))
    );

    rig.levels = barrel(&[]);
    rig.tick();
    assert_eq!(
        rig.set_override(OverrideTarget::Mixer, true, Some(30)),
        Err(ReservoirOverrideError::Blocked(
            ReservoirBlocker::ReservoirEmpty
        ))
    );
}

#[test]
fn an_inlet_valve_that_failed_to_close_refuses_inlet_overrides_and_starts() {
    let mut rig = Rig::new(RO_ONLY);
    rig.set_override(OverrideTarget::Ro, true, Some(60))
        .expect("ro on");
    rig.run(1);

    rig.relays.refuse_off(Some(RoValve));
    rig.service.clear_override(OverrideTarget::Ro);
    rig.run(2);

    assert_eq!(
        rig.set_override(OverrideTarget::Tap, true, Some(30)),
        Err(ReservoirOverrideError::Blocked(ReservoirBlocker::Interlock))
    );
    assert_eq!(
        rig.start_fill(),
        Err(FillStartError::Blocked(ReservoirBlocker::Interlock))
    );
    assert!(rig
        .set_override(OverrideTarget::Mixer, true, Some(30))
        .is_ok());
}

#[test]
fn a_stale_tds_reading_refuses_a_targeted_start() {
    let mut rig = Rig::new(TARGETED);
    rig.tds = Some(150.0);
    rig.tick();
    rig.tds = None;
    rig.run(31);
    assert_eq!(
        rig.start_fill(),
        Err(FillStartError::Blocked(ReservoirBlocker::TdsUnavailable))
    );

    rig.tds = Some(150.0);
    rig.tick();
    assert_eq!(rig.start_fill(), Ok(()));
}

#[test]
fn a_start_drops_on_overrides_and_keeps_off_overrides() {
    let rig = Rig::new(RO_ONLY);
    rig.set_override(OverrideTarget::Tap, false, None)
        .expect("tap off");
    rig.set_override(OverrideTarget::Mixer, true, Some(60))
        .expect("mixer on");

    rig.start_fill().expect("started");

    let status = rig.status();
    assert_eq!(status.channels.mixer.mode, ChannelMode::Auto);
    assert_eq!(status.channels.tap_valve.mode, ChannelMode::ManualOff);
}
