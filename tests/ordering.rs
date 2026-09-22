use std::cmp::Ordering;

use chrono_stamp::{ChronoStamp, Stability};
use proptest::prelude::*;

#[test]
fn sort_matches_the_documented_development_cycle() {
    let mut versions = [
        "2026.1.0",
        "2026.1.0-rc",
        "2026.1.0-def5678",
        "2026.1.0-alpha",
        "2025.12.9",
        "2026.1.0-beta",
        "2026.1.0-abc1234",
        "2026.1.0-dev",
    ]
    .map(|value| value.parse::<ChronoStamp>().unwrap());

    versions.sort();

    assert_eq!(
        versions.map(|version| version.to_string()),
        [
            "2025.12.9",
            "2026.1.0-abc1234",
            "2026.1.0-def5678",
            "2026.1.0-dev",
            "2026.1.0-alpha",
            "2026.1.0-beta",
            "2026.1.0-rc",
            "2026.1.0",
        ]
    );
}

proptest! {
    #[test]
    fn release_display_round_trips(
        year in 1_u16..=9999,
        month in 1_u8..=12,
        increment in any::<u64>(),
        tag_index in 0_u8..5,
    ) {
        let tag = match tag_index {
            0 => Stability::Dev,
            1 => Stability::Alpha,
            2 => Stability::Beta,
            3 => Stability::Rc,
            _ => Stability::Final,
        };
        let stamp = ChronoStamp::release(year, month, increment, tag).unwrap();
        prop_assert_eq!(stamp.to_string().parse::<ChronoStamp>().unwrap(), stamp);
    }

    #[test]
    fn hash_display_round_trips(
        year in 1_u16..=9999,
        month in 1_u8..=12,
        hash in "[0-9a-f]{7,16}",
    ) {
        let stamp = ChronoStamp::git(year, month, hash).unwrap();
        prop_assert_eq!(stamp.to_string().parse::<ChronoStamp>().unwrap(), stamp);
    }

    #[test]
    fn comparison_is_antisymmetric(
        left_increment in any::<u64>(),
        right_increment in any::<u64>(),
    ) {
        let left = ChronoStamp::release(2026, 8, left_increment, Stability::Final).unwrap();
        let right = ChronoStamp::release(2026, 8, right_increment, Stability::Final).unwrap();
        prop_assert_eq!(left.cmp(&right), right.cmp(&left).reverse());
        if left_increment == right_increment {
            prop_assert_eq!(left.cmp(&right), Ordering::Equal);
        }
    }
}
