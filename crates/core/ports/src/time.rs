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
