use crate::domain::metrics::ProbeVoltagesDto;

pub trait ProbeVoltageQuery {
    fn probe_voltages(&self) -> ProbeVoltagesDto;
}
