use mivia_domain::ids::Timestamp;
use mivia_domain::naming::Stamp;

const DAY_MS: i64 = 86_400_000;

pub fn iso_utc(at: Timestamp) -> String {
    let days = at.0.div_euclid(DAY_MS);
    let ms_of_day = at.0.rem_euclid(DAY_MS);
    let (year, month, day) = civil_date(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        ms_of_day / 3_600_000,
        ms_of_day / 60_000 % 60,
        ms_of_day / 1_000 % 60,
        ms_of_day % 1_000,
    )
}

pub fn utc_date(at: Timestamp) -> String {
    let (year, month, day) = civil_date(at.0.div_euclid(DAY_MS));
    format!("{year:04}-{month:02}-{day:02}")
}

pub fn utc_stamp(at: Timestamp) -> Stamp {
    let ms_of_day = at.0.rem_euclid(DAY_MS);
    let (year, month, day) = civil_date(at.0.div_euclid(DAY_MS));
    let small = |value: i64| u8::try_from(value).unwrap_or_default();
    Stamp {
        year: u16::try_from(year).unwrap_or_default(),
        month: small(month),
        day: small(day),
        hour: small(ms_of_day / 3_600_000),
        minute: small(ms_of_day / 60_000 % 60),
    }
}

pub fn parse_iso_utc(text: &str) -> Option<Timestamp> {
    let (date, rest) = text.strip_suffix('Z')?.split_once('T')?;
    let mut date = date.splitn(3, '-').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let (clock, fraction) = rest.split_once('.').unwrap_or((rest, "0"));
    let mut clock = clock.splitn(3, ':').map(str::parse::<i64>);
    let (hour, minute, second) = (
        clock.next()?.ok()?,
        clock.next()?.ok()?,
        clock.next()?.ok()?,
    );
    let millis: i64 = format!("{fraction:0<3}").get(..3)?.parse().ok()?;
    let in_range = (1..=12).contains(&month) && (1..=31).contains(&day);
    let clock_ok = hour < 24 && minute < 60 && second < 61;
    (in_range && clock_ok).then(|| {
        let seconds = (days_from_civil(year, month, day) * 24 + hour) * 3600 + minute * 60 + second;
        Timestamp(seconds * 1000 + millis)
    })
}

pub fn offset_minutes(local: Stamp, utc: Stamp) -> i64 {
    let minutes = |stamp: Stamp| {
        let days = days_from_civil(
            i64::from(stamp.year),
            i64::from(stamp.month),
            i64::from(stamp.day),
        );
        (days * 24 + i64::from(stamp.hour)) * 60 + i64::from(stamp.minute)
    };
    minutes(local) - minutes(utc)
}

pub fn iso_with_offset(at: Timestamp, offset_minutes: i64) -> String {
    let local = iso_utc(at.plus_ms(offset_minutes * 60_000));
    let body = local.get(..19).unwrap_or_default();
    let sign = if offset_minutes < 0 { '-' } else { '+' };
    let abs = offset_minutes.abs();
    format!("{body}{sign}{:02}:{:02}", abs / 60, abs % 60)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted_month = (month + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_date(days_since_epoch: i64) -> (i64, i64, i64) {
    let shifted = days_since_epoch + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
