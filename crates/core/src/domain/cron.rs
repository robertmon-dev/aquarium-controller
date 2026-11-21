const WEEKDAY_NAMES: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

pub const REPEAT_HORIZON_S: u64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CivilTime {
    pub minute: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub weekday: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Firing {
    minute: (u8, u8, u8, u8),
    epoch: u64,
}

impl Firing {
    pub const fn new(at: CivilTime, epoch: u64) -> Self {
        Self {
            minute: (at.month, at.day, at.hour, at.minute),
            epoch,
        }
    }

    pub fn repeats(&self, at: CivilTime, epoch: u64) -> bool {
        Self::new(at, epoch).minute == self.minute && self.epoch.abs_diff(epoch) < REPEAT_HORIZON_S
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronExpr {
    minutes: u64,
    hours: u64,
    days: u64,
    months: u64,
    weekdays: u64,
    days_restricted: bool,
    weekdays_restricted: bool,
}

struct Field {
    mask: u64,
    restricted: bool,
}

impl CronExpr {
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        let fields: Vec<&str> = text.split_whitespace().collect();
        let &[minute, hour, day, month, weekday] = fields.as_slice() else {
            return Err("cron needs five fields: minute hour day month weekday");
        };

        let minutes = field(minute, 0, 59, &[])?;
        let hours = field(hour, 0, 23, &[])?;
        let days = field(day, 1, 31, &[])?;
        let months = field(month, 1, 12, &[])?;
        let mut weekdays = field(weekday, 0, 7, &WEEKDAY_NAMES)?;

        if weekdays.mask & (1 << 7) != 0 {
            weekdays.mask = (weekdays.mask & !(1 << 7)) | 1;
        }

        Ok(Self {
            minutes: minutes.mask,
            hours: hours.mask,
            days: days.mask,
            months: months.mask,
            weekdays: weekdays.mask,
            days_restricted: days.restricted,
            weekdays_restricted: weekdays.restricted,
        })
    }

    pub fn matches(&self, at: CivilTime) -> bool {
        let day = bit(self.days, at.day);
        let weekday = bit(self.weekdays, at.weekday);
        let day_matches = if self.days_restricted && self.weekdays_restricted {
            day || weekday
        } else {
            day && weekday
        };

        bit(self.minutes, at.minute)
            && bit(self.hours, at.hour)
            && bit(self.months, at.month)
            && day_matches
    }
}

fn bit(mask: u64, value: u8) -> bool {
    value < 64 && mask & (1 << value) != 0
}

fn field(text: &str, min: u8, max: u8, names: &[&str]) -> Result<Field, &'static str> {
    let mut mask = 0_u64;

    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => (
                range,
                step.parse::<u8>()
                    .map_err(|_| "cron step must be a number")?,
            ),
            None => (part, 1),
        };

        if step == 0 {
            return Err("cron step must be positive");
        }

        let (low, high) = if range == "*" {
            (min, max)
        } else if let Some((low, high)) = range.split_once('-') {
            (value(low, names)?, value(high, names)?)
        } else {
            let single = value(range, names)?;
            (single, if part.contains('/') { max } else { single })
        };

        if low < min || high > max || low > high {
            return Err("cron value out of range");
        }

        let mut current = low;
        while current <= high {
            mask |= 1 << current;
            current = match current.checked_add(step) {
                Some(next) => next,
                None => break,
            };
        }
    }

    Ok(Field {
        mask,
        restricted: !text.starts_with('*'),
    })
}

fn value(text: &str, names: &[&str]) -> Result<u8, &'static str> {
    if let Some(index) = names
        .iter()
        .position(|name| name.eq_ignore_ascii_case(text))
    {
        return Ok(index as u8);
    }

    text.parse()
        .map_err(|_| "cron value must be a number or a weekday name")
}

#[cfg(test)]
mod tests;
