#![recursion_limit = "256"]

mod common;
mod sim;

use std::time::Duration;

use tokio::task::LocalSet;

use aquarium_core::domain::pump::{
    Checkpoint, OperationKind, OperationRecord, OperationResult, OperationState, Phase, PumpId,
    PumpSettingsPatch, WaterChangePlan,
};
use aquarium_core::ports::inbound::PumpControl;
use aquarium_core::ports::outbound::PumpRecord;

use sim::{Event, Layout, Options, Sim, StoreOp};

const RESERVOIR_L: f64 = 150.0;
const PLAN: WaterChangePlan = WaterChangePlan {
    drain_s: 60,
    settle_s: 30,
    refill_s: 600,
};
const TOP_OFF_S: u32 = 300;
const TOP_OFF_DEFICIT_L: f64 = 4.0;
const CHECKPOINT_S: f64 = 10.0;

const SHORT_OUTAGE: Duration = Duration::from_secs(5);
const LONG_OUTAGE: Duration = Duration::from_secs(700);
const SNTP_DELAY: Duration = Duration::from_secs(30);
const SETTLE_AFTER: Duration = Duration::from_secs(120);
const OFFSETS_S: [f64; 7] = [0.0, 0.5, 5.0, 9.5, 10.5, 20.0, 29.5];
const CANCEL_AFTER_S: f64 = 5.0;
const CUT_AFTER_CANCEL_S: f64 = 30.0;
const LIMIT_S: f64 = 5000.0;

const LITRES_TOL: f64 = 0.6;
const BALANCE_TOL: f64 = 1e-6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    WaterChange,
    TopOff,
}

impl Kind {
    const fn operation(self) -> OperationKind {
        match self {
            Self::WaterChange => OperationKind::WaterChange,
            Self::TopOff => OperationKind::TopOff,
        }
    }

