use super::*;

#[tokio::test]
async fn telemetry_reports_the_published_lighting_status() {
    let sink = RecordingSink::default();
    let published = LightingMetricsDto {
        profile: LightingProfile::Moonlight,
        channels: ChannelLevels::new(4, 0, 10, 0),
        override_active: true,
        available: true,
    };
    let mut c = build(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
        FakeLighting::publishing(published),
        FakeLevels::all_dry(),
    );

    c.sample().await;

    assert_eq!(sink.last().lighting, published);
}

#[tokio::test]
async fn lighting_is_unavailable_when_nothing_was_published() {
    let sink = RecordingSink::default();
    let mut c = build(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
        FakeLighting::unavailable(),
        FakeLevels::all_dry(),
    );

    c.sample().await;

    assert!(!sink.last().lighting.available);
}

#[tokio::test]
async fn telemetry_carries_the_sump_light_status() {
    let sink = RecordingSink::default();
    let status = SumpLightStatus {
        on: true,
        expires_in_s: Some(600),
        available: true,
    };
    let mut c = build_with_sump_light(FakeSumpLight(status), &sink);

    c.sample().await;

    assert_eq!(sink.last().sump_light, status);
}
