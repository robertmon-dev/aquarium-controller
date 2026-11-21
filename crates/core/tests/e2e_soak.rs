#![recursion_limit = "256"]

mod common;
mod sim;

use std::collections::VecDeque;

use tokio::task::LocalSet;

use aquarium_core::domain::pump::{
    OperationKind, OperationRecord, OperationResult, OperationState, PumpId, PumpSettingsPatch,
    PumpsStatus,
};
use aquarium_core::ports::inbound::PumpControl;

use sim::{civil, Event, Layout, Lcg, Options, Sim};

const SEED: u64 = 18;
const START_EPOCH: u64 = 1_795_392_000;
const DAY_S: u64 = 86_400;

const SETTINGS: &str = r#"{
    "flow_lph":{"makeup":600,"drain":1500},
    "refill_stop_sensor":"sump_return_level",
    "min_water_change_gap_h":12,
    "reservoir_ready_l":47,
    "top_off":[{"cron":"0 */3 * * *","duration_s":120}],
    "water_change":[{"cron":"0 10 * * sun","drain_s":112,"settle_s":30,"refill_s":600}]}"#;
const DRAIN_S: f64 = 112.0;
const CHECKPOINT_S: f64 = 10.0;

const EVAPORATION_L_PER_DAY: (f64, f64) = (5.0, 10.0);
const DAY_HOURS: (u64, u64) = (8, 22);
const DAY_WEIGHT: f64 = 1.5;
const NIGHT_WEIGHT: f64 = 0.5;
const REFILL_EVERY_DAYS: u64 = 3;
const REFILL_DAY_OFFSET: u64 = 2;
const REFILL_HOUR: u64 = 18;
const NEW_WATER_MG_L: f64 = 200.0;
const OPERATOR_PERIOD_S: u64 = 600;

const P_RESTART: f64 = 0.6;
const P_POWER_CUT: f64 = 0.3;
const P_SNTP_LOSS: f64 = 0.7;
const P_LEVEL_OUTAGE: f64 = 0.7;
const P_CUT_IN_WATER_CHANGE: f64 = 0.75;
const RESTART_OUTAGE_S: (f64, f64) = (2.0, 90.0);
const POWER_OUTAGE_S: (f64, f64) = (5.0, 1800.0);
const CHANGE_CUT_OUTAGE_S: (f64, f64) = (5.0, 900.0);
const CHANGE_CUT_WITHIN_S: f64 = 400.0;
const SNTP_LOSS_S: (f64, f64) = (60.0, 1800.0);
const LATE_SNTP_S: (f64, f64) = (60.0, 600.0);
const LEVEL_OUTAGE_S: (f64, f64) = (1.0, 8.0);
const P_SYNCED_BOOT: f64 = 0.7;

const DRY_RUN_GRACE_S: f64 = 3.0;
const TANK_DEFICIT_MAX_L: f64 = 60.0;
const EXCHANGE_TOL_L: f64 = 1.0;
const CONCENTRATION_BAND: f64 = 0.10;
const BALANCE_TOL_L: f64 = 1e-6;
const LEVEL_EPS: f64 = 1e-9;

#[derive(Debug, Clone, Copy)]
enum Disturbance {
    Restart {
        outage_s: f64,
        late_sync_s: Option<f64>,
    },
    PowerCut {
        outage_s: f64,
        late_sync_s: Option<f64>,
    },
    SntpLoss {
        duration_s: f64,
    },
    LevelOutage {
        duration_s: f64,
    },
}

#[derive(Debug, Clone, Copy)]
struct Scheduled {
    at_s: f64,
    what: Disturbance,
}

fn late_sync(rng: &mut Lcg) -> Option<f64> {
    (!rng.chance(P_SYNCED_BOOT)).then(|| rng.range(LATE_SNTP_S.0, LATE_SNTP_S.1))
}