    const fn states(self) -> &'static [OperationState] {
        match self {
            Self::WaterChange => &[
                OperationState::Draining,
                OperationState::Settling,
                OperationState::Refilling,
            ],
            Self::TopOff => &[OperationState::Refilling],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Boot {
    Synced,
    LateSync,
}

#[derive(Debug, Clone, Copy)]
enum Trigger {
    None,
    Phase {
        state: OperationState,
        offset_s: f64,
        outage: Duration,
        boot: Boot,
    },
    Freeze {
        op: usize,
    },
    Cancel {
        state: OperationState,
        freeze: Option<usize>,
    },
}

#[derive(Debug)]
struct Cut {
    t_s: f64,
    memory: OperationState,
    persisted: Option<Checkpoint>,
    dropped: Option<StoreOp>,
    booted_into: OperationState,
}

#[derive(Debug)]
struct Cancelled {
    t_s: f64,
    acknowledged: bool,
    durable: bool,
}

struct Run {
    sim: Sim,
    start_op: usize,
    cuts: Vec<Cut>,
    cancelled: Option<Cancelled>,
    resumes: usize,
    suspended: bool,
}

fn record(sim: &Sim, kind: Kind) -> Option<OperationRecord> {
    let status = sim.service.status();
    match kind {
        Kind::WaterChange => status.last_water_change,
        Kind::TopOff => status.last_top_off,
    }
}

fn persisted(sim: &Sim) -> Option<Checkpoint> {
    sim.store
        .get(PumpRecord::Checkpoint)
        .and_then(|bytes| serde_json::from_slice::<Option<Checkpoint>>(&bytes).ok())
        .flatten()
}

fn phase_state(checkpoint: &Checkpoint) -> OperationState {
    OperationState::from(checkpoint.operation.phase)
}

async fn cut(sim: &mut Sim, outage: Duration, boot: Boot) -> Cut {
    let memory = sim.service.status().operation.state;
    let persisted = persisted(sim);
    let dropped = sim.store.ops().into_iter().find(|op| !op.applied);
    let t_s = sim.water().t_s;

    sim.crash(true);
    sim.run_for(outage).await;
    sim.boot_up(boot == Boot::Synced).await;
    let booted_into = sim.service.status().operation.state;

    if boot == Boot::LateSync {
        sim.run_for(SNTP_DELAY).await;
        sim.clock.sync();
    }

    Cut {
        t_s,
        memory,
        persisted,
        dropped,
        booted_into,
    }
}

async fn run(kind: Kind, trigger: Trigger) -> Run {
    let mut sim = Sim::start_with(
        Layout::REAL.with_reservoir(RESERVOIR_L),
        Options::DEFAULT.manual(),
    )
    .await;
    let patch: PumpSettingsPatch = serde_json::from_str(
        r#"{"flow_lph":{"makeup":600,"drain":1500},"refill_stop_sensor":"sump_return_level"}"#,
    )
    .expect("settings json");
    sim.service
        .apply_settings(patch)
        .expect("settings accepted");

    if kind == Kind::TopOff {
        sim.water().evaporate(TOP_OFF_DEFICIT_L);
    }
    sim.run_for(Duration::from_secs(2)).await;

    let start_op = sim.store.op_count();
    if let Trigger::Freeze { op } = trigger {
        sim.store.fail_after(op);
    }
    match kind {
        Kind::WaterChange => sim.service.start_water_change(PLAN),
        Kind::TopOff => sim.service.start_top_off(TOP_OFF_S),
    }
    .expect("operation started");

    let mut pending = !matches!(trigger, Trigger::None);
    let mut cuts = Vec::new();
    let mut cancelled = None;
    let mut resumes = 0;
    let mut suspended = false;
    let mut entered = (sim.service.status().operation.state, sim.water().t_s);
    let mut seen = vec![entered.0];
    let mut previous = entered.0;
    let mut handled_cuts = 0;

    loop {
        let t_s = sim.water().t_s;
        assert!(
            t_s < LIMIT_S,
            "run did not settle: {}",
            sim.water().describe()
        );

        if sim.store.frozen() {
            cuts.push(cut(&mut sim, SHORT_OUTAGE, Boot::Synced).await);
            pending = false;
        }

        if cuts.len() > handled_cuts {
            handled_cuts = cuts.len();
            previous = cuts[handled_cuts - 1].booted_into;
            entered = (previous, sim.water().t_s);
        }

        let state = sim.service.status().operation.state;
        match trigger {
            Trigger::Phase {
                state: target,
                offset_s,
                outage,
                boot,
            } if pending => {
                let reached = state == target && t_s - entered.1 >= offset_s;
                let passed = seen.contains(&target) && state != target;
                if reached || passed {
                    cuts.push(cut(&mut sim, outage, boot).await);
                    pending = false;
                }
            }
            Trigger::Cancel {
                state: target,
                freeze,
            } if pending => {
                if cancelled.is_none() && state == target && t_s - entered.1 >= CANCEL_AFTER_S {
                    let ops = sim.store.op_count();
                    if let Some(extra) = freeze {
                        sim.store.fail_after(ops + extra);
                    }
                    sim.service.cancel(kind.operation());
                    cancelled = Some(Cancelled {
                        t_s,
                        acknowledged: !sim.store.frozen(),
                        durable: !sim.store.holds_checkpoint(),
                    });
                }

                if let Some(done) = &cancelled {
                    if !sim.store.frozen() && t_s - done.t_s >= CUT_AFTER_CANCEL_S {
                        cuts.push(cut(&mut sim, SHORT_OUTAGE, Boot::Synced).await);
                        pending = false;
                    }
                }
            }
            _ => {}
        }

        sim.step().await;

        let state = sim.service.status().operation.state;
        if state != previous {
            if previous == OperationState::Resuming && state != OperationState::Suspended {
                resumes += 1;
            }
            entered = (state, sim.water().t_s);
            seen.push(state);
            previous = state;
        }

        if state == OperationState::Suspended {
            suspended = true;
            sim.service.cancel(kind.operation());
        }

        if !pending && !sim.store.frozen() && state == OperationState::Idle {
            sim.run_for(SETTLE_AFTER).await;
            break;
        }
    }

    Run {
        sim,
        start_op,
        cuts,
        cancelled,
        resumes,
        suspended,
    }
}

fn assert_invariants(run: &Run, kind: Kind, label: &str) {
    let sim = &run.sim;
    let water = sim.water();
    let layout = water.layout;
    let status = sim.service.status();

    assert_eq!(water.spilled_l, 0.0, "{label}: {}", water.describe());
    assert_eq!(water.tank_spilled_l, 0.0, "{label}");
    assert_eq!(water.makeup_dry_s, 0.0, "{label}");
    assert_eq!(water.return_dry_s, 0.0, "{label}: {}", water.describe());
    assert!(
        water.balance_error_l().abs() < BALANCE_TOL,
        "{label}: balance off by {}",
        water.balance_error_l()
    );
    assert_eq!(status.operation.state, OperationState::Idle, "{label}");
    assert!(
        !water.pump_on(PumpId::Drain) && !water.pump_on(PumpId::Makeup),
        "{label}"
    );
    assert!(
        run.resumes <= run.cuts.len(),
        "{label}: {} resumes",
        run.resumes
    );

    let max_drain_l = (f64::from(PLAN.drain_s) + CHECKPOINT_S) * layout.drain_l_per_s();
    match kind {
        Kind::WaterChange => assert!(
            water.drained_l <= max_drain_l + LITRES_TOL,
            "{label}: drained {:.2} L",
            water.drained_l
        ),
        Kind::TopOff => assert_eq!(water.drained_l, 0.0, "{label}"),
    }

    if let Some(last) = record(sim, kind).filter(|last| last.result == OperationResult::SumpFull) {
        assert!(
            water.deficit_l().abs() < LITRES_TOL,
            "{label}: {last:?} left the system {:.2} L short",
            water.deficit_l()
        );
    }

    for cut in &run.cuts {
        let drain_after = water.count_after(Event::Pump(PumpId::Drain, true), cut.t_s);
        let makeup_after = water.count_after(Event::Pump(PumpId::Makeup, true), cut.t_s);

        match &cut.persisted {
            None => {
                assert_eq!(cut.booted_into, OperationState::Idle, "{label}: {cut:?}");
                assert_eq!(
                    drain_after + makeup_after,
                    0,
                    "{label}: resumed nothing persisted"
                );
            }
            Some(checkpoint) => {
                assert_eq!(
                    cut.booted_into,
                    OperationState::Resuming,
                    "{label}: {cut:?}"
                );
                if checkpoint.operation.phase != Phase::Draining {
                    assert_eq!(
                        drain_after, 0,
                        "{label}: drain resumed after its phase: {cut:?}"
                    );
                }
            }
        }

        let lagging = match (&cut.persisted, cut.memory) {
            (None, OperationState::Idle) => false,
            (Some(checkpoint), memory) => phase_state(checkpoint) != memory,
            (None, _) => true,
        };
        if lagging {
            let dropped = cut.dropped.expect("only a lost write lets the store lag");
            assert_eq!(dropped.record, PumpRecord::Checkpoint, "{label}: {cut:?}");
        }
    }

    if let Some(cancel) = &run.cancelled {
        assert!(
            cancel.durable || !cancel.acknowledged,
            "{label}: {cancel:?}"
        );
    }

    if let Some(cancel) = run.cancelled.as_ref().filter(|cancel| cancel.durable) {
        assert_eq!(
            water.count_after(Event::Pump(PumpId::Drain, true), cancel.t_s),
            0,
            "{label}: drain ran after an acknowledged cancel"
        );
        assert_eq!(
            water.count_after(Event::Pump(PumpId::Makeup, true), cancel.t_s),
            0,
            "{label}: makeup ran after an acknowledged cancel"
        );
        assert!(
            run.cuts.iter().all(|cut| cut.persisted.is_none()),
            "{label}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn power_cut_at_every_phase_offset_resumes_or_suspends_safely() {
    LocalSet::new()
        .run_until(async {
            let mut cases = 0;
            let mut resumed = 0;
            let mut suspended = 0;

            for kind in [Kind::WaterChange, Kind::TopOff] {
                for &state in kind.states() {
                    for offset_s in OFFSETS_S {
                        for (outage, boot) in [
                            (SHORT_OUTAGE, Boot::Synced),
                            (SHORT_OUTAGE, Boot::LateSync),
                            (LONG_OUTAGE, Boot::Synced),
                        ] {
                            let label = format!("{kind:?} {state:?}+{offset_s}s {outage:?} {boot:?}");
                            let run = run(
                                kind,
                                Trigger::Phase {
                                    state,
                                    offset_s,
                                    outage,
                                    boot,
                                },
                            )
                            .await;

                            assert_eq!(run.cuts.len(), 1, "{label}");
                            let cut = &run.cuts[0];
                            assert!(cut.dropped.is_none(), "{label}");
                            assert_eq!(
                                cut.persisted.as_ref().map(phase_state),
                                (cut.memory != OperationState::Idle).then_some(cut.memory),
                                "{label}: the store lags memory without a lost write"
                            );
                            if cut.persisted.is_some() {
                                assert_eq!(
                                    run.suspended,
                                    outage == LONG_OUTAGE,
                                    "{label}: resume verdict"
                                );
                                assert_eq!(run.resumes, usize::from(outage != LONG_OUTAGE), "{label}");
                                let last = record(&run.sim, kind).expect("a record");
                                if run.suspended {
                                    assert_eq!(last.result, OperationResult::InterruptedByReboot);
                                } else {
                                    assert!(last.resumed, "{label}: {last:?}");
                                    assert_eq!(last.result, OperationResult::SumpFull, "{label}");
                                }
                            }
                            assert_invariants(&run, kind, &label);

                            cases += 1;
                            resumed += run.resumes;
                            suspended += usize::from(run.suspended);
                        }
                    }
                }
            }

            println!("power cuts at phase offsets: {cases} cases, {resumed} resumed, {suspended} suspended");
            assert_eq!(cases, 4 * OFFSETS_S.len() * 3);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn power_cut_during_any_store_write_never_resurrects_a_finished_operation() {
    LocalSet::new()
        .run_until(async {
            for kind in [Kind::WaterChange, Kind::TopOff] {
                let reference = run(kind, Trigger::None).await;
                assert_invariants(&reference, kind, "reference");
                let script = reference.sim.store.ops();
                let mut lagged = 0;
                let mut resurrected_by_lost_erase = 0;

                for op in reference.start_op..script.len() {
                    let label = format!("{kind:?} write #{op} {:?}", script[op]);
                    let run = run(kind, Trigger::Freeze { op }).await;

                    assert_eq!(run.cuts.len(), 1, "{label}");
                    let cut = &run.cuts[0];
                    let dropped = cut.dropped.expect("a dropped write");
                    assert_eq!(
                        (dropped.record, dropped.erase),
                        (script[op].record, script[op].erase),
                        "{label}: the replay diverged"
                    );
                    assert_invariants(&run, kind, &label);

                    if cut.persisted.as_ref().map(phase_state) != Some(cut.memory)
                        && cut.memory != OperationState::Idle
                    {
                        lagged += 1;
                    }
                    if cut.memory == OperationState::Idle && cut.persisted.is_some() {
                        assert!(dropped.erase || dropped.tombstone, "{label}: {cut:?}");
                        resurrected_by_lost_erase += 1;
                    }
                }

                println!(
                    "{kind:?}: {} writes swept, {lagged} cuts left the store a phase behind, \
                     {resurrected_by_lost_erase} lost finishing erases",
                    script.len() - reference.start_op
                );
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn power_cut_around_a_cancel_honours_only_acknowledged_cancels() {
    LocalSet::new()
        .run_until(async {
            for kind in [Kind::WaterChange, Kind::TopOff] {
                for &state in kind.states() {
                    for freeze in [Some(0), Some(1), None] {
                        let label = format!("{kind:?} cancel in {state:?} freeze {freeze:?}");
                        let run = run(kind, Trigger::Cancel { state, freeze }).await;
                        let cancel = run.cancelled.as_ref().expect("cancelled");

                        assert_eq!(cancel.acknowledged, freeze.is_none(), "{label}");
                        assert_eq!(cancel.durable, freeze != Some(0), "{label}");
                        assert_eq!(run.cuts.len(), 1, "{label}");
                        let cut = &run.cuts[0];
                        if cancel.durable {
                            assert_eq!(cut.booted_into, OperationState::Idle, "{label}");
                            assert_eq!(run.resumes, 0, "{label}");
                        } else {
                            assert_eq!(cut.booted_into, OperationState::Resuming, "{label}");
                            assert_eq!(run.resumes, 1, "{label}");
                        }
                        if freeze == Some(1) {
                            assert_ne!(
                                record(&run.sim, kind).map(|last| last.result),
                                Some(OperationResult::Aborted),
                                "{label}: the lost history write"
                            );
                        }
                        assert_invariants(&run, kind, &label);
                    }
                }
            }
        })
        .await;
}

fn resumed_checkpoint(record: PumpRecord, bytes: &[u8]) -> bool {
    record == PumpRecord::Checkpoint
        && serde_json::from_slice::<Option<Checkpoint>>(bytes)
            .ok()
            .flatten()
            .is_some_and(|checkpoint| checkpoint.operation.resumed)
}

#[tokio::test(start_paused = true)]
#[ignore = "production bug: the resumed checkpoint is written after the relays switch, so a \
            power cut on that write auto-resumes the same operation again"]
async fn power_cut_on_the_resume_checkpoint_write_still_resumes_at_most_once() {
    LocalSet::new()
        .run_until(async {
            let mut sim = Sim::start_with(
                Layout::REAL.with_reservoir(RESERVOIR_L),
                Options::DEFAULT.manual(),
            )
            .await;
            sim.run_for(Duration::from_secs(2)).await;
            sim.service
                .start_water_change(PLAN)
                .expect("water change started");
            sim.run_until(Duration::from_secs(120), "20 s of drain", |status, _| {
                status.operation.drain_ran_s >= 20
            })
            .await;

            sim.store.fail_on(resumed_checkpoint);
            sim.power_cut(SHORT_OUTAGE, true).await;

            let mut auto_resumes = 0;
            let first_cut = sim.water().t_s;
            loop {
                sim.step().await;
                let state = sim.service.status().operation.state;

                if sim.store.frozen() {
                    auto_resumes += 1;
                    sim.power_cut(SHORT_OUTAGE, true).await;
                } else if state == OperationState::Suspended {
                    break;
                }
                assert!(sim.water().t_s - first_cut < 2000.0, "no verdict");
            }

            assert_eq!(sim.water().spilled_l, 0.0, "{}", sim.water().describe());
            assert!(
                auto_resumes <= 1,
                "{auto_resumes} automatic resumes of one operation over {:.0} s of power cuts \
                 at the resume checkpoint write (persisted resumed flag never lands)",
                sim.water().t_s - first_cut
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn probe_late_refill_cut() {
    LocalSet::new()
        .run_until(async {
            let mut sim = Sim::start_with(
                Layout::REAL.with_reservoir(RESERVOIR_L),
                Options::DEFAULT.manual(),
            )
            .await;
            sim.run_for(Duration::from_secs(2)).await;
            sim.service.start_water_change(PLAN).expect("started");
            sim.run_until(Duration::from_secs(900), "late refill", |status, water| {
                status.operation.state == OperationState::Refilling
                    && status.operation.refill_ran_s > 0
                    && water.deficit_l() < 1.5
            })
            .await;
            println!(
                "before cut: {} deficit {:.2}",
                sim.water().describe(),
                sim.water().deficit_l()
            );
            sim.power_cut(SHORT_OUTAGE, true).await;
            println!(
                "after boot: {} deficit {:.2} {:?}",
                sim.water().describe(),
                sim.water().deficit_l(),
                sim.service.status().operation
            );
            sim.run_until(Duration::from_secs(900), "idle", |status, _| {
                status.operation.state == OperationState::Idle
            })
            .await;
            sim.run_for(Duration::from_secs(120)).await;
            println!(
                "end: {} deficit {:.2} {:?}",
                sim.water().describe(),
                sim.water().deficit_l(),
                sim.service.status().last_water_change
            );
        })
        .await;
}
