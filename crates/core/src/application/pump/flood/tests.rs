use crate::application::pump::bench::Bench;
use crate::application::pump::fixtures::*;
use crate::application::PumpController;
use crate::domain::flood::FloodProbes;
use crate::domain::pump::{Blocker, OperationKind, OperationResult, PumpId};
use crate::ports::inbound::{FloodResetError, PumpControl, StartError};
use crate::ports::outbound::{LeakSource, PumpRecord, PumpStatusSource};

const DRY: FloodProbes = FloodProbes {
    reservoir: false,
    sump: false,
};
const BARREL: FloodProbes = FloodProbes {
    reservoir: true,
    sump: false,
};
const SUMP: FloodProbes = FloodProbes {
    reservoir: false,
    sump: true,
};
const BOTH: FloodProbes = FloodProbes {
    reservoir: true,
    sump: true,
};

#[test]
fn each_probe_blocks_all_outputs_and_aborts_an_active_operation() {
    for alarm in [BARREL, SUMP, BOTH] {
        let mut rig = Rig::new();
        let levels = healthy();
        rig.service.observe_flood(DRY);
        rig.tick(Some(&levels));
        rig.service.start_top_off(60).unwrap();
        rig.tick(Some(&levels));
        assert!(rig.pins()[PumpId::Makeup.index()]);

        rig.service.observe_flood(alarm);
        rig.tick(None);
        assert_eq!(rig.pins(), [false; 4]);
        assert_eq!(rig.service.flood_status().probes, Some(alarm));
        let status = rig.service.status();
        assert_eq!(
            status.last_top_off.unwrap().result,
            OperationResult::Aborted
        );
        for id in PumpId::ALL {
            assert_eq!(status.channels.get(id).blocked_by, Some(Blocker::Leak));
        }
        assert_eq!(rig.service.reset_flood(), Err(FloodResetError::Wet));
        assert_eq!(
            rig.service.start_water_change(PLAN),
            Err(StartError::Blocked(Blocker::Leak))
        );
        assert_eq!(
            rig.service.resume(OperationKind::TopOff),
            Err(StartError::Blocked(Blocker::Leak))
        );

        rig.service.observe_flood(DRY);
        rig.tick(Some(&levels));
        assert_eq!(rig.pins(), [false; 4]);
        rig.service.reset_flood().unwrap();
        rig.tick(Some(&levels));
        assert!(!rig.pins()[PumpId::Makeup.index()]);
        assert_eq!(rig.service.leak(), Some(false));
    }
}

#[test]
fn reset_after_reboot_requires_fresh_dry_readings_from_both_zones() {
    let (service, store) = service_and_store();
    assert_eq!(service.leak(), None);
    service.observe_flood(BOTH);
    let reboot = service_on(&store, TestClock::synced(EPOCH));
    assert_eq!(reboot.leak(), Some(true));
    assert_eq!(reboot.reset_flood(), Err(FloodResetError::Unavailable));
    reboot.observe_flood(SUMP);
    assert_eq!(reboot.reset_flood(), Err(FloodResetError::Wet));
    reboot.observe_flood(DRY);
    reboot.reset_flood().unwrap();
    assert!(
        !service_on(&store, TestClock::synced(EPOCH))
            .flood_status()
            .latched
    );
}

#[test]
fn unreadable_alarm_requires_acknowledgment() {
    let (_, store) = service_and_store();
    store.put(PumpRecord::Flood, b"?".to_vec());
    assert!(
        service_on(&store, TestClock::synced(EPOCH))
            .flood_status()
            .latched
    );
}

#[test]
fn a_failed_write_keeps_the_lock_and_the_next_sample_retries() {
    let bench = Bench::new();
    bench.store.refuse(true);
    bench.service.observe_flood(BARREL);
    assert_eq!(bench.store.inner.get(PumpRecord::Flood), None);
    bench.store.refuse(false);
    bench.service.observe_flood(DRY);
    assert_eq!(bench.store.inner.get(PumpRecord::Flood).unwrap(), b"true");
    bench.store.refuse(true);
    assert!(matches!(
        bench.service.reset_flood(),
        Err(FloodResetError::Storage(_))
    ));
    assert!(bench.service.flood_status().latched);
    bench.store.refuse(false);
    bench.service.reset_flood().unwrap();
}

#[test]
fn flood_protection_drives_idle_relays_and_retries_a_failed_cutoff() {
    let (service, _) = service_and_store();
    let outputs = RecordingOutputs::default();
    let mut controller = PumpController::new(outputs.clone(), service.clone(), timing());
    let released_on = [true, true, false, false];
    controller.tick_flood_only(false, released_on);
    assert_eq!(outputs.state(), released_on);
    *outputs.failing.lock().unwrap() = Some(PumpId::Return);
    controller.tick_flood_only(true, released_on);
    assert!(service.status().channels.return_pump.fault.is_some());
    *outputs.failing.lock().unwrap() = None;
    controller.tick_flood_only(true, released_on);
    assert_eq!(outputs.state(), [false; 4]);
    controller.tick_flood_only(false, released_on);
    assert_eq!(outputs.state(), released_on);
}

#[test]
fn saved_flood_opens_and_cuts_off_idle_pump_relays_even_with_both_components_off() {
    use crate::application::pump::startup_pump_outputs;
    use crate::ports::outbound::PumpOutputs;
    let (service, store) = service_and_store();
    assert!(startup_pump_outputs::<RecordingOutputs, ()>(
        false,
        service.flood_status().latched,
        || panic!("idle relays opened without an alarm")
    )
    .unwrap()
    .is_none());
    service.observe_flood(BARREL);
    let reboot = service_on(&store, TestClock::synced(EPOCH));
    let mut released = RecordingOutputs::default();
    released.set(PumpId::Return, true).unwrap();
    released.set(PumpId::Uv, true).unwrap();
    let outputs = startup_pump_outputs(false, reboot.flood_status().latched, || {
        Ok::<_, ()>(released)
    })
    .unwrap()
    .unwrap();
    assert_eq!(outputs.state(), [false; 4]);
    assert_eq!(reboot.reset_flood(), Err(FloodResetError::Unavailable));
}