fn schedule(days: u64, rng: &mut Lcg) -> VecDeque<Scheduled> {
    let mut events = Vec::new();

    for day in 0..days {
        let base = (day * DAY_S) as f64;
        let at = |rng: &mut Lcg| base + rng.range(0.0, DAY_S as f64);

        if rng.chance(P_RESTART) {
            let at_s = at(rng);
            let outage_s = rng.range(RESTART_OUTAGE_S.0, RESTART_OUTAGE_S.1);
            let late_sync_s = late_sync(rng);
            events.push(Scheduled {
                at_s,
                what: Disturbance::Restart {
                    outage_s,
                    late_sync_s,
                },
            });
        }
        if rng.chance(P_POWER_CUT) {
            let at_s = at(rng);
            let outage_s = rng.range(POWER_OUTAGE_S.0, POWER_OUTAGE_S.1);
            let late_sync_s = late_sync(rng);
            events.push(Scheduled {
                at_s,
                what: Disturbance::PowerCut {
                    outage_s,
                    late_sync_s,
                },
            });
        }
        if rng.chance(P_SNTP_LOSS) {
            let at_s = at(rng);
            let duration_s = rng.range(SNTP_LOSS_S.0, SNTP_LOSS_S.1);
            events.push(Scheduled {
                at_s,
                what: Disturbance::SntpLoss { duration_s },
            });
        }
        if rng.chance(P_LEVEL_OUTAGE) {
            let at_s = at(rng);
            let duration_s = rng.range(LEVEL_OUTAGE_S.0, LEVEL_OUTAGE_S.1);
            events.push(Scheduled {
                at_s,
                what: Disturbance::LevelOutage { duration_s },
            });
        }
        if civil(START_EPOCH + day * DAY_S).weekday == 0 && rng.chance(P_CUT_IN_WATER_CHANGE) {
            let at_s = base + 10.0 * 3600.0 + rng.range(0.0, CHANGE_CUT_WITHIN_S);
            let outage_s = rng.range(CHANGE_CUT_OUTAGE_S.0, CHANGE_CUT_OUTAGE_S.1);
            let what = if rng.coin() {
                Disturbance::PowerCut {
                    outage_s,
                    late_sync_s: None,
                }
            } else {
                Disturbance::Restart {
                    outage_s: outage_s.min(RESTART_OUTAGE_S.1),
                    late_sync_s: None,
                }
            };
            events.push(Scheduled { at_s, what });
        }
    }

    events.sort_by(|a, b| a.at_s.total_cmp(&b.at_s));
    events.into()
}

fn evaporation_weight(hour: u64) -> f64 {
    if (DAY_HOURS.0..DAY_HOURS.1).contains(&hour) {
        DAY_WEIGHT
    } else {
        NIGHT_WEIGHT
    }
}

fn evaporation_l_per_s(daily_l: f64, hour: u64) -> f64 {
    let day_hours = (DAY_HOURS.1 - DAY_HOURS.0) as f64;
    let weights = day_hours * DAY_WEIGHT + (24.0 - day_hours) * NIGHT_WEIGHT;
    daily_l * evaporation_weight(hour) / (weights * 3600.0)
}

#[derive(Debug, Clone, Copy)]
struct Down {
    until_s: f64,
    power: bool,
    late_sync_s: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct ChangeStart {
    drained_l: f64,
}

#[derive(Debug, Default)]
struct Report {
    restarts: usize,
    power_cuts: usize,
    skipped_cuts: usize,
    sntp_losses: usize,
    level_outages: usize,
    operator_resumes: usize,
    operator_cancels: usize,
    reservoir_refills: usize,
    supplied_l: f64,
    allowed_dry_s: f64,
    water_changes: Vec<OperationRecord>,
    top_offs: Vec<OperationResult>,
    skipped_windows: usize,
    min_tank_l: f64,
    min_return_cm: f64,
    min_concentration: f64,
    max_concentration: f64,
    max_balance_error_l: f64,
}

struct Soak {
    sim: Sim,
    rng: Lcg,
    events: VecDeque<Scheduled>,
    down: Option<Down>,
    sync_at_s: f64,
    outage_until_s: Option<f64>,
    dry_grace_until_s: f64,
    last_return_dry_s: f64,
    evaporation_l_per_day: f64,
    change: Option<ChangeStart>,
    status: PumpsStatus,
    report: Report,
}

impl Soak {
    fn t_s(&self) -> f64 {
        self.sim.water().t_s
    }

