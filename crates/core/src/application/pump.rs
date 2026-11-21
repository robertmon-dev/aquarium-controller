use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::{mpsc, watch, Notify};
use tokio::time::{Instant, MissedTickBehavior};

use crate::domain::cron::{CivilTime, CronExpr, Firing};
use crate::domain::pump::{
    decide, litres, resume_verdict, validate_override, validate_top_off, Blocker, ChannelDecision,
    ChannelFault, ChannelMode, ChannelStatus, ChannelsStatus, Checkpoint, DecisionInput,
    LastOperations, LevelConditions, Operation, OperationEvent, OperationKind, OperationPlan,
    OperationResult, OperationState, OperationStatus, OverrideError, Phase, PumpId, PumpSettings,
    PumpSettingsPatch, PumpsStatus, PumpsTelemetry, ResumeVerdict, RuntimeCounters, SkipReason,
    SkippedWindow, Step, StoredOverride, WaterChangePlan, STARVED_CLEAR_S, STARVED_LOCKOUTS,
    STARVED_WINDOW_S,
};
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot};
use crate::ports::inbound::{PumpControl, PumpSettingsError, StartError};
use crate::ports::outbound::{Clock, PumpOutputs, PumpRecord, PumpStatusSource, PumpStore};

#[derive(Debug, Clone, Copy)]
pub struct PumpTiming {
    pub return_restart_delay: Duration,
    pub level_stale: Duration,
    pub tick: Duration,
    pub checkpoint_interval: Duration,
    pub runtime_flush: Duration,
}

