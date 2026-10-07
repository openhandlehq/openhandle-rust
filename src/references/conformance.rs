use serde::Deserialize;

use super::{Reference, resolve_reference};
use crate::errors::Error;

const FIXTURE: &str = include_str!("../../testdata/reference-conformance.json");

#[derive(Deserialize)]
struct Fixture {
    version: u32,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    platform: String,
    resource: String,
    input: Input,
    identifier: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct Input {
    kind: String,
    value: String,
}

#[test]
fn every_shared_reference_case_resolves_as_the_fixture_requires() {
    let fixture: Fixture = serde_json::from_str(FIXTURE).expect("fixture is valid JSON");
    assert_eq!(fixture.version, 1, "unexpected fixture version");
    assert!(!fixture.cases.is_empty(), "fixture has no cases");

    let mut failures = Vec::new();
    for case in &fixture.cases {
        let reference = match case.input.kind.as_str() {
            "raw" => Reference::Raw(case.input.value.clone()),
            "username" => Reference::Username(case.input.value.clone()),
            "id" => Reference::Id(case.input.value.clone()),
            "url" => Reference::Url(case.input.value.clone()),
            kind => panic!("{}: unknown reference kind {kind}", case.name),
        };
        let result = resolve_reference(&reference, &case.platform, &case.resource);
        let outcome = match (&result, &case.error, &case.identifier) {
            (Ok(identifier), None, Some(expected)) if identifier == expected => continue,
            (Err(error), Some(expected), _) if error_name(error) == expected => continue,
            (Ok(identifier), _, _) => format!("resolved to {identifier:?}"),
            (Err(error), _, _) => format!("failed with {} ({error})", error_name(error)),
        };
        failures.push(format!(
            "{}: {outcome}, want {}",
            case.name,
            case.error
                .clone()
                .or_else(|| case.identifier.clone())
                .unwrap_or_default()
        ));
    }
    assert!(
        failures.is_empty(),
        "reference conformance failures:\n{}",
        failures.join("\n")
    );
}

fn error_name(error: &Error) -> &'static str {
    match error {
        Error::ReferenceMismatch(_) => "reference_mismatch",
        Error::Reference(_) => "invalid_reference",
        _ => "unexpected_error",
    }
}
