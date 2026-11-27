pub const fn duty_for_pct(pct: u8, max_duty: u32) -> u32 {
    let clamped = if pct > 100 { 100 } else { pct } as u32;
    (clamped * max_duty + 50) / 100
}

#[cfg(test)]
mod tests;
