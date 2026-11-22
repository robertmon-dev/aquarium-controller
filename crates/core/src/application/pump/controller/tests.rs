use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{watch, Notify};
use tokio::task::LocalSet;

use crate::adapters::outbound::memory::{InMemoryWaterLevelStore, VolatilePumpStore};
use crate::application::pump::fixtures::*;
use crate::application::pump::{pump_loop, PumpController, PumpTiming};
use crate::domain::pump::{Blocker, ChannelFault, OperationResult, PumpId};
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};
use crate::ports::inbound::{PumpControl, StartError};
use crate::ports::outbound::{LevelSample, PumpStatusSource, WaterLevelSink};

#[test]
fn healthy_levels_run_circulation_and_report_a_change_once() {
    let mut rig = Rig::new();
    let healthy = healthy();

    assert!(rig.tick(Some(&healthy)));
    assert_eq!(rig.pins(), [true, true, false, false]);

    rig.now += Duration::from_millis(100);
    assert!(!rig.tick(Some(&healthy)));
}

#[test]
fn missing_levels_keep_circulation_and_block_water_movers() {
    let mut rig = Rig::new();

    rig.tick(None);

    assert_eq!(rig.pins(), [true, true, false, false]);
    let status = rig.service.status();
    assert_eq!(
        status.channels.makeup.blocked_by,
        Some(Blocker::LevelsUnavailable)
    );
    assert_eq!(
        status.channels.drain.blocked_by,
        Some(Blocker::LevelsUnavailable)
    );
}

#[test]
fn a_failed_output_is_reported_as_a_fault_in_its_last_known_state() {
    let mut rig = Rig::new();
    *rig.outputs.failing.lock().unwrap() = Some(PumpId::Uv);

    rig.tick(Some(&healthy()));

    let uv = rig.service.status().channels.uv;
    assert_eq!((uv.on, uv.fault), (true, Some(ChannelFault::OutputError)));

    *rig.outputs.failing.lock().unwrap() = Some(PumpId::Makeup);
    rig.service.start_top_off(60).expect("idle");
    rig.run(1, Some(&healthy()));

    let makeup = rig.service.status().channels.makeup;
    assert_eq!(
        (makeup.on, makeup.fault),
        (false, Some(ChannelFault::OutputError))
    );
}

#[test]
fn the_default_timing_checkpoints_every_three_seconds() {
    assert_eq!(
        PumpTiming::DEFAULT.checkpoint_interval,
        Duration::from_secs(3)
    );
    assert_eq!(
        PumpTiming::DEFAULT.return_restart_delay,
        Duration::from_secs(30)
    );
    assert_eq!(PumpTiming::DEFAULT.level_stale, Duration::from_secs(1));
    assert_eq!(PumpTiming::DEFAULT.tick, Duration::from_secs(1));
    assert_eq!(PumpTiming::DEFAULT.runtime_flush, Duration::from_secs(600));
}

#[test]
fn a_failed_return_pump_forces_the_uv_off() {
    let mut rig = Rig::new();
    *rig.outputs.failing.lock().unwrap() = Some(PumpId::Return);

    rig.tick(Some(&healthy()));

    let channels = rig.service.status().channels;
    assert!(!rig.pins()[PumpId::Uv.index()]);
    assert_eq!(channels.uv.blocked_by, Some(Blocker::ReturnOff));
    assert_eq!(channels.return_pump.fault, Some(ChannelFault::OutputError));
}

fn spawn_loop(levels: &InMemoryWaterLevelStore) -> (Arc<Service>, RecordingOutputs, Arc<Notify>) {
    let (service, outputs, republish, _heartbeat) = spawn_loop_with_heartbeat(levels.subscribe());
    (service, outputs, republish)
}

fn spawn_loop_with_heartbeat(
    levels: watch::Receiver<Option<LevelSample>>,
) -> (Arc<Service>, RecordingOutputs, Arc<Notify>, Arc<AtomicU32>) {
    let service = service_on(
        &Arc::new(VolatilePumpStore::default()),
        TestClock::synced(EPOCH),
    );
    let outputs = RecordingOutputs::default();
    let republish = Arc::new(Notify::new());
    let heartbeat = Arc::new(AtomicU32::new(0));

    tokio::task::spawn_local(pump_loop(
        PumpController::new(outputs.clone(), Arc::clone(&service), timing()),
        levels,
        service.change_signal(),
        Arc::clone(&republish),
        timing(),
        Arc::clone(&heartbeat),
    ));

    (service, outputs, republish, heartbeat)
}

