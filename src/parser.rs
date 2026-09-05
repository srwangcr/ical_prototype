use ical::parser::ical::IcalParser;
use chrono::{DateTime, Utc};
use crate::error::{Result, SyncError};

#[derive(Debug, Clone)]
pub struct CalendarEvent {
    pub uid: String,
    pub start_date: i64,
    pub end_date: i64,
    pub summary: Option<String>,
}

pub fn parse_ics(content: &str) -> Result<Vec<CalendarEvent>> {
    let mut events = Vec::new();
    let parser = IcalParser::new(content.as_bytes());

    for result in parser {
        match result {
            Ok(calendar) => {
                // Iterar sobre los eventos usando el método correcto
                for event in calendar.events {
                    let mut uid = None;
                    let mut dtstart = None;
                    let mut dtend = None;
                    let mut summary = None;

                    for property in event.properties {
                        match property.name.as_str() {
                            "UID" => {
                                uid = property.value;
                            }
                            "DTSTART" => {
                                if let Some(val) = property.value {
                                    dtstart = parse_timestamp(&val)?;
                                }
                            }
                            "DTEND" => {
                                if let Some(val) = property.value {
                                    dtend = parse_timestamp(&val)?;
                                }
                            }
                            "SUMMARY" => {
                                summary = property.value;
                            }
                            _ => {}
                        }
                    }

                    if let (Some(uid), Some(start), Some(end)) = (uid, dtstart, dtend) {
                        if start < end {
                            events.push(CalendarEvent {
                                uid,
                                start_date: start,
                                end_date: end,
                                summary,
                            });
                        }
                    }
                }
            }
            Err(e) => {
                return Err(SyncError::Ical(format!("Error parseando ICS: {:?}", e)));
            }
        }
    }

    Ok(events)
}

fn parse_timestamp(value: &str) -> Result<Option<i64>> {
    let value = value.trim().trim_matches('"');
    
    // Formato: 20240101T120000Z
    if let Ok(dt) = DateTime::parse_from_str(value, "%Y%m%dT%H%M%SZ") {
        return Ok(Some(dt.timestamp()));
    }
    
    // Formato: 20240101
    if let Ok(dt) = chrono::NaiveDate::parse_from_str(value, "%Y%m%d") {
        let datetime = dt.and_hms_opt(0, 0, 0).unwrap();
        return Ok(Some(datetime.and_utc().timestamp()));
    }

    // Formato: 20240101T120000
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S") {
        return Ok(Some(dt.and_utc().timestamp()));
    }

    // Formato: 2024-01-01 12:00:00
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return Ok(Some(dt.and_utc().timestamp()));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ics() {
        let ics_content = r#"
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Test//EN
BEGIN:VEVENT
UID:test123
DTSTART:20240101T120000Z
DTEND:20240101T140000Z
SUMMARY:Test Reservation
END:VEVENT
END:VCALENDAR
"#;

        let events = parse_ics(ics_content).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].uid, "test123");
        assert_eq!(events[0].summary, Some("Test Reservation".to_string()));
    }
}