    fn label(&self) -> String {
        let at = civil(START_EPOCH + self.t_s() as u64);
        format!(
            "day {} {:02}:{:02} ({}): {}",
            self.t_s() as u64 / DAY_S,
            at.hour,
            at.minute,
            self.t_s(),
            self.sim.water().describe()
        )
    }

    async fn disturb(&mut self, t_s: f64) {
        while self.events.front().is_some_and(|next| next.at_s <= t_s) {
            let next = self.events.pop_front().expect("a due event");

            match next.what {
                Disturbance::Restart {
                    outage_s,
                    late_sync_s,
                }
                | Disturbance::PowerCut {
                    outage_s,
                    late_sync_s,
                } => {
                    if self.down.is_some() {
                        self.report.skipped_cuts += 1;
                        continue;
                    }
                    let power = matches!(next.what, Disturbance::PowerCut { .. });
                    self.sim.crash(power);
                    self.down = Some(Down {
                        until_s: t_s + outage_s,
                        power,
                        late_sync_s,
                    });
                    if power {
                        self.report.power_cuts += 1;
                    } else {
                        self.report.restarts += 1;
                    }
                }
                Disturbance::SntpLoss { duration_s } => {
                    self.sim.clock.lose_sync();
                    self.sync_at_s = self.sync_at_s.max(t_s + duration_s);
                    self.report.sntp_losses += 1;
                }
                Disturbance::LevelOutage { duration_s } => {
                    self.sim.water().outage = true;
                    self.outage_until_s = Some(t_s + duration_s);
                    self.report.level_outages += 1;
                }
            }
        }

        if let Some(down) = self.down.filter(|down| t_s >= down.until_s) {
            if let Some(late_s) = down.late_sync_s {
                self.sync_at_s = self.sync_at_s.max(t_s + late_s);
            }
            self.sim.boot_up(t_s >= self.sync_at_s).await;
            self.down = None;
            self.dry_grace_until_s = t_s + DRY_RUN_GRACE_S;
        }

        if self.down.is_none() && !self.sim.clock.is_synced() && t_s >= self.sync_at_s {
            self.sim.clock.sync();
        }

        if self.outage_until_s.is_some_and(|until| t_s >= until) {
            self.sim.water().outage = false;
            self.outage_until_s = None;
            self.dry_grace_until_s = t_s + DRY_RUN_GRACE_S;
        }
    }

    fn tend(&mut self, t: u64) {
        let of_day = t % DAY_S;
        let day = t / DAY_S;

        if of_day == 0 {
            self.evaporation_l_per_day = self
                .rng
                .range(EVAPORATION_L_PER_DAY.0, EVAPORATION_L_PER_DAY.1);
        }

        if day % REFILL_EVERY_DAYS == REFILL_DAY_OFFSET && of_day == REFILL_HOUR * 3600 {
            let capacity = Layout::REAL.reservoir_l;
            let added = self.sim.water().refill_reservoir(capacity);
            self.report.reservoir_refills += 1;
            self.report.supplied_l += added;
        }

        let litres = evaporation_l_per_s(self.evaporation_l_per_day, of_day / 3600);
        let mut water = self.sim.water();
        let at_weir = water.tank_l >= water.layout.tank_weir_l - LEVEL_EPS;
        if water.pump_on(PumpId::Return) && at_weir {
            water.evaporate(litres);
        } else {
            water.evaporate_tank(litres);
        }
        water.makeup_mg_l = if self.status.operation.kind == Some(OperationKind::WaterChange) {
            NEW_WATER_MG_L
        } else {
            0.0
        };
    }

