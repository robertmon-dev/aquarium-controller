use crate::protocol::tds::TdsError;

#[allow(async_fn_in_trait)]
pub trait TdsReader {
    async fn read_volts(&mut self) -> Result<f32, TdsError>;
}

impl<T: TdsReader> TdsReader for Option<T> {
    async fn read_volts(&mut self) -> Result<f32, TdsError> {
        match self {
            Some(probe) => probe.read_volts().await,
            None => Err(TdsError::Unavailable(
                "TDS component is disabled in settings",
            )),
        }
    }
}

#[cfg(test)]
mod tests;
