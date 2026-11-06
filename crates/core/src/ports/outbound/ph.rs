use crate::protocol::ph::PhError;

#[allow(async_fn_in_trait)]
pub trait PhReader {
    async fn read_volts(&mut self) -> Result<f32, PhError>;
}

impl<T: PhReader> PhReader for Option<T> {
    async fn read_volts(&mut self) -> Result<f32, PhError> {
        match self {
            Some(probe) => probe.read_volts().await,
            None => Err(PhError::Unavailable("pH component is disabled in settings")),
        }
    }
}

#[cfg(test)]
mod tests;