    fn check(&mut self) {
        let t_s = self.t_s();
        let label = self.label();
        let water = self.sim.water();
        let layout = water.layout;

        assert_eq!(water.spilled_l, 0.0, "sump spill at {label}");
        assert_eq!(water.tank_spilled_l, 0.0, "tank spill at {label}");
        assert_eq!(water.makeup_dry_s, 0.0, "makeup ran dry at {label}");
        assert_eq!(water.drain_dry_s, 0.0, "drain ran dry at {label}");

        let dry = water.return_dry_s - self.last_return_dry_s;
        if dry > 0.0 {
            let fail_safe = self.down.is_some_and(|down| !down.power);
            let allowed = fail_safe || water.outage || t_s <= self.dry_grace_until_s;
            assert!(
                allowed,
                "return ran dry {dry:.2} s outside a window at {label}"
            );
            self.report.allowed_dry_s += dry;
        }
        self.last_return_dry_s = water.return_dry_s;

        assert!(
            water.tank_l >= layout.tank_weir_l - TANK_DEFICIT_MAX_L
                && water.tank_l <= layout.tank_weir_l + LEVEL_EPS,
            "tank out of band at {label}"
        );
        assert!(
            water.return_level_cm() >= layout.return_intake_cm - LEVEL_EPS,
            "return chamber below the intake at {label}"
        );
        let balance = water.balance_error_l().abs();
        assert!(
            balance < BALANCE_TOL_L,
            "water balance off by {balance} L at {label}"
        );

        let report = &mut self.report;
        report.min_tank_l = report.min_tank_l.min(water.tank_l);
        report.min_return_cm = report.min_return_cm.min(water.return_level_cm());
        report.min_concentration = report.min_concentration.min(water.concentration_mg_l());
        report.max_concentration = report.max_concentration.max(water.concentration_mg_l());
        report.max_balance_error_l = report.max_balance_error_l.max(balance);
    }

    fn observe(&mut self, t: u64) {
        if self.down.is_some() {
            return;
        }

        let status = self.sim.service.status();

        if status.operation.kind == Some(OperationKind::WaterChange) && self.change.is_none() {
            self.change = Some(ChangeStart {
                drained_l: self.sim.water().drained_l,
            });
        }

        if status.last_water_change != self.status.last_water_change {
            if let Some(record) = status.last_water_change {
                self.finish_change(record);
            }
        }
        if status.last_top_off != self.status.last_top_off {
            self.report
                .top_offs
                .extend(status.last_top_off.map(|record| record.result));
        }
        if status.last_skipped != self.status.last_skipped && status.last_skipped.is_some() {
            self.report.skipped_windows += 1;
        }

        if t % OPERATOR_PERIOD_S == 0 && status.operation.state == OperationState::Suspended {
            let kind = status.operation.kind.expect("a suspended operation");
            if self.sim.service.resume(kind).is_ok() {
                self.report.operator_resumes += 1;
            } else {
                self.sim.service.cancel(kind).expect("cancelled");
                self.report.operator_cancels += 1;
            }
        }

        self.status = status;
    }

