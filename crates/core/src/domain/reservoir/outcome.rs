use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum FillTrigger {
    Manual,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum FillResult {
    Completed,
    OffTarget,
    Overtime,
    Timeout,
    Aborted,
    TdsUnavailable,
    Leak,
    Blocked,
    InterruptedByReboot,
}

impl FillResult {
    pub const fn fault(self) -> Option<FillFault> {
        match self {
            Self::Overtime => Some(FillFault::Overtime),
            Self::Leak => Some(FillFault::Leak),
            _ => None,
        }
    }

    pub const fn latches_auto(self) -> bool {
        matches!(
            self,
            Self::Timeout | Self::TdsUnavailable | Self::Blocked | Self::Aborted
        )
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::OffTarget => "off_target",
            Self::Overtime => "overtime",
            Self::Timeout => "timeout",
            Self::Aborted => "aborted",
            Self::TdsUnavailable => "tds_unavailable",
            Self::Leak => "leak",
            Self::Blocked => "blocked",
            Self::InterruptedByReboot => "interrupted_by_reboot",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum FillFault {
    Overtime,
    Leak,
    Unreadable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct FaultRecord {
    pub fault: FillFault,
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningMarker {
    pub trigger: FillTrigger,
    pub started_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct FillRecord {
    pub trigger: FillTrigger,
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub started_at: Option<u64>,
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub finished_at: Option<u64>,
    pub result: FillResult,
    #[serde(default, deserialize_with = "read_tds")]
    pub final_tds_ppm: Option<f32>,
    pub ro_s: u32,
    pub tap_s: u32,
    pub mix_s: u32,
    pub ro_pulses: u32,
    pub tap_pulses: u32,
    pub duration_s: u32,
}

fn read_tds<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<f32>, D::Error> {
    Ok(Option::<f64>::deserialize(deserializer)?.map(|ppm| ppm as f32))
}

impl FillRecord {
    pub const fn interrupted(marker: RunningMarker) -> Self {
        Self {
            trigger: marker.trigger,
            started_at: marker.started_at,
            finished_at: None,
            result: FillResult::InterruptedByReboot,
            final_tds_ppm: None,
            ro_s: 0,
            tap_s: 0,
            mix_s: 0,
            ro_pulses: 0,
            tap_pulses: 0,
            duration_s: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillEvent {
    pub record: FillRecord,
    pub ro_l: Option<f32>,
    pub tap_l: Option<f32>,
}

#[cfg(test)]
mod tests;
