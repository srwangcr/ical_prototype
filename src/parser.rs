use ical::parser::ical::IcalParser;
use crate::error::{Result, SyncError};

#[derive(Debug, Clone)]
pub struct CalendarEvent {
    pub uid: String,
    pub start_date: i64,
    pub end_date: i64,
    pub summary: Option<String>,
    pub is_cancelled: bool,
}

/// FIX #2: ahora devolvemos TODOS los eventos con UID válido (incluidos los
/// cancelados), marcados con `is_cancelled`. El llamador decide qué hacer con
/// cada uno: los activos se guardan/chequean por overlap, los cancelados se
/// usan para borrar la reserva correspondiente si ya la teníamos guardada.
/// Antes, un evento sin start/end válidos simplemente se descartaba en
/// silencio; ahora solo se descartan si falta el UID o las fechas son
/// inválidas, que es un caso distinto (feed corrupto) al de "cancelado".
pub fn parse_ics(content: &str) -> Result<Vec<CalendarEvent>> {
    let mut events = Vec::new();
    let parser = IcalParser::new(content.as_bytes());

    for result in parser {
        match result {
            Ok(calendar) => {
                for event in calendar.events {
                    let mut uid = None;
                    let mut dtstart = None;
                    let mut dtend = None;
                    let mut summary = None;
                    let mut is_cancelled = false;

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
                            "STATUS" => {
                                if let Some(val) = property.value {
                                    if val.trim().eq_ignore_ascii_case("CANCELLED") {
                                        is_cancelled = true;
                                    }
                                }
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
                                is_cancelled,
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

    // FIX (encontrado al correr los tests): "20240101T120000Z" es el formato
    // UTC estándar que mandan Airbnb/Vrbo/Booking. `DateTime::parse_from_str`
    // con `%Y%m%dT%H%M%SZ` NUNCA matcheaba, porque chrono no interpreta una
    // "Z" literal en el string de formato como offset +00:00 — necesita un
    // especificador real (`%#z`) o hay que tratarla aparte. Como está esto
    // en el código original, cero eventos con hora UTC se estaban guardando
    // nunca; solo las fechas "todo el día" (formato %Y%m%d) funcionaban.
    if let Some(stripped) = value.strip_suffix('Z') {
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(stripped, "%Y%m%dT%H%M%S") {
            return Ok(Some(dt.and_utc().timestamp()));
        }
    }

    if let Ok(dt) = chrono::NaiveDate::parse_from_str(value, "%Y%m%d") {
        let datetime = dt.and_hms_opt(0, 0, 0).unwrap();
        return Ok(Some(datetime.and_utc().timestamp()));
    }

    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S") {
        return Ok(Some(dt.and_utc().timestamp()));
    }

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
        assert!(!events[0].is_cancelled);
    }

    #[test]
    fn test_parse_ics_cancelled_event() {
        let ics_content = r#"
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Test//EN
BEGIN:VEVENT
UID:test456
DTSTART:20240101T120000Z
DTEND:20240101T140000Z
SUMMARY:Cancelled Reservation
STATUS:CANCELLED
END:VEVENT
END:VCALENDAR
"#;

        let events = parse_ics(ics_content).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].is_cancelled);
    }

    #[test]
    fn test_parse_ics_all_day_event() {
        // Formato "todo el día" (sin hora), usado por algunos feeds para
        // check-in/check-out. No lleva sufijo Z.
        let ics_content = r#"
BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Test//EN
BEGIN:VEVENT
UID:test789
DTSTART:20240105
DTEND:20240107
SUMMARY:All-day stay
END:VEVENT
END:VCALENDAR
"#;

        let events = parse_ics(ics_content).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].start_date < events[0].end_date);
        assert!(!events[0].is_cancelled);
    }
}
