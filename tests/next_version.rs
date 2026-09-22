use chrono_stamp::{
    ChronoStamp, NextVersionError, NextVersionOptions, Stability, YearMonth, next_version,
};

fn stamp(value: &str) -> ChronoStamp {
    value.parse().unwrap()
}

fn month(year: u16, month: u8) -> YearMonth {
    YearMonth::new(year, month).unwrap()
}

#[test]
fn starts_at_zero_without_a_release_this_month() {
    let versions = [stamp("2026.7.9"), stamp("2026.8.0-abcdef0")];
    let next = next_version(&versions, month(2026, 8), NextVersionOptions::default()).unwrap();
    assert_eq!(next.to_string(), "2026.8.0");
}

#[test]
fn resets_at_a_new_month() {
    let versions = [stamp("2026.7.9")];
    let next = next_version(&versions, month(2026, 8), NextVersionOptions::default()).unwrap();
    assert_eq!(next.to_string(), "2026.8.0");
}

#[test]
fn increments_a_final_release_in_the_same_month() {
    let versions = [stamp("2026.8.0"), stamp("2026.8.1")];
    let next = next_version(&versions, month(2026, 8), NextVersionOptions::default()).unwrap();
    assert_eq!(next.to_string(), "2026.8.2");
}

#[test]
fn promotes_a_prerelease_without_incrementing() {
    let versions = [stamp("2026.8.4-rc")];
    let next = next_version(&versions, month(2026, 8), NextVersionOptions::default()).unwrap();
    assert_eq!(next.to_string(), "2026.8.4");
}

#[test]
fn repeating_a_tag_starts_the_next_increment() {
    let versions = [stamp("2026.8.4-rc")];
    let options = NextVersionOptions {
        tag: Stability::Rc,
        ..NextVersionOptions::default()
    };
    let next = next_version(&versions, month(2026, 8), options).unwrap();
    assert_eq!(next.to_string(), "2026.8.5-rc");
}

#[test]
fn rejects_future_versions_by_default() {
    let versions = [stamp("2026.9.0")];
    let error = next_version(&versions, month(2026, 8), NextVersionOptions::default()).unwrap_err();
    assert!(matches!(error, NextVersionError::FutureVersion { .. }));
}

#[test]
fn explicit_increment_is_respected() {
    let versions = [stamp("2026.8.10")];
    let options = NextVersionOptions {
        tag: Stability::Beta,
        increment: Some(42),
        allow_future: false,
    };
    let next = next_version(&versions, month(2026, 8), options).unwrap();
    assert_eq!(next.to_string(), "2026.8.42-beta");
}

#[test]
fn explicit_increment_must_advance() {
    let versions = [stamp("2026.8.10")];
    let options = NextVersionOptions {
        increment: Some(9),
        ..NextVersionOptions::default()
    };
    let error = next_version(&versions, month(2026, 8), options).unwrap_err();
    assert!(matches!(error, NextVersionError::NonMonotonic { .. }));
}

#[test]
fn explicit_increment_can_promote_at_the_same_increment() {
    let versions = [stamp("2026.8.10-rc")];
    let options = NextVersionOptions {
        increment: Some(10),
        ..NextVersionOptions::default()
    };
    let next = next_version(&versions, month(2026, 8), options).unwrap();
    assert_eq!(next.to_string(), "2026.8.10");
}

#[test]
fn allowing_future_uses_the_latest_authoritative_month() {
    let versions = [stamp("2026.9.3")];
    let options = NextVersionOptions {
        allow_future: true,
        ..NextVersionOptions::default()
    };
    let next = next_version(&versions, month(2026, 8), options).unwrap();
    assert_eq!(next.to_string(), "2026.9.4");
}

#[test]
fn increment_overflow_is_reported() {
    let versions = [stamp("2026.8.18446744073709551615")];
    let error = next_version(&versions, month(2026, 8), NextVersionOptions::default()).unwrap_err();
    assert_eq!(error, NextVersionError::IncrementOverflow);
}