    fn finish_change(&mut self, record: OperationRecord) {
        let label = self.label();
        let start = self.change.take();
        self.report.water_changes.push(record);

        if record.result != OperationResult::SumpFull {
            return;
        }

        let water = self.sim.water();
        let flow = water.layout.drain_l_per_s();
        let drained = water.drained_l - start.expect("a tracked water change").drained_l;
        let planned = DRAIN_S * flow;
        let extra = if record.resumed {
            CHECKPOINT_S * flow
        } else {
            0.0
        };
        assert!(
            drained >= planned - EXCHANGE_TOL_L && drained <= planned + extra + EXCHANGE_TOL_L,
            "{record:?} drained {drained:.2} L, planned {planned:.2} L at {label}"
        );
        assert!(
            water.deficit_l().abs() <= EXCHANGE_TOL_L,
            "{record:?} left the system {:.2} L short at {label}",
            water.deficit_l()
        );
    }
}

async fn soak(days: u64) -> Report {
    let wall = std::time::Instant::now();
    let options = Options {
        epoch: START_EPOCH,
        ..Options::COARSE.manual()
    };
    let sim = Sim::start_with(Layout::REAL, options).await;
    sim.water().dissolve(NEW_WATER_MG_L);
    let patch: PumpSettingsPatch = serde_json::from_str(SETTINGS).expect("settings json");
    sim.service
        .apply_settings(patch)
        .expect("settings accepted");

    let mut rng = Lcg::new(SEED);
    let events = schedule(days, &mut rng);
    let status = sim.service.status();
    let concentration = sim.water().concentration_mg_l();
    let mut soak = Soak {
        sim,
        rng,
        events,
        down: None,
        sync_at_s: 0.0,
        outage_until_s: None,
        dry_grace_until_s: 0.0,
        last_return_dry_s: 0.0,
        evaporation_l_per_day: 0.0,
        change: None,
        status,
        report: Report {
            min_tank_l: f64::MAX,
            min_return_cm: f64::MAX,
            min_concentration: concentration,
            max_concentration: concentration,
            ..Report::default()
        },
    };

    for t in 0..days * DAY_S {
        soak.disturb(t as f64).await;
        soak.tend(t);
        soak.sim.step().await;
        soak.check();
        soak.observe(t + 1);
    }

    let report = soak.report;
    let water = soak.sim.water();
    let drift = water.concentration_mg_l() / NEW_WATER_MG_L - 1.0;
    let completed = report
        .water_changes
        .iter()
        .filter(|record| record.result == OperationResult::SumpFull)
        .count();
    let return_starts = water.count(Event::Pump(PumpId::Return, true));

    println!(
        "soak {days} d (seed {SEED:#x}) in {:.2} s wall: {} restarts, {} power cuts \
         ({} skipped while down), {} SNTP losses, {} level outages; operator {} resumes, \
         {} cancels, {} reservoir refills ({:.1} L)",
        wall.elapsed().as_secs_f64(),
        report.restarts,
        report.power_cuts,
        report.skipped_cuts,
        report.sntp_losses,
        report.level_outages,
        report.operator_resumes,
        report.operator_cancels,
        report.reservoir_refills,
        report.supplied_l,
    );
    println!(
        "  water changes {:?}",
        report
            .water_changes
            .iter()
            .map(|record| (record.result, record.resumed))
            .collect::<Vec<_>>()
    );
    println!(
        "  top-offs {} ({} sump_full), {} skipped windows, {return_starts} return starts, \
         spilled {:.2} L, return dry {:.1} s (all in windows), makeup dry {:.1} s, \
         evaporated {:.1} L, added {:.1} L, drained {:.1} L",
        report.top_offs.len(),
        report
            .top_offs
            .iter()
            .filter(|result| **result == OperationResult::SumpFull)
            .count(),
        report.skipped_windows,
        water.spilled_l + water.tank_spilled_l,
        report.allowed_dry_s,
        water.makeup_dry_s,
        water.evaporated_l,
        water.added_l,
        water.drained_l,
    );
    println!(
        "  tank min {:.1} L, return chamber min {:.2} cm, concentration {:.1}..{:.1} mg/L, \
         end drift {:+.2} %, max balance error {:.1e} L, reservoir {:.1} L",
        report.min_tank_l,
        report.min_return_cm,
        report.min_concentration,
        report.max_concentration,
        drift * 100.0,
        report.max_balance_error_l,
        water.reservoir_l,
    );

    assert!(
        drift.abs() <= CONCENTRATION_BAND,
        "concentration drifted {:+.2} %",
        drift * 100.0
    );
    assert!(
        completed as u64 >= days / 7,
        "{completed} completed water changes in {days} days"
    );
    drop(water);
    for (id, recorded) in soak.sim.relays.switch_ons() {
        if matches!(id, PumpId::Drain | PumpId::Makeup) {
            assert!(recorded, "{id:?} switched on before pump_op was stored");
        }
    }
    report
}

#[tokio::test(start_paused = true)]
async fn soak_one_week() {
    LocalSet::new()
        .run_until(async {
            let report = soak(7).await;
            assert!(report.restarts + report.power_cuts > 0);
            assert!(report.sntp_losses > 0 && report.level_outages > 0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
#[ignore = "month-long soak, run with make test-soak"]
async fn soak_one_month() {
    LocalSet::new()
        .run_until(async {
            let report = soak(30).await;
            assert!(report.restarts + report.power_cuts > 0);
        })
        .await;
}
