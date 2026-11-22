mod barrel;

use std::time::Duration;

use tokio::task::LocalSet;

use aquarium_core::domain::reservoir::{
    FillFault, FillResult, ReservoirBlocker, ReservoirChannel, ReservoirSettings,
};
use aquarium_core::ports::inbound::{FaultClearError, FillStartError, ReservoirControl};

use barrel::{Barrel, BarrelLayout, BarrelSim};

const LAYOUT: BarrelLayout = BarrelLayout::DEFAULT;
const HOURS: u64 = 3600;

fn hours(count: u64) -> Duration {
    Duration::from_secs(count * HOURS)
}

fn ro_only() -> ReservoirSettings {
    ReservoirSettings {
        empty_fill_s: LAYOUT.empty_fill_s(),
        max_duration_s: 43_200,
        ..ReservoirSettings::DEFAULT
    }
}

fn targeted() -> ReservoirSettings {
    ReservoirSettings {
        target_tds_ppm: Some(150),
        tolerance_ppm: 10,
        ..ro_only()
    }
}

#[tokio::test(start_paused = true)]
async fn an_empty_barrel_fills_to_the_target() {
    LocalSet::new()
        .run_until(async {
            let sim = BarrelSim::start(Barrel::new(LAYOUT), targeted()).await;
            sim.service.start_fill().expect("started");

            let record = sim.finished(hours(6)).await;
            let barrel = sim.barrel();
            assert_eq!(record.result, FillResult::Completed, "{record:?}");
            assert!(
                (barrel.mean_ppm() - 150.0).abs() <= 10.0,
                "mean {}",
                barrel.mean_ppm()
            );
            assert!(barrel.litres >= LAYOUT.full_l);
            assert_eq!(barrel.overflow_l, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn automatic_top_ups_keep_the_barrel_full_and_on_target() {
    LocalSet::new()
        .run_until(async {
            let settings = ReservoirSettings {
                auto_fill: true,
                ..targeted()
            };
            let sim =
                BarrelSim::start(Barrel::new(LAYOUT).with_water(100.5, 150.0), settings).await;

            let mut fills = Vec::new();
            for _ in 0..12 {
                sim.barrel().draw(3.0);
                fills.extend(sim.fills_during(hours(2)).await);
            }

            assert_eq!(fills.len(), 12, "{fills:?}");
            assert!(
                fills
                    .iter()
                    .all(|record| record.result == FillResult::Completed),
                "{fills:?}"
            );
            let barrel = sim.barrel();
            assert!(barrel.litres >= LAYOUT.full_l);
            assert!(
                (barrel.mean_ppm() - 150.0).abs() <= 10.0,
                "mean {}",
                barrel.mean_ppm()
            );
            assert_eq!(barrel.overflow_l, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_brief_dip_below_the_full_sensor_starts_nothing() {
    LocalSet::new()
        .run_until(async {
            let settings = ReservoirSettings {
                auto_fill: true,
                ..targeted()
            };
            let sim =
                BarrelSim::start(Barrel::new(LAYOUT).with_water(100.2, 150.0), settings).await;

            sim.barrel().draw(0.5);
            sim.run_for(Duration::from_secs(300)).await;
            sim.barrel().add(0.5, 150.0);
            sim.run_for(Duration::from_secs(900)).await;

            assert!(sim.status().fill.is_none());
            assert!(sim.status().last.is_none());
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_water_change_pauses_both_inlets_and_the_fill_finishes_after_it() {
    LocalSet::new()
        .run_until(async {
            let sim =
                BarrelSim::start(Barrel::new(LAYOUT).with_water(60.0, 150.0), targeted()).await;
            sim.service.start_fill().expect("started");
            sim.run_for(Duration::from_secs(300)).await;

            sim.begin_water_change().await;
            sim.run_for(Duration::from_secs(1200)).await;
            assert_eq!(sim.barrel().poured_during_water_change_l, 0.0);
            assert!(sim.status().fill.is_some());
            sim.end_water_change();

            let record = sim.finished(hours(4)).await;
            assert_eq!(record.result, FillResult::Completed, "{record:?}");
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_salty_residue_ends_off_target_without_room_to_dilute() {
    LocalSet::new()
        .run_until(async {
            let sim =
                BarrelSim::start(Barrel::new(LAYOUT).with_water(60.0, 400.0), targeted()).await;
            sim.service.start_fill().expect("started");

            let record = sim.finished(hours(4)).await;
            assert_eq!(record.result, FillResult::OffTarget, "{record:?}");
            assert!(record.final_tds_ppm.expect("a reading") > 160.0);
            assert_eq!(record.tap_pulses, 0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_stuck_full_sensor_ends_in_overtime_with_bounded_inflow() {
    LocalSet::new()
        .run_until(async {
            let mut barrel = Barrel::new(LAYOUT).with_water(50.0, 150.0);
            barrel.full_stuck_dry = true;
            let sim = BarrelSim::start(barrel, ro_only()).await;
            sim.service.start_fill().expect("started");

            let record = sim.finished(hours(6)).await;
            assert_eq!(record.result, FillResult::Overtime, "{record:?}");
            assert_eq!(
                sim.status().fault.map(|fault| fault.fault),
                Some(FillFault::Overtime)
            );

            let budget_l = LAYOUT.ro_lph * f64::from(LAYOUT.empty_fill_s()) * 1.2 / 3600.0;
            let barrel = sim.barrel();
            assert!(
                barrel.poured_l[0] <= budget_l + LAYOUT.ro_lph * 3.0 / 3600.0,
                "poured {}",
                barrel.poured_l[0]
            );
            assert_eq!(barrel.valves, [false; 4]);
            drop(barrel);
            assert_eq!(
                sim.service.start_fill(),
                Err(FillStartError::Blocked(ReservoirBlocker::Fault))
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_burst_hose_trips_overtime_before_the_barrel_fills() {
    LocalSet::new()
        .run_until(async {
            let mut barrel = Barrel::new(LAYOUT).with_water(20.0, 150.0);
            barrel.hose_burst = true;
            let sim = BarrelSim::start(barrel, ro_only()).await;
            sim.service.start_fill().expect("started");

            let record = sim.finished(hours(6)).await;
            assert_eq!(record.result, FillResult::Overtime);
            let barrel = sim.barrel();
            assert_eq!(barrel.litres, 20.0);
            let budget_l = LAYOUT.ro_lph * f64::from(LAYOUT.empty_fill_s()) * 1.2 / 3600.0;
            assert!(
                barrel.spilled_l <= budget_l + LAYOUT.ro_lph * 3.0 / 3600.0,
                "spilled {}",
                barrel.spilled_l
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_reboot_mid_fill_waits_for_a_manual_start() {
    LocalSet::new()
        .run_until(async {
            let settings = ReservoirSettings {
                auto_fill: true,
                ..ro_only()
            };
            let mut sim =
                BarrelSim::start(Barrel::new(LAYOUT).with_water(60.0, 150.0), settings).await;
            sim.service.start_fill().expect("started");
            sim.run_for(Duration::from_secs(300)).await;

            sim.reboot().await;
            let status = sim.status();
            assert!(status.held_after_reboot);
            assert_eq!(
                status.last.map(|record| record.result),
                Some(FillResult::InterruptedByReboot)
            );

            sim.run_for(hours(1)).await;
            assert!(sim.status().fill.is_none());
            assert_eq!(sim.barrel().valves, [false; 4]);

            sim.service.start_fill().expect("manual start");
            assert_eq!(sim.finished(hours(4)).await.result, FillResult::Completed);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn missing_tds_readings_end_the_fill_and_latch_automation() {
    LocalSet::new()
        .run_until(async {
            let settings = ReservoirSettings {
                auto_fill: true,
                ..targeted()
            };
            let sim = BarrelSim::start(Barrel::new(LAYOUT).with_water(90.0, 150.0), settings).await;
            sim.tds_live
                .store(false, std::sync::atomic::Ordering::Relaxed);

            let record = sim.finished(hours(1)).await;
            assert_eq!(record.result, FillResult::TdsUnavailable);
            assert!(sim.status().auto_latched);

            sim.run_for(hours(2)).await;
            assert!(sim.status().fill.is_none());
            assert_eq!(sim.status().last, Some(record));
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_leak_closes_every_channel_until_the_fault_is_cleared() {
    LocalSet::new()
        .run_until(async {
            let sim =
                BarrelSim::start(Barrel::new(LAYOUT).with_water(60.0, 150.0), ro_only()).await;
            sim.service.start_fill().expect("started");
            sim.run_for(Duration::from_secs(60)).await;

            sim.set_leak(Some(true));
            sim.run_for(Duration::from_secs(3)).await;
            assert_eq!(sim.barrel().valves, [false; 4]);
            assert_eq!(
                sim.status().fault.map(|fault| fault.fault),
                Some(FillFault::Leak)
            );
            assert_eq!(
                sim.service.clear_fault(),
                Err(FaultClearError::Blocked(ReservoirBlocker::Leak))
            );

            sim.set_leak(Some(false));
            sim.run_for(Duration::from_secs(3)).await;
            assert_eq!(sim.service.clear_fault(), Ok(()));
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn long_ro_runs_respect_the_duty_cycle() {
    LocalSet::new()
        .run_until(async {
            let settings = ReservoirSettings {
                inlet_max_on_s: 1200,
                inlet_rest_s: 300,
                ..ro_only()
            };
            let sim = BarrelSim::start(Barrel::new(LAYOUT), settings).await;
            sim.service.start_fill().expect("started");

            assert_eq!(sim.finished(hours(6)).await.result, FillResult::Completed);
            let segments = sim.barrel().on_segments(ReservoirChannel::RoValve);
            assert!(segments.len() >= 5, "{segments:?}");
            for (start, end) in &segments {
                assert!(end - start <= 1202.0, "run of {} s", end - start);
            }
            for pair in segments.windows(2) {
                assert!(
                    pair[1].0 - pair[0].1 >= 298.0,
                    "rest of {} s",
                    pair[1].0 - pair[0].1
                );
            }
        })
        .await;
}
