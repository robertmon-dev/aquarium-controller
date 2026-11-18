use super::*;

struct Probe(f32);

impl TdsReader for Probe {
    async fn read_volts(&mut self) -> Result<f32, TdsError> {
        Ok(self.0)
    }
}

#[tokio::test]
async fn some_delegates_to_the_probe() {
    let mut source = Some(Probe(1.2));

    assert_eq!(source.read_volts().await, Ok(1.2));
}

#[tokio::test]
async fn none_reports_the_probe_as_unavailable() {
    let mut source: Option<Probe> = None;

    assert_eq!(
        source.read_volts().await,
        Err(TdsError::Unavailable(
            "TDS component is disabled in settings"
        ))
    );
}
