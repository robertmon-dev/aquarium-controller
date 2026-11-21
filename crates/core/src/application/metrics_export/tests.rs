use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use super::*;

#[derive(Default)]
struct FakeWriter {
    bodies: Mutex<Vec<String>>,
    responses: Mutex<VecDeque<Result<(), ExportError>>>,
}

impl FakeWriter {
    fn failing_with(error: ExportError, times: usize) -> Self {
        Self {
            bodies: Mutex::new(Vec::new()),
            responses: Mutex::new((0..times).map(|_| Err(error)).collect()),
        }
    }

    fn bodies(&self) -> Vec<String> {
        self.bodies.lock().unwrap().clone()
    }

    fn last_body(&self) -> String {
        self.bodies().last().cloned().expect("nothing was written")
    }
}

impl MetricsWriter for &FakeWriter {
    async fn write(&self, body: &str) -> Result<(), ExportError> {
        self.bodies.lock().unwrap().push(body.to_owned());
        self.responses.lock().unwrap().pop_front().unwrap_or(Ok(()))
    }
}

fn dated(epoch_s: u64) -> TankTelemetryDto {
    let mut dto = TankTelemetryDto::empty();
    dto.timestamp_epoch_s = epoch_s;
    dto
}

const BACKOFF: Duration = Duration::from_secs(30);

fn exporter(writer: &FakeWriter, capacity: usize, batch: usize) -> MetricsExporter<&FakeWriter> {
    MetricsExporter::new(writer, capacity, batch, BACKOFF)
}

#[tokio::test(start_paused = true)]
async fn a_transient_error_defers_the_next_attempt_until_the_backoff_ends() {
    let writer = FakeWriter::failing_with(ExportError::Unavailable(503), 1);
    let mut subject = exporter(&writer, 100, 100);
    let start = Instant::now();

    subject.enqueue(&dated(1_750_000_000));
    assert!(subject.ready(start));
    assert!(subject.flush().await.is_err());
    assert_eq!(subject.next_attempt_at(), Some(start + BACKOFF));

    for second in (2..30).step_by(2) {
        tokio::time::sleep_until(start + Duration::from_secs(second)).await;
        subject.enqueue(&dated(1_750_000_000 + second));
        assert!(!subject.ready(Instant::now()));
        subject.flush_ready().await;
    }
    assert_eq!(writer.bodies().len(), 1);
    assert_eq!(subject.pending(), 15);

    tokio::time::sleep_until(start + BACKOFF).await;
    assert!(subject.ready(Instant::now()));
    subject.flush_ready().await;
    assert_eq!(writer.bodies().len(), 2);
    assert_eq!(subject.pending(), 0);
    assert_eq!(subject.next_attempt_at(), None);
}

#[tokio::test(start_paused = true)]
async fn the_export_loop_retries_a_503_once_per_backoff_while_samples_keep_coming() {
    let writer = FakeWriter::failing_with(ExportError::Unavailable(503), 1);
    let (snapshots, rx) = watch::channel(TankTelemetryDto::empty());
    let (_events, events_rx) = mpsc::unbounded_channel();
    let start = Instant::now();

    let producer = async {
        let mut after_retry = 0;
        for second in (0..=40u64).step_by(2) {
            tokio::time::sleep_until(start + Duration::from_secs(second)).await;
            snapshots.send_replace(dated(1_750_000_000 + second));
            tokio::time::sleep(Duration::from_millis(1)).await;
            let attempts = writer.bodies().len();
            match second {
                0..30 => assert_eq!(attempts, 1, "at {second} s"),
                30 => {
                    assert!((2..=3).contains(&attempts), "at {second} s: {attempts}");
                    after_retry = attempts;
                }
                _ => assert_eq!(
                    attempts,
                    after_retry + (second as usize - 30) / 2,
                    "at {second} s"
                ),
            }
        }
    };

    tokio::select! {
        () = export_loop(exporter(&writer, 100, 100), rx, events_rx) => panic!("the loop ended"),
        () = producer => {}
    }

    let retry = &writer.bodies()[1];
    assert!(retry.contains("1750000000"), "{retry}");
    assert!(retry.contains("1750000028"), "{retry}");
}

#[tokio::test]
async fn a_successful_write_drains_the_batch() {
    let writer = FakeWriter::default();
    let mut subject = exporter(&writer, 10, 10);

    subject.enqueue(&dated(1_750_000_000));
    subject.enqueue(&dated(1_750_000_030));
    assert_eq!(subject.pending(), 2);

    let written = subject.flush().await.expect("the fake writer accepts");

    assert_eq!(written, 2);
    assert_eq!(subject.pending(), 0);
}

