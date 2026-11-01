#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ExportError {
    #[error("transport failure: {0}")]
    Transport(&'static str),

    #[error("InfluxDB rejected the write permanently (HTTP {0})")]
    Rejected(u16),

    #[error("InfluxDB is unavailable (HTTP {0})")]
    Unavailable(u16),
}

impl ExportError {
    pub fn is_permanent(self) -> bool {
        matches!(self, Self::Rejected(_))
    }
}

#[allow(async_fn_in_trait)]
pub trait MetricsWriter {
    async fn write(&self, body: &str) -> Result<(), ExportError>;
}