impl PumpTiming {
    pub const DEFAULT: Self = Self {
        return_restart_delay: Duration::from_secs(30),
        level_stale: Duration::from_secs(1),
        tick: Duration::from_secs(1),
        checkpoint_interval: Duration::from_secs(3),
        runtime_flush: Duration::from_secs(600),
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expiry {
    Never,
    At(Instant),
    Epoch(u64),
    Timed { at: u64, deadline: Instant },
}

impl Expiry {
    fn stored(self) -> Option<StoredOverride> {
        match self {
            Self::Never => Some(StoredOverride { expires_at: None }),
            Self::Epoch(at) | Self::Timed { at, .. } => Some(StoredOverride {
                expires_at: Some(at),
            }),
            Self::At(_) => None,
        }
    }

    fn remaining_s(self, now: Instant, epoch: Option<u64>) -> Option<u32> {
        match self {
            Self::Never => None,
            Self::At(at) | Self::Timed { deadline: at, .. } => {
                Some(at.saturating_duration_since(now).as_millis().div_ceil(1000) as u32)
            }
            Self::Epoch(at) => epoch.map(|epoch| at.saturating_sub(epoch) as u32),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Output {
    on: bool,
    fault: Option<ChannelFault>,
    blocked_by: Option<Blocker>,
}

#[derive(Debug, Clone)]
enum Activity {
    Idle,
    Running(Operation),
    Resuming(Checkpoint),
    Suspended(Operation),
}

struct Window<T> {
    cron: CronExpr,
    plan: T,
    fired: Option<Firing>,
}

impl<T> Window<T> {
    fn due(&mut self, civil: CivilTime, epoch: u64) -> bool {
        let repeated = self.fired.is_some_and(|fired| fired.repeats(civil, epoch));

        if !self.cron.matches(civil) || repeated {
            return false;
        }

        self.fired = Some(Firing::new(civil, epoch));
        true
    }
}

struct Schedule {
    top_off: Vec<Window<u32>>,
    water_change: Vec<Window<WaterChangePlan>>,
}

fn windows<W, T>(
    windows: &[W],
    previous: &[Window<T>],
    parts: impl Fn(&W) -> (&str, T),
) -> Vec<Window<T>> {
    windows
        .iter()
        .filter_map(|window| {
            let (cron, plan) = parts(window);
            CronExpr::parse(cron).ok().map(|cron| (cron, plan))
        })
        .enumerate()
        .map(|(index, (cron, plan))| Window {
            cron,
            plan,
            fired: previous.get(index).and_then(|window| window.fired),
        })
        .collect()
}

impl Schedule {
    fn compile(settings: &PumpSettings, previous: Option<&Self>) -> Self {
        Self {
            top_off: windows(
                &settings.top_off,
                previous.map_or(&[], |schedule| &schedule.top_off),
                |window| (window.cron.as_str(), window.duration_s),
            ),
            water_change: windows(
                &settings.water_change,
                previous.map_or(&[], |schedule| &schedule.water_change),
                |window| (window.cron.as_str(), window.plan()),
            ),
        }
    }
}

#[derive(Default)]
struct Pending {
    checkpoint: bool,
    erase_checkpoint: bool,
    last: bool,
    overrides: bool,
    events: Vec<OperationEvent>,
}

struct State {
    settings: PumpSettings,
    schedule: Schedule,
    overrides: [Option<Expiry>; 2],
    pending: Pending,
    activity: Activity,
    fresh_operation: bool,
    levels: Option<LevelConditions>,
    outputs: [Option<Output>; 4],
    runtime_ms: [u64; 4],
    reset_at: [Option<u64>; 4],
    reset_instant: [Option<Instant>; 4],
    last: LastOperations,
    last_skipped: Option<SkippedWindow>,
    last_fired_minute: Option<u64>,
    resume_sync_epoch: Option<u64>,
    circulation_starved: bool,
}

impl State {
    fn live_override(&mut self, id: PumpId, now: Instant, epoch: Option<u64>) -> Option<Expiry> {
        let slot = id.override_slot()?;
        let expired = match self.overrides[slot]? {
            Expiry::Never => false,
            Expiry::At(at) | Expiry::Timed { deadline: at, .. } => now >= at,
            Expiry::Epoch(at) => epoch.is_some_and(|epoch| epoch >= at),
        };

        if expired {
            self.overrides[slot] = None;
            self.pending.overrides = true;
        }

        self.overrides[slot]
    }

    fn forced_off(&mut self, id: PumpId, now: Instant, epoch: Option<u64>) -> bool {
        match self.live_override(id, now, epoch) {
            None => false,
            Some(Expiry::Epoch(_)) if epoch.is_none() => id == PumpId::Uv,
            Some(_) => true,
        }
    }

    fn stored_overrides(&self) -> [Option<StoredOverride>; 2] {
        self.overrides.map(|slot| slot.and_then(Expiry::stored))
    }

    fn runtime(&self) -> RuntimeCounters {
        RuntimeCounters {
            runtime_s: self.runtime_ms.map(|ms| ms / 1000),
            reset_at: self.reset_at,
        }
    }

    fn channel(&mut self, id: PumpId, now: Instant, epoch: Option<u64>) -> ChannelStatus {
        let over = self.live_override(id, now, epoch);
        let in_force = self.forced_off(id, now, epoch);
        let output = self.outputs[id.index()];

        ChannelStatus {
            on: output.map_or(id.fail_safe_on(), |output| output.on),
            mode: if in_force {
                ChannelMode::ManualOff
            } else {
                ChannelMode::Auto
            },
            override_expires_in_s: over.and_then(|expiry| expiry.remaining_s(now, epoch)),
            fault: match output {
                Some(output) => output.fault,
                None => Some(ChannelFault::Unavailable),
            },
            blocked_by: output.and_then(|output| output.blocked_by),
            runtime_s: self.runtime_ms[id.index()] / 1000,
            runtime_reset_at: self.reset_at[id.index()],
        }
    }

    fn channels(&mut self, now: Instant, epoch: Option<u64>) -> ChannelsStatus {
        ChannelsStatus {
            return_pump: self.channel(PumpId::Return, now, epoch),
            uv: self.channel(PumpId::Uv, now, epoch),
            makeup: self.channel(PumpId::Makeup, now, epoch),
            drain: self.channel(PumpId::Drain, now, epoch),
        }
    }

    fn operation_status(&self) -> OperationStatus {
        let (state, operation) = match &self.activity {
            Activity::Idle => return OperationStatus::IDLE,
            Activity::Running(operation) => (OperationState::from(operation.phase), operation),
            Activity::Resuming(checkpoint) => (OperationState::Resuming, &checkpoint.operation),
            Activity::Suspended(operation) => (OperationState::Suspended, operation),
        };

        let flows = self.settings.flow_lph;
        let drain_ran_s = operation.ran_s(Phase::Draining);
        let refill_ran_s = operation.ran_s(Phase::Refilling);

        OperationStatus {
            kind: Some(operation.plan.kind),
            state,
            phase_remaining_s: matches!(self.activity, Activity::Running(_))
                .then(|| operation.remaining_s()),
            drain_ran_s,
            settle_ran_s: operation.ran_s(Phase::Settling),
            refill_ran_s,
            drained_l: litres(drain_ran_s, flows.drain),
            refilled_l: litres(refill_ran_s, flows.makeup),
        }
    }

    fn phase(&self) -> Option<Phase> {
        match &self.activity {
            Activity::Running(operation) => Some(operation.phase),
            _ => None,
        }
    }

    fn admit(
        &mut self,
        plan: &OperationPlan,
        now: Instant,
        epoch: Option<u64>,
    ) -> Result<(), StartError> {
        match (&self.activity, plan.kind) {
            (Activity::Idle, _) => {}
            (Activity::Running(current), OperationKind::WaterChange)
                if current.plan.kind == OperationKind::TopOff => {}
            _ => return Err(StartError::Conflict),
        }

        let levels = self
            .levels
            .ok_or(StartError::Blocked(Blocker::LevelsUnavailable))?;

        if plan.kind == OperationKind::TopOff {
            if levels.reservoir_empty {
                return Err(StartError::Blocked(Blocker::ReservoirEmpty));
            }

            if levels.tank_overflow {
                return Err(StartError::Blocked(Blocker::TankOverflow));
            }

            if levels.sump_full {
                return Err(StartError::Blocked(Blocker::SumpFull));
            }
        }

        if plan.kind == OperationKind::WaterChange && !levels.reservoir_ready {
            return Err(StartError::Blocked(Blocker::ReservoirLow));
        }

        if self.forced_off(PumpId::Return, now, epoch) {
            return Err(StartError::Blocked(Blocker::ReturnOff));
        }

        if plan.kind == OperationKind::WaterChange && self.water_change_too_recent(epoch) {
            return Err(StartError::Blocked(Blocker::RecentWaterChange));
        }

        Ok(())
    }

    fn water_change_too_recent(&self, epoch: Option<u64>) -> bool {
        let gap_s = u64::from(self.settings.min_water_change_gap_h) * 3600;
        let started_at = self.last.last_drain_started_at();

        match (epoch, started_at) {
            (Some(now), Some(started)) if gap_s > 0 => now < started.saturating_add(gap_s),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RecordOutcome {
    pub changed: bool,
    pub on_changed: bool,
}

pub struct Observation {
    phase: Option<Phase>,
    return_forced_off: bool,
    uv_forced_off: bool,
    changed: bool,
}

pub struct PumpService<S, C> {
    state: Mutex<State>,
    store: S,
    clock: C,
    notify: Arc<Notify>,
    events: Option<mpsc::UnboundedSender<OperationEvent>>,
}

fn load<S: PumpStore, T: DeserializeOwned>(store: &S, record: PumpRecord) -> Option<T> {
    let bytes = store.load(record)?;

    match serde_json::from_slice(&bytes) {
        Ok(value) => Some(value),
        Err(_) => {
            tracing::warn!(record = record.key(), "stored pump record is unreadable");
            None
        }
    }
}

impl<S: PumpStore, C: Clock> PumpService<S, C> {
    pub fn new(
        defaults: PumpSettings,
        store: S,
        clock: C,
        events: Option<mpsc::UnboundedSender<OperationEvent>>,
    ) -> Self {
        let settings = load::<S, PumpSettings>(&store, PumpRecord::Settings)
            .filter(|stored| stored.validate().is_ok())
            .unwrap_or(defaults);

        let overrides = load::<S, [Option<StoredOverride>; 2]>(&store, PumpRecord::Overrides)
            .unwrap_or_default()
            .map(|slot| slot.map(|stored| stored.expires_at.map_or(Expiry::Never, Expiry::Epoch)));

        let runtime = load::<S, RuntimeCounters>(&store, PumpRecord::Runtime).unwrap_or_default();
        let last =
            load::<S, LastOperations>(&store, PumpRecord::LastOperations).unwrap_or_default();
        let activity = load::<S, Option<Checkpoint>>(&store, PumpRecord::Checkpoint)
            .flatten()
            .map_or(Activity::Idle, Activity::Resuming);

        Self {
            state: Mutex::new(State {
                schedule: Schedule::compile(&settings, None),
                settings,
                overrides,
                pending: Pending::default(),
                activity,
                fresh_operation: false,
                levels: None,
                outputs: [None; 4],
                runtime_ms: runtime.runtime_s.map(|seconds| seconds * 1000),
                reset_at: runtime.reset_at,
                reset_instant: [None; 4],
                last,
                last_skipped: None,
                last_fired_minute: None,
                resume_sync_epoch: None,
                circulation_starved: false,
            }),
            store,
            clock,
            notify: Arc::new(Notify::new()),
            events,
        }
    }

    pub fn refill_stop_sensor(&self) -> Option<WaterLevelRole> {
        self.lock().settings.refill_stop_sensor
    }

    pub fn change_signal(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    pub fn observe(
        &self,
        levels: Option<LevelConditions>,
        elapsed: Duration,
        now: Instant,
    ) -> Observation {
        let observation = self.observe_deferred(levels, elapsed, now);
        self.persist_pending();
        observation
    }

    fn observe_deferred(
        &self,
        levels: Option<LevelConditions>,
        elapsed: Duration,
        now: Instant,
    ) -> Observation {
        let epoch = self.clock.epoch_seconds();
        let mut state = self.lock();
        state.levels = levels;

        let elapsed_ms = elapsed.as_millis() as u64;
        let mut wall_elapsed_ms = if std::mem::take(&mut state.fresh_operation) {
            0
        } else {
            elapsed_ms
        };
        let mut operation_elapsed_ms = wall_elapsed_ms;

        if let Activity::Running(operation) = &state.activity {
            let channel_ran = |id: PumpId| state.outputs[id.index()].is_some_and(|o| o.on);

            let idle_channel = match operation.phase {
                Phase::Draining => !channel_ran(PumpId::Drain),
                Phase::Refilling => !channel_ran(PumpId::Makeup),
                Phase::Settling => false,
            };

            if idle_channel {
                operation_elapsed_ms = 0;
            }
        }

        for id in PumpId::ALL {
            let credited_ms = match state.reset_instant[id.index()].take() {
                Some(reset) => {
                    elapsed_ms.min(now.saturating_duration_since(reset).as_millis() as u64)
                }
                None => elapsed_ms,
            };

            if state.outputs[id.index()].is_some_and(|output| output.on) {
                state.runtime_ms[id.index()] += credited_ms;
            }
        }

        let mut changed = false;

        if let Activity::Resuming(checkpoint) = &state.activity {
            let checkpoint = checkpoint.clone();

            if state.resume_sync_epoch.is_none() {
                state.resume_sync_epoch = epoch;
            }

            let ready = checkpoint.operation.ready_to_resume(levels).is_ok();
            let verdict = if checkpoint.operation.resumed {
                ResumeVerdict::Suspend
            } else {
                resume_verdict(
                    checkpoint.checkpoint_at,
                    state.resume_sync_epoch,
                    self.clock.uptime_seconds(),
                    state.settings.resume_window_s,
                    ready,
                )
            };

            match verdict {
                ResumeVerdict::Wait => {}
                ResumeVerdict::Resume => {
                    let mut operation = checkpoint.operation;
                    operation.resume();
                    tracing::info!(
                        kind = ?operation.plan.kind,
                        phase = ?operation.phase,
                        "pump operation resumed after a restart"
                    );
                    state.activity = Activity::Running(operation);
                    operation_elapsed_ms = 0;
                    wall_elapsed_ms = 0;
                    state.pending.checkpoint = true;
                    self.flush_pending(&mut state);
                    changed = true;
                }
                ResumeVerdict::Suspend => {
                    tracing::warn!(
                        kind = ?checkpoint.operation.plan.kind,
                        "pump operation suspended after a restart"
                    );
                    state.activity = Activity::Suspended(checkpoint.operation);
                    changed = true;
                }
            }
        }

        let mut finished = None;
        let mut phase_changed = false;

        if let Activity::Running(operation) = &mut state.activity {
            match operation.advance(operation_elapsed_ms, wall_elapsed_ms, levels) {
                Step::Continue => {}
                Step::PhaseChanged => {
                    tracing::info!(phase = ?operation.phase, "pump operation phase");
                    phase_changed = true;
                }
                Step::Finished(result) => finished = Some((operation.clone(), result)),
            }
        }

        if phase_changed {
            state.pending.checkpoint = true;
            changed = true;
        }

        if let Some((operation, result)) = finished {
            self.finish(&mut state, &operation, result, epoch);
            changed = true;
        }

        let return_forced_off = state.forced_off(PumpId::Return, now, epoch);
        let uv_forced_off = state.forced_off(PumpId::Uv, now, epoch);

        if state.pending.overrides {
            changed = true;
        }

        Observation {
            phase: state.phase(),
            return_forced_off,
            uv_forced_off,
            changed,
        }
    }

    pub fn fire_due_windows(&self, levels: Option<LevelConditions>) -> bool {
        let started = self.fire_due_windows_deferred(levels, Instant::now());
        self.persist_pending();
        started
    }

    fn fire_due_windows_deferred(&self, levels: Option<LevelConditions>, now: Instant) -> bool {
        let (Some(epoch), Some(civil)) =
            (self.clock.epoch_seconds(), self.clock.local_civil_time())
        else {
            return false;
        };

        let minute = epoch / 60;
        let mut state = self.lock();

        if state.last_fired_minute == Some(minute) {
            return false;
        }
        state.last_fired_minute = Some(minute);
        state.levels = levels;

        let mut due: Vec<(OperationKind, usize, OperationPlan)> = Vec::new();

        for (index, window) in state.schedule.water_change.iter_mut().enumerate() {
            if window.due(civil, epoch) {
                due.push((
                    OperationKind::WaterChange,
                    index,
                    OperationPlan::water_change(window.plan),
                ));
            }
        }

        for (index, window) in state.schedule.top_off.iter_mut().enumerate() {
            if window.due(civil, epoch) {
                due.push((
                    OperationKind::TopOff,
                    index,
                    OperationPlan::top_off(window.plan),
                ));
            }
        }

        let mut started = false;

        for (kind, index, plan) in due {
            match self.begin(&mut state, plan, now, Some(epoch)) {
                Ok(()) => started = true,
                Err(error) => {
                    let (reason, blocked_by) = match error {
                        StartError::Blocked(blocker) => (SkipReason::Blocked, Some(blocker)),
                        _ => (SkipReason::Busy, None),
                    };

                    tracing::warn!(
                        ?kind,
                        index,
                        ?reason,
                        ?blocked_by,
                        "scheduled pump window skipped"
                    );

                    state.last_skipped = Some(SkippedWindow {
                        kind,
                        index: index as u8,
                        reason,
                        blocked_by,
                        at: Some(epoch),
                    });
                }
            }
        }

        started
    }

    pub fn record(
        &self,
        id: PumpId,
        on: bool,
        fault: Option<ChannelFault>,
        blocked_by: Option<Blocker>,
    ) -> RecordOutcome {
        let next = Output {
            on,
            fault,
            blocked_by,
        };

        let previous = self.lock().outputs[id.index()].replace(next);

        RecordOutcome {
            changed: previous != Some(next),
            on_changed: previous.map(|output| output.on) != Some(on),
        }
    }

    pub fn checkpoint(&self) {
        let mut state = self.lock();
        state.pending.checkpoint = true;
        self.flush_pending(&mut state);
    }

    pub fn persist_pending(&self) {
        let mut state = self.lock();
        self.flush_pending(&mut state);
    }

    fn flush_pending(&self, state: &mut State) {
        let pending = std::mem::take(&mut state.pending);

        if pending.erase_checkpoint {
            if let Err(reason) = self.store.erase(PumpRecord::Checkpoint) {
                tracing::warn!(reason, "pump checkpoint not erased, writing a tombstone");

                if let Err(reason) = self.write(PumpRecord::Checkpoint, &None::<Checkpoint>) {
                    tracing::error!(reason, "pump checkpoint tombstone not stored");
                }
            }
        }

        if pending.last {
            self.persist(PumpRecord::LastOperations, &state.last);
        }

        if pending.checkpoint {
            if let Activity::Running(operation) = &state.activity {
                self.persist(
                    PumpRecord::Checkpoint,
                    &Checkpoint {
                        operation: operation.clone(),
                        checkpoint_at: self.clock.epoch_seconds(),
                    },
                );
            }
        }

        if pending.overrides {
            self.persist(PumpRecord::Overrides, &state.stored_overrides());
        }

        if let Some(events) = &self.events {
            for event in pending.events {
                let _ = events.send(event);
            }
        }
    }

    fn reset_runtime_at(&self, id: PumpId, now: Instant) {
        let epoch = self.clock.epoch_seconds();

        {
            let mut state = self.lock();
            state.runtime_ms[id.index()] = 0;
            state.reset_at[id.index()] = epoch;
            state.reset_instant[id.index()] = Some(now);
            self.persist(PumpRecord::Runtime, &state.runtime());
        }

        self.notify.notify_one();
    }

    pub fn report_circulation(&self, starved: bool) -> bool {
        let mut state = self.lock();
        let changed = state.circulation_starved != starved;
        state.circulation_starved = starved;

        if changed && starved {
            tracing::warn!("the sump keeps running dry, circulation is starved");
        } else if changed {
            tracing::info!("the sump stayed wet, circulation recovered");
        }

        changed
    }

    pub fn flush_runtime(&self) {
        let state = self.lock();
        self.persist(PumpRecord::Runtime, &state.runtime());
    }

    fn start(&self, plan: OperationPlan) -> Result<(), StartError> {
        let epoch = self.clock.epoch_seconds();
        {
            let mut state = self.lock();

            if plan.kind == OperationKind::WaterChange
                && !state.settings.drain_fits_reservoir(plan.drain_s)
            {
                return Err(StartError::Invalid(
                    "the planned drain exceeds the water the reservoir holds",
                ));
            }

            self.begin(&mut state, plan, Instant::now(), epoch)?;
            self.flush_pending(&mut state);
        }
        self.notify.notify_one();
        Ok(())
    }

    fn begin(
        &self,
        state: &mut State,
        plan: OperationPlan,
        now: Instant,
        epoch: Option<u64>,
    ) -> Result<(), StartError> {
        state.admit(&plan, now, epoch)?;

        if let Activity::Running(preempted) = std::mem::replace(&mut state.activity, Activity::Idle)
        {
            self.finish(state, &preempted, OperationResult::Aborted, epoch);
        }

        state.activity = Activity::Running(Operation::start(plan, epoch));
        state.fresh_operation = true;
        state.pending.checkpoint = true;
        tracing::info!(kind = ?plan.kind, "pump operation started");

        Ok(())
    }

    fn finish(
        &self,
        state: &mut State,
        operation: &Operation,
        result: OperationResult,
        epoch: Option<u64>,
    ) {
        let sump_low = state.levels.is_some_and(|levels| levels.sump_low);
        let record = operation
            .record(result)
            .graded(state.settings.flow_lph, sump_low);
        let result = record.result;

        state.last.remember(operation, record);

        state.activity = Activity::Idle;
        state.pending.last = true;
        state.pending.erase_checkpoint = true;

        tracing::info!(kind = ?record.kind, ?result, "pump operation finished");

        let flows = state.settings.flow_lph;
        state.pending.events.push(OperationEvent {
            record,
            finished_at: epoch,
            drained_l: litres(record.drain_ran_s, flows.drain),
            refilled_l: litres(record.refill_ran_s, flows.makeup),
        });
    }

    fn write<T: Serialize>(&self, record: PumpRecord, value: &T) -> Result<(), &'static str> {
        let bytes =
            serde_json::to_vec(value).map_err(|_| "the pump record could not be encoded")?;
        self.store.store(record, &bytes)
    }

    fn persist<T: Serialize>(&self, record: PumpRecord, value: &T) {
        if let Err(reason) = self.write(record, value) {
            tracing::error!(record = record.key(), reason, "pump record not stored");
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("pump mutex poisoned")
    }
}

impl<S: PumpStore, C: Clock> PumpControl for PumpService<S, C> {
    fn status(&self) -> PumpsStatus {
        let now = Instant::now();
        let epoch = self.clock.epoch_seconds();
        let mut state = self.lock();

        PumpsStatus {
            channels: state.channels(now, epoch),
            operation: state.operation_status(),
            last_top_off: state.last.top_off,
            last_water_change: state.last.water_change,
            last_skipped: state.last_skipped,
            circulation_starved: state.circulation_starved,
        }
    }

    fn settings(&self) -> PumpSettings {
        self.lock().settings.clone()
    }

    fn apply_settings(&self, patch: PumpSettingsPatch) -> Result<PumpSettings, PumpSettingsError> {
        let candidate = {
            let mut state = self.lock();
            let candidate = patch.merge_onto(&state.settings);

            candidate.validate().map_err(PumpSettingsError::Invalid)?;
            self.write(PumpRecord::Settings, &candidate)
                .map_err(PumpSettingsError::NotPersisted)?;

            state.schedule = Schedule::compile(&candidate, Some(&state.schedule));
            state.settings = candidate.clone();
            candidate
        };

        self.notify.notify_one();

        Ok(candidate)
    }

    fn set_override(
        &self,
        id: PumpId,
        duration_s: Option<u32>,
    ) -> Result<ChannelStatus, OverrideError> {
        validate_override(id, duration_s)?;
        let slot = id.override_slot().ok_or(OverrideError::NotOverridable)?;

        let now = Instant::now();
        let epoch = self.clock.epoch_seconds();
        let expiry = match (duration_s, epoch) {
            (None, _) => Expiry::Never,
            (Some(seconds), Some(epoch)) => Expiry::Timed {
                at: epoch + u64::from(seconds),
                deadline: now + Duration::from_secs(seconds.into()),
            },
            (Some(seconds), None) => {
                tracing::warn!(
                    pump = id.name(),
                    "override set without time sync will not survive a restart"
                );
                Expiry::At(now + Duration::from_secs(seconds.into()))
            }
        };

        let status = {
            let mut state = self.lock();
            let previous = state.overrides[slot].replace(expiry);

            if expiry.stored().is_some() {
                if let Err(reason) = self.write(PumpRecord::Overrides, &state.stored_overrides()) {
                    state.overrides[slot] = previous;
                    tracing::error!(
                        record = PumpRecord::Overrides.key(),
                        reason,
                        "override not stored"
                    );
                    return Err(OverrideError::NotPersisted(reason));
                }
            } else {
                self.persist(PumpRecord::Overrides, &state.stored_overrides());
            }

            state.channel(id, now, epoch)
        };

        self.notify.notify_one();

        Ok(status)
    }

    fn clear_override(&self, id: PumpId) -> Result<(), OverrideError> {
        let slot = id.override_slot().ok_or(OverrideError::NotOverridable)?;

        {
            let mut state = self.lock();
            let previous = state.overrides[slot].take();

            if previous.and_then(Expiry::stored).is_some() {
                if let Err(reason) = self.write(PumpRecord::Overrides, &state.stored_overrides()) {
                    state.overrides[slot] = previous;
                    tracing::error!(
                        record = PumpRecord::Overrides.key(),
                        reason,
                        "override not stored"
                    );
                    return Err(OverrideError::NotPersisted(reason));
                }
            }
        }

        self.notify.notify_one();

        Ok(())
    }

    fn reset_runtime(&self, id: PumpId) {
        self.reset_runtime_at(id, Instant::now());
    }

    fn start_top_off(&self, duration_s: u32) -> Result<(), StartError> {
        validate_top_off(duration_s).map_err(StartError::Invalid)?;
        self.start(OperationPlan::top_off(duration_s))
    }

    fn start_water_change(&self, plan: WaterChangePlan) -> Result<(), StartError> {
        plan.validate().map_err(StartError::Invalid)?;
        self.start(OperationPlan::water_change(plan))
    }

    fn resume(&self, kind: OperationKind) -> Result<(), StartError> {
        {
            let mut state = self.lock();
            let mut operation = match std::mem::replace(&mut state.activity, Activity::Idle) {
                Activity::Suspended(operation) if operation.plan.kind == kind => operation,
                other => {
                    state.activity = other;
                    return Err(StartError::Conflict);
                }
            };

            if let Err(blocker) = operation.ready_to_resume(state.levels) {
                state.activity = Activity::Suspended(operation);
                return Err(StartError::Blocked(blocker));
            }

            operation.resume();
            tracing::info!(?kind, "pump operation resumed over the API");
            state.activity = Activity::Running(operation);
            state.fresh_operation = true;
            state.pending.checkpoint = true;
            self.flush_pending(&mut state);
        }

        self.notify.notify_one();

        Ok(())
    }

    fn cancel(&self, kind: OperationKind) {
        let epoch = self.clock.epoch_seconds();

        {
            let mut state = self.lock();
            let (operation, result) = match std::mem::replace(&mut state.activity, Activity::Idle) {
                Activity::Running(operation) if operation.plan.kind == kind => {
                    (operation, OperationResult::Aborted)
                }
                Activity::Suspended(operation) if operation.plan.kind == kind => {
                    (operation, OperationResult::InterruptedByReboot)
                }
                Activity::Resuming(checkpoint) if checkpoint.operation.plan.kind == kind => {
                    (checkpoint.operation, OperationResult::InterruptedByReboot)
                }
                other => {
                    state.activity = other;
                    return;
                }
            };

            self.finish(&mut state, &operation, result, epoch);
            self.flush_pending(&mut state);
        }

        self.notify.notify_one();
    }
}

impl<S: PumpStore, C: Clock> PumpStatusSource for PumpService<S, C> {
    fn pump_telemetry(&self) -> PumpsTelemetry {
        let state = self.lock();
        let on =
            |id: PumpId| state.outputs[id.index()].map_or(id.fail_safe_on(), |output| output.on);
        let operation = state.operation_status();

        PumpsTelemetry {
            return_pump: on(PumpId::Return),
            uv: on(PumpId::Uv),
            makeup: on(PumpId::Makeup),
            drain: on(PumpId::Drain),
            operation_kind: operation.kind,
            operation_state: operation.state,
            circulation_starved: state.circulation_starved,
        }
    }
}

#[derive(Debug, Default)]
struct ReturnLockout {
    locked: bool,
    wet_since: Option<Instant>,
}

impl ReturnLockout {
    fn update(&mut self, levels: Option<LevelConditions>, now: Instant, delay: Duration) -> bool {
        match levels {
            None => {
                self.wet_since = None;
                false
            }
            Some(levels) if levels.sump_low => {
                self.locked = true;
                self.wet_since = None;
                true
            }
            Some(_) if self.locked => {
                let since = *self.wet_since.get_or_insert(now);

                if now.saturating_duration_since(since) >= delay {
                    self.locked = false;
                    self.wet_since = None;
                }

                self.locked
            }
            Some(_) => false,
        }
    }
}

#[derive(Debug, Default)]
struct CirculationWatch {
    lockouts: VecDeque<Instant>,
    low_since: Option<Instant>,
    wet_since: Option<Instant>,
    was_low: bool,
    starved: bool,
}

impl CirculationWatch {
    fn update(&mut self, sump_low: Option<bool>, operating: bool, now: Instant) -> bool {
        let window = Duration::from_secs(STARVED_WINDOW_S);

        match sump_low {
            None => {
                self.low_since = None;
                self.wet_since = None;
                self.was_low = false;
            }
            Some(true) => {
                self.wet_since = None;

                if operating {
                    self.low_since = None;
                } else {
                    if !self.was_low {
                        self.lockouts.push_back(now);
                    }

                    let since = *self.low_since.get_or_insert(now);
                    self.lockouts
                        .retain(|at| now.saturating_duration_since(*at) < window);
                    self.starved |= self.lockouts.len() >= STARVED_LOCKOUTS
                        || now.saturating_duration_since(since) >= window;
                }

                self.was_low = true;
            }
            Some(false) => {
                self.low_since = None;
                let since = *self.wet_since.get_or_insert(now);

                if now.saturating_duration_since(since) >= Duration::from_secs(STARVED_CLEAR_S) {
                    self.starved = false;
                    self.lockouts.clear();
                }

                self.was_low = false;
            }
        }

        self.starved
    }
}

pub struct PumpController<O, S, C> {
    outputs: O,
    service: Arc<PumpService<S, C>>,
    timing: PumpTiming,
    lockout: ReturnLockout,
    circulation: CirculationWatch,
    last_tick: Option<Instant>,
    last_checkpoint: Option<Instant>,
    last_flush: Option<Instant>,
}

impl<O: PumpOutputs, S: PumpStore, C: Clock> PumpController<O, S, C> {
    pub fn new(outputs: O, service: Arc<PumpService<S, C>>, timing: PumpTiming) -> Self {
        Self {
            outputs,
            service,
            timing,
            lockout: ReturnLockout::default(),
            circulation: CirculationWatch::default(),
            last_tick: None,
            last_checkpoint: None,
            last_flush: None,
        }
    }

    pub fn tick(&mut self, snapshot: Option<&WaterLevelSnapshot>, now: Instant) -> bool {
        let refill_stop = self.service.refill_stop_sensor();
        let levels = snapshot.map(|snapshot| LevelConditions::from_snapshot(snapshot, refill_stop));
        let elapsed = self
            .last_tick
            .map_or(Duration::ZERO, |last| now.saturating_duration_since(last));
        self.last_tick = Some(now);

        let started = self.service.fire_due_windows_deferred(levels, now);
        let observed = self.service.observe_deferred(levels, elapsed, now);
        let locked_out = self
            .lockout
            .update(levels, now, self.timing.return_restart_delay);
        let starved = self.circulation.update(
            levels.map(|levels| levels.sump_low),
            observed.phase.is_some(),
            now,
        );
        let starvation_changed = self.service.report_circulation(starved);

        let decisions = decide(DecisionInput {
            levels,
            return_locked_out: locked_out,
            return_forced_off: observed.return_forced_off,
            uv_forced_off: observed.uv_forced_off,
            phase: observed.phase,
        });

        let mut outputs_changed = false;
        let mut on_changed = false;
        let mut return_failed = false;
        for id in [PumpId::Return, PumpId::Uv, PumpId::Makeup, PumpId::Drain] {
            let mut decision = decisions[id.index()];

            if id == PumpId::Uv && return_failed {
                decision = ChannelDecision {
                    on: false,
                    blocked_by: Some(Blocker::ReturnOff),
                };
            }

            let (on, fault) = self.drive(id, decision.on);

            if id == PumpId::Return {
                return_failed = fault.is_some();
            }

            let outcome = self.service.record(id, on, fault, decision.blocked_by);
            outputs_changed |= outcome.changed;
            on_changed |= outcome.on_changed;
        }

        self.service.persist_pending();

        let last_flush = *self.last_flush.get_or_insert(now);
        if on_changed || now.saturating_duration_since(last_flush) >= self.timing.runtime_flush {
            self.service.flush_runtime();
            self.last_flush = Some(now);
        }

        let last_checkpoint = *self.last_checkpoint.get_or_insert(now);
        if now.saturating_duration_since(last_checkpoint) >= self.timing.checkpoint_interval {
            self.service.checkpoint();
            self.last_checkpoint = Some(now);
        }

        started || observed.changed || outputs_changed || starvation_changed
    }

    fn drive(&mut self, id: PumpId, on: bool) -> (bool, Option<ChannelFault>) {
        match self.outputs.set(id, on) {
            Ok(()) => (on, None),
            Err(reason) => {
                tracing::error!(pump = id.name(), reason, "pump output failed");
                (false, Some(ChannelFault::OutputError))
            }
        }
    }
}

pub async fn pump_loop<O, S, C>(
    mut controller: PumpController<O, S, C>,
    mut levels: watch::Receiver<WaterLevelSnapshot>,
    api_changed: Arc<Notify>,
    republish: Arc<Notify>,
    timing: PumpTiming,
    heartbeat: Arc<AtomicU32>,
) where
    O: PumpOutputs,
    S: PumpStore,
    C: Clock,
{
    let mut latest: Option<(WaterLevelSnapshot, Instant)> = None;
    let mut closed = false;
    let mut ticker = tokio::time::interval(timing.tick);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        heartbeat.fetch_add(1, Ordering::Relaxed);

        let api_wake = tokio::select! {
            received = levels.changed(), if !closed => {
                match received {
                    Ok(()) => latest = Some((levels.borrow_and_update().clone(), Instant::now())),
                    Err(_) => {
                        closed = true;
                        latest = None;
                    }
                }
                false
            }
            () = api_changed.notified() => true,
            _ = ticker.tick() => false,
        };

        let now = Instant::now();
        let fresh = latest
            .as_ref()
            .filter(|(_, at)| now.saturating_duration_since(*at) < timing.level_stale)
            .map(|(snapshot, _)| snapshot);

        if controller.tick(fresh, now) || api_wake {
            republish.notify_one();
        }
    }
}

#[cfg(test)]
mod tests;