#[tokio::test]
async fn a_failed_write_keeps_the_snapshots_for_the_next_attempt() {
    let writer = FakeWriter::failing_with(ExportError::Unavailable(503), 1);
    let mut subject = exporter(&writer, 10, 10);

    subject.enqueue(&dated(1_750_000_000));

    assert!(subject.flush().await.is_err());
    assert_eq!(subject.pending(), 1, "a lost write must not lose the point");

    let written = subject.flush().await.expect("the retry succeeds");
    assert_eq!(written, 1);
    assert_eq!(subject.pending(), 0);
}

#[tokio::test]
async fn throttling_keeps_the_batch() {
    let writer = FakeWriter::failing_with(ExportError::Unavailable(429), 1);
    let mut subject = exporter(&writer, 10, 10);

    subject.enqueue(&dated(1_750_000_000));
    assert!(subject.flush().await.is_err());

    assert_eq!(subject.pending(), 1);
}

#[tokio::test]
async fn a_permanent_rejection_discards_the_batch() {
    let writer = FakeWriter::failing_with(ExportError::Rejected(401), 1);
    let mut subject = exporter(&writer, 10, 10);

    subject.enqueue(&dated(1_750_000_000));
    assert!(subject.flush().await.is_err());

    assert_eq!(
        subject.pending(),
        0,
        "retrying a bad token forever would only fill the buffer"
    );
}

#[tokio::test]
async fn a_batch_is_capped_at_the_configured_size() {
    let writer = FakeWriter::default();
    let mut subject = exporter(&writer, 10, 2);

    for i in 0..5 {
        subject.enqueue(&dated(1_750_000_000 + i * 30));
    }

    let written = subject.flush().await.expect("accepted");

    assert_eq!(written, 2);
    assert_eq!(subject.pending(), 3);
}

#[tokio::test]
async fn an_overflowing_buffer_drops_the_oldest_snapshot() {
    let writer = FakeWriter::default();
    let mut subject = exporter(&writer, 2, 10);

    subject.enqueue(&dated(1_750_000_000));
    subject.enqueue(&dated(1_750_000_030));
    subject.enqueue(&dated(1_750_000_060));

    assert_eq!(subject.pending(), 2);
    subject.flush().await.expect("accepted");

    let body = writer.last_body();
    assert!(
        !body.contains("1750000000"),
        "the oldest snapshot should have been dropped: {body}"
    );
    assert!(body.contains("1750000030"), "got: {body}");
    assert!(body.contains("1750000060"), "got: {body}");
}

#[tokio::test]
async fn an_undated_snapshot_never_enters_the_buffer() {
    let writer = FakeWriter::default();
    let mut subject = exporter(&writer, 10, 10);

    assert!(
        !subject.enqueue(&dated(0)),
        "an unsynced clock must be reported as a drop"
    );
    assert_eq!(subject.pending(), 0);
}

#[tokio::test]
async fn flushing_an_empty_buffer_writes_nothing() {
    let writer = FakeWriter::default();
    let mut subject = exporter(&writer, 10, 10);

    let written = subject.flush().await.expect("nothing to do");

    assert_eq!(written, 0);
    assert!(
        writer.bodies().is_empty(),
        "an empty flush must not hit the network"
    );
}

#[tokio::test]
async fn batched_snapshots_are_joined_with_newlines() {
    let writer = FakeWriter::default();
    let mut subject = exporter(&writer, 10, 10);

    subject.enqueue(&dated(1_750_000_000));
    subject.enqueue(&dated(1_750_000_030));
    subject.flush().await.expect("accepted");

    let body = writer.last_body();
    assert!(!body.ends_with('\n'), "got: {body:?}");
    for line in body.lines() {
        assert!(!line.trim().is_empty(), "blank line in: {body:?}");
    }
}

#[tokio::test]
async fn operation_events_share_the_buffer_with_snapshots() {
    use crate::domain::pump::{OperationEvent, OperationKind, OperationRecord, OperationResult};

    let writer = FakeWriter::default();
    let mut subject = exporter(&writer, 10, 10);
    let event = OperationEvent {
        record: OperationRecord {
            kind: OperationKind::TopOff,
            started_at: None,
            drain_ran_s: 0,
            settle_ran_s: 0,
            refill_ran_s: 60,
            result: OperationResult::Completed,
            resumed: false,
            finished_at: Some(1_750_000_060),
        },
        finished_at: Some(1_750_000_060),
        drained_l: None,
        refilled_l: None,
    };

    assert!(subject.enqueue(&dated(1_750_000_000)));
    assert!(subject.enqueue_operation(&event));
    assert!(!subject.enqueue_operation(&OperationEvent {
        finished_at: None,
        ..event
    }));
    assert_eq!(subject.pending(), 2);

    subject.flush().await.expect("accepted");

    assert!(writer
        .last_body()
        .contains("aquarium_pump_operation,kind=top_off,result=completed"));
}
