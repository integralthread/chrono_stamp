use chrono_stamp::ChronoStamp;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    valid: Vec<ValidCase>,
    invalid: Vec<String>,
    comparisons: Vec<[String; 2]>,
}

#[derive(Debug, Deserialize)]
struct ValidCase {
    input: String,
    display: String,
    canonical: String,
    prerelease: bool,
    dev_build: bool,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("fixtures/format_contract.json"))
        .expect("format fixture must contain valid JSON")
}

#[test]
fn accepted_inputs_normalize_as_specified() {
    for case in fixture().valid {
        let stamp = case
            .input
            .parse::<ChronoStamp>()
            .unwrap_or_else(|error| panic!("{} should parse: {error}", case.input));

        assert_eq!(
            stamp.to_string(),
            case.display,
            "display for {}",
            case.input
        );
        assert_eq!(
            stamp.canonical(),
            case.canonical,
            "canonical for {}",
            case.input
        );
        assert_eq!(
            stamp.is_prerelease(),
            case.prerelease,
            "pre-release for {}",
            case.input
        );
        assert_eq!(
            stamp.is_dev_build(),
            case.dev_build,
            "dev build for {}",
            case.input
        );

        assert_eq!(
            stamp.to_string().parse::<ChronoStamp>().unwrap(),
            stamp,
            "display round trip for {}",
            case.input
        );
        assert_eq!(
            stamp.canonical().parse::<ChronoStamp>().unwrap(),
            stamp,
            "canonical round trip for {}",
            case.input
        );
    }
}

#[test]
fn rejected_inputs_stay_rejected() {
    for input in fixture().invalid {
        assert!(
            input.parse::<ChronoStamp>().is_err(),
            "{input} should have been rejected"
        );
    }
}

#[test]
fn comparison_examples_are_in_ascending_order() {
    for [left, right] in fixture().comparisons {
        let left = left.parse::<ChronoStamp>().unwrap();
        let right = right.parse::<ChronoStamp>().unwrap();
        assert!(left < right, "expected {left} < {right}");
    }
}

#[test]
fn serde_representation_is_the_display_string() {
    let stamp = "2026.8.3-final".parse::<ChronoStamp>().unwrap();
    let json = serde_json::to_string(&stamp).unwrap();
    assert_eq!(json, "\"2026.8.3\"");
    assert_eq!(serde_json::from_str::<ChronoStamp>(&json).unwrap(), stamp);
}
