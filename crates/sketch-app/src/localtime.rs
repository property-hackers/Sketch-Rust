//! Local date and time for revision stamps (stored as UTC ISO 8601).
//!
//! The standard library has no time zone support, so the offset comes from
//! the OS: `localtime_r` on Unix, `GetTimeZoneInformation` on Windows. Where
//! neither is available the time is shown as UTC.

/// The years a stamp may name: four digits, as RFC 3339 writes them.
const YEARS: std::ops::RangeInclusive<i64> = 0..=9999;

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm), or None
/// when the year is outside 0000..=9999 (so nothing below can overflow).
fn days_from_civil_checked(y: i64, m: i64, d: i64) -> Option<i64> {
    if !YEARS.contains(&y) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// Inverse of `days_from_civil_checked`; `z` is already shifted by 719 468 days
/// (the 0000-03-01 epoch) and bounded by the caller.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Unix seconds of an RFC 3339 stamp: `2026-11-12T15:04:05.123Z` or with a
/// `+hh:mm` / `-hh:mm` offset (converted to UTC). Fractions of a second are
/// dropped on purpose; a missing zone or seconds field, or any malformed
/// part, gives None.
pub fn parse_utc(iso: &str) -> Option<i64> {
    let (date, time) = iso.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    if d.next().is_some() {
        return None;
    }
    let (clock, offset) = match time.strip_suffix('Z') {
        Some(clock) => (clock, 0),
        None => {
            let at = time.rfind(['+', '-'])?;
            let sign = if time[at..].starts_with('-') { -1 } else { 1 };
            let (hh, mm) = time[at + 1..].split_once(':')?;
            let (hh, mm) = (digits(hh)?, digits(mm)?);
            if hh > 23 || mm > 59 {
                return None;
            }
            (&time[..at], sign * (hh * 60 + mm) * 60)
        }
    };
    let clock = clock.split_once('.').map_or(clock, |(whole, fraction)| {
        if digits(fraction).is_some() { whole } else { "" }
    });
    let mut t = clock.split(':');
    let (h, min, s) = (digits(t.next()?)?, digits(t.next()?)?, digits(t.next()?)?);
    if t.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&day) || h > 23 || min > 59 || s > 60 {
        return None;
    }
    let seconds = days_from_civil_checked(y, m, day)?.checked_mul(86_400)?.checked_add(h * 3600 + min * 60 + s)?;
    seconds.checked_sub(offset)
}

/// A non-empty run of ASCII digits as a number.
fn digits(text: &str) -> Option<i64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// `2026-11-12 3:04 PM` for Unix seconds shifted by `offset` seconds; None when the
/// shift overflows or lands outside the years 0000..=9999.
pub fn format_shifted(unix: i64, offset: i64) -> Option<String> {
    let local = unix.checked_add(offset)?;
    let (y, m, d) = civil_from_days(local.div_euclid(86_400).checked_add(719_468)?);
    if !YEARS.contains(&y) {
        return None;
    }
    let sod = local.rem_euclid(86_400);
    let (h, min) = (sod / 3600, (sod % 3600) / 60);
    let hour12 = if h % 12 == 0 { 12 } else { h % 12 };
    let am_pm = if h < 12 { "AM" } else { "PM" };
    Some(format!("{y:04}-{m:02}-{d:02} {hour12}:{min:02} {am_pm}"))
}

/// Seconds east of UTC at the Unix instant `unix`, or None when unknown.
#[cfg(all(unix, target_pointer_width = "64"))]
fn utc_offset(unix: i64) -> Option<i64> {
    #[repr(C)]
    struct Tm {
        fields: [i32; 9],
        gmtoff: i64,
        zone: *const u8,
    }
    extern "C" {
        fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
    }
    let mut tm = Tm {
        fields: [0; 9],
        gmtoff: 0,
        zone: std::ptr::null(),
    };
    // SAFETY: `tm` matches the C `struct tm` layout of glibc, musl and macOS
    // on 64-bit targets (nine ints, then `tm_gmtoff`, then `tm_zone`);
    // `localtime_r` only writes through the pointer it is given.
    let done = unsafe { localtime_r(&unix, &mut tm) };
    (!done.is_null()).then_some(tm.gmtoff)
}