#[tokio::test(start_paused = true)]
async fn the_loop_treats_quiet_level_sensors_as_unavailable() {
    LocalSet::new()
        .run_until(async {
            let levels = InMemoryWaterLevelStore::new();
            let (service, _outputs, _republish) = spawn_loop(&levels);

            levels.publish(healthy());
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert_eq!(service.status().channels.makeup.blocked_by, None);

            tokio::time::sleep(Duration::from_millis(1500)).await;
            assert_eq!(
                service.status().channels.makeup.blocked_by,
                Some(Blocker::LevelsUnavailable)
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_level_sample_delivered_late_is_judged_by_its_measurement_time() {
    LocalSet::new()
        .run_until(async {
            let levels = InMemoryWaterLevelStore::new();
            let receiver = levels.subscribe();
            levels.publish(healthy());
            tokio::time::sleep(Duration::from_secs(2)).await;

            let (service, outputs, _republish, _heartbeat) = spawn_loop_with_heartbeat(receiver);
            tokio::time::sleep(Duration::from_millis(10)).await;

            assert_eq!(
                service.start_water_change(PLAN),
                Err(StartError::Blocked(Blocker::LevelsUnavailable))
            );
            tokio::time::sleep(Duration::from_millis(1500)).await;
            assert!(!outputs.state()[PumpId::Drain.index()]);
            assert_eq!(
                service.status().channels.drain.blocked_by,
                Some(Blocker::LevelsUnavailable)
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn an_api_change_is_applied_without_waiting_for_the_ticker() {
    LocalSet::new()
        .run_until(async {
            let levels = InMemoryWaterLevelStore::new();
            let (service, outputs, republish) = spawn_loop(&levels);

            levels.publish(healthy());
            tokio::time::sleep(Duration::from_millis(10)).await;
            assert!(outputs.state()[1]);
            tokio::time::timeout(Duration::from_millis(10), republish.notified())
                .await
                .expect("startup republished");

            service.set_override(PumpId::Uv, None).expect("valid");
            tokio::time::sleep(Duration::from_millis(10)).await;

            assert!(!outputs.state()[1]);
            tokio::time::timeout(Duration::from_millis(10), republish.notified())
                .await
                .expect("republished");
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn the_loop_beats_its_heartbeat_every_tick() {
    LocalSet::new()
        .run_until(async {
            let levels = InMemoryWaterLevelStore::new();
            let (_service, _outputs, _republish, heartbeat) =
                spawn_loop_with_heartbeat(levels.subscribe());

            tokio::time::sleep(Duration::from_millis(3500)).await;

            assert!(
                heartbeat.load(Ordering::Relaxed) >= 4,
                "{}",
                heartbeat.load(Ordering::Relaxed)
            );
        })
        .await;
}

fn shorted() -> WaterLevelSnapshot {
    snapshot(&WaterLevelRole::ALL)
}

fn suspect(rig: &Rig) -> bool {
    rig.service.status().level_sensors_suspect
}

#[test]
fn an_unplugged_harness_is_treated_as_missing_levels() {
    let mut rig = Rig::new();
    set_refill_stop(&rig.service, WaterLevelRole::SumpMechanical);
    rig.run(5, Some(&running_sump()));
    rig.service.start_top_off(60).expect("idle");
    rig.run(1, Some(&running_sump()));
    assert!(rig.pins()[PumpId::Makeup.index()]);
    assert!(!suspect(&rig));

    let unplugged = WaterLevelSnapshot::dry();
    rig.now += Duration::from_millis(100);
    assert!(rig.tick(Some(&unplugged)));
    assert!(suspect(&rig));
    assert!(rig.service.pump_telemetry().level_sensors_suspect);
    assert_eq!(rig.pins(), [true, true, false, false]);

    let status = rig.service.status();
    assert_eq!(
        status.last_top_off.map(|record| record.result),
        Some(OperationResult::Aborted)
    );
    assert_eq!(
        status.channels.makeup.blocked_by,
        Some(Blocker::LevelsUnavailable)
    );
    assert_eq!(status.channels.return_pump.blocked_by, None);

    rig.run(900, Some(&unplugged));
    assert!(suspect(&rig));
    assert_eq!(rig.pins(), [true, true, false, false]);
    assert!(!starved(&rig));
    assert_eq!(
        rig.service.start_top_off(60),
        Err(StartError::Blocked(Blocker::LevelsUnavailable))
    );

    rig.run(1, Some(&running_sump()));
    assert!(!suspect(&rig));
    assert_eq!(rig.service.start_top_off(60), Ok(()));
}

#[test]
fn a_sump_drained_by_the_pumps_is_not_an_unplugged_harness() {
    let mut rig = Rig::new();
    rig.run(5, Some(&running_sump()));
    rig.run(30, Some(&healthy()));
    rig.run(1, Some(&low_sump()));

    assert!(!suspect(&rig));
    assert_eq!(rig.pins(), [false, false, false, false]);
}

#[test]
fn a_shorted_harness_refuses_a_water_change_and_keeps_the_return() {
    let mut rig = Rig::new();
    rig.run(5, Some(&running_sump()));

    rig.now += Duration::from_millis(100);
    assert!(rig.tick(Some(&shorted())));
    assert!(suspect(&rig));
    rig.run(10, Some(&shorted()));

    assert_eq!(rig.pins(), [true, true, false, false]);
    assert_eq!(
        rig.service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::LevelsUnavailable))
    );

    let mut tank_dry = shorted();
    tank_dry.sensors[WaterLevelRole::MainTank.index()].state = WaterLevelState::Dry;
    rig.run(1, Some(&tank_dry));
    assert!(!suspect(&rig));
}

#[test]
fn a_harness_fault_is_reported_until_it_clears() {
    let mut rig = Rig::new();
    rig.run(1, Some(&running_sump()));
    assert!(!rig.service.pump_telemetry().level_sensors_suspect);

    rig.run(1, Some(&WaterLevelSnapshot::dry()));
    assert!(rig.service.pump_telemetry().level_sensors_suspect);

    rig.now += Duration::from_secs(1);
    assert!(rig.tick(Some(&running_sump())));
    assert!(!rig.service.pump_telemetry().level_sensors_suspect);
}
