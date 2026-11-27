//! Calendrier : événements à venir et participants invités (préparation des réunions, noms
//! des intervenants). Lecture seule. Remplacé plus bas par une implémentation complète.

use crate::protocol::{CalendarEvent, PermissionValue};

pub fn status() -> PermissionValue {
    PermissionValue::Unknown
}

pub fn events(_from: chrono::DateTime<chrono::Local>, _to: chrono::DateTime<chrono::Local>) -> Vec<CalendarEvent> {
    vec![]
}