#[cfg(windows)]
fn utc_offset(_unix: i64) -> Option<i64> {
    #[repr(C)]
    struct SystemTime([u16; 8]);
    #[repr(C)]
    #[allow(dead_code)]
    struct TimeZoneInformation {
        bias: i32,
        standard_name: [u16; 32],
        standard_date: SystemTime,
        standard_bias: i32,
        daylight_name: [u16; 32],
        daylight_date: SystemTime,
        daylight_bias: i32,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetTimeZoneInformation(info: *mut TimeZoneInformation) -> u32;
    }
    const TIME_ZONE_ID_INVALID: u32 = 0xFFFF_FFFF;
    const TIME_ZONE_ID_DAYLIGHT: u32 = 2;
    let mut info = TimeZoneInformation {
        bias: 0,
        standard_name: [0; 32],
        standard_date: SystemTime([0; 8]),
        standard_bias: 0,
        daylight_name: [0; 32],
        daylight_date: SystemTime([0; 8]),
        daylight_bias: 0,
    };
    // SAFETY: `info` matches the Win32 TIME_ZONE_INFORMATION layout and is
    // only written by the call. The current zone rule is used for every
    // date, so a past date across a DST change can be an hour off.
    let id = unsafe { GetTimeZoneInformation(&mut info) };
    if id == TIME_ZONE_ID_INVALID {
        return None;
    }
    let extra = if id == TIME_ZONE_ID_DAYLIGHT {
        info.daylight_bias
    } else {
        info.standard_bias
    };
    // Bias is minutes to add to local time to get UTC.
    Some(-i64::from(info.bias + extra) * 60)
}

#[cfg(not(any(windows, all(unix, target_pointer_width = "64"))))]
fn utc_offset(_unix: i64) -> Option<i64> {
    None
}

/// A stored UTC stamp as local `2026-11-12 3:04 PM`; the raw text if it does
/// not parse, UTC (marked) if the OS offset is unavailable.
pub fn display(iso_utc: &str) -> String {
    let Some(unix) = parse_utc(iso_utc) else {
        return iso_utc.to_string();
    };
    let shown = match utc_offset(unix) {
        Some(offset) => format_shifted(unix, offset),
        None => format_shifted(unix, 0).map(|text| format!("{text} UTC")),
    };
    shown.unwrap_or_else(|| iso_utc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_with_an_offset() {
        let unix = parse_utc("2026-11-12T15:04:05.123Z").unwrap();
        assert_eq!(format_shifted(unix, 0).as_deref(), Some("2026-11-12 3:04 PM"));
        assert_eq!(format_shifted(unix, -5 * 3600).as_deref(), Some("2026-11-12 10:04 AM"));
        assert_eq!(format_shifted(unix, 10 * 3600).as_deref(), Some("2026-11-13 1:04 AM"));
        assert_eq!(parse_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(format_shifted(0, 0).as_deref(), Some("1970-01-01 12:00 AM"));
        // Offsets convert to UTC; a missing or malformed zone or seconds field is None.
        assert_eq!(parse_utc("2026-11-12T20:34:05+05:30"), parse_utc("2026-11-12T15:04:05Z"));
        assert_eq!(parse_utc("2026-11-12T10:04:05-05:00"), parse_utc("2026-11-12T15:04:05Z"));
        assert!(parse_utc("2026-11-12T15:04:05").is_none(), "no zone");
        assert!(parse_utc("2026-11-12T15:04Z").is_none(), "no seconds");
        assert!(parse_utc("2026-11-12T15:04:xxZ").is_none());
        assert!(parse_utc("2026-11-12T15:04:05+0x:00").is_none());
        assert!(parse_utc("garbage").is_none());
        assert!(parse_utc("2026-13-01T00:00:00Z").is_none());
        assert_eq!(display("garbage"), "garbage");
    }

    #[test]
    fn hostile_stamps_neither_panic_nor_wrap() {
        // Years outside 0000..=9999, offsets past 23:59 and values that overflow i64
        // are not times.
        assert!(parse_utc("99999-01-01T00:00:00Z").is_none());
        assert!(parse_utc("2026-11-12T15:04:05+99:99").is_none());
        assert!(parse_utc("2026-11-12T15:04:05+99999999999999999:00").is_none());
        assert!(parse_utc("9223372036854775807-01-01T00:00:00Z").is_none());
        assert!(parse_utc("2026-11-12T99999999999999999:04:05Z").is_none());
        assert_eq!(parse_utc("9999-12-31T23:59:59Z").map(|u| format_shifted(u, 0)), Some(Some("9999-12-31 11:59 PM".into())));
        assert_eq!(display("99999-01-01T00:00:00Z"), "99999-01-01T00:00:00Z");
        // Shifting never overflows or leaves the four-digit year range.
        assert_eq!(format_shifted(i64::MAX, 0), None);
        assert_eq!(format_shifted(0, i64::MAX), None);
        assert_eq!(format_shifted(i64::MIN, -1), None);
        assert_eq!(format_shifted(253_402_300_800, 0), None, "year 10000");
        assert_eq!(format_shifted(-62_167_219_201, 0), None, "year -1");
        assert_eq!(days_from_civil_checked(i64::MAX, 1, 1), None);
    }
}
