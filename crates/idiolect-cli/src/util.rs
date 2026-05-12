//! Shared helpers for CLI subcommands.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use anyhow::{Context, Result, anyhow};
use idiolect_records::Datetime;

/// Best-effort "now" as a `Datetime`. Manual Gregorian split avoids
/// pulling a date crate; precision is millisecond.
pub fn now_datetime() -> Result<Datetime> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("clock before unix epoch")?;
    let secs = now.as_secs() as i64;
    let millis = now.subsec_millis();
    let days = secs.div_euclid(86_400);
    let time_of_day = secs.rem_euclid(86_400);
    let (year, month, day) = days_to_ymd(days);
    let hour = (time_of_day / 3600) as u32;
    let minute = ((time_of_day % 3600) / 60) as u32;
    let second = (time_of_day % 60) as u32;
    let s = format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z"
    );
    Datetime::parse(s).map_err(|e| anyhow!("parse datetime: {e}"))
}

fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    let mut year: i64 = 1970;
    let mut remaining = days;
    loop {
        let len = if is_leap(year) { 366 } else { 365 };
        if remaining < len {
            break;
        }
        remaining -= len;
        year += 1;
    }
    let lengths = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month: u32 = 0;
    for (idx, &len) in lengths.iter().enumerate() {
        if remaining < len {
            month = (idx + 1) as u32;
            break;
        }
        remaining -= len;
    }
    let day = (remaining + 1) as u32;
    (year, month, day)
}

const fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}
