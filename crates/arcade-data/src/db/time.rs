use chrono::{DateTime, SecondsFormat, Utc};

pub(super) fn parse_db_datetime_opt(value: Option<String>) -> Option<DateTime<Utc>> {
    value.and_then(|raw| parse_db_datetime(&raw))
}

pub(super) fn parse_db_datetime(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Some(dt.with_timezone(&Utc));
    }

    let parsed = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc))
}

pub(super) fn to_cloud_version(updated_at: DateTime<Utc>) -> String {
    updated_at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub(super) fn now_sqlite() -> String {
    Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}
