use super::*;

struct Probe(f32);

impl PhReader for Probe {
    async fn read_volts(&mut self) -> Result<f32, PhError> {
        Ok(self.0)
    }
}

#[tokio::test]
async fn some_delegates_to_the_probe() {
    let mut source = Some(Probe(1.5));

    assert_eq!(source.read_volts().await, Ok(1.5));
}

#[tokio::test]
async fn none_reports_the_probe_as_unavailable() {
    let mut source: Option<Probe> = None;

    assert_eq!(
        source.read_volts().await,
        Err(PhError::Unavailable("pH component is disabled in settings"))
    );
}
