use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;
use sha2::Sha256;

use super::{VerifyOptions, WebhookErrorCode, verify};
use crate::generated::models::WebhookEvent;

const FIXTURE: &str = include_str!("../../testdata/webhook-conformance.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    version: u32,
    secret: String,
    default_tolerance_seconds: u64,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    body: String,
    headers: BTreeMap<String, String>,
    now: u64,
    tolerance_seconds: Option<u64>,
    event: Option<serde_json::Value>,
    error: Option<String>,
}

#[test]
fn every_shared_webhook_case_verifies_as_the_fixture_requires() {
    let fixture: Fixture = serde_json::from_str(FIXTURE).expect("fixture is valid JSON");
    assert_eq!(fixture.version, 1, "unexpected fixture version");
    assert!(!fixture.cases.is_empty(), "fixture has no cases");

    let mut failures = Vec::new();
    for case in &fixture.cases {
        let mut headers = HeaderMap::new();
        for (name, value) in &case.headers {
            headers.insert(
                HeaderName::from_bytes(name.as_bytes()).expect("fixture header name"),
                HeaderValue::from_str(value).expect("fixture header value"),
            );
        }
        let tolerance = case
            .tolerance_seconds
            .unwrap_or(fixture.default_tolerance_seconds);
        let options = VerifyOptions::new()
            .tolerance(Duration::from_secs(tolerance))
            .now(UNIX_EPOCH + Duration::from_secs(case.now));
        let result = verify(case.body.as_bytes(), &headers, &fixture.secret, options);

        let outcome = match (&result, &case.event, &case.error) {
            (Ok(event), Some(expected), None) => {
                let expected: WebhookEvent = serde_json::from_value(expected.clone())
                    .expect("fixture event is a WebhookEvent");
                if *event == expected {
                    continue;
                }
                format!("returned {event:?}, want {expected:?}")
            }
            (Err(error), None, Some(expected)) if error.code().as_str() == expected => continue,
            (Ok(event), _, _) => format!("returned {event:?}, want {:?}", case.error),
            (Err(error), _, _) => format!("failed with {}, want {:?}", error.code(), case.error),
        };
        failures.push(format!("{}: {outcome}", case.name));
    }
    assert!(
        failures.is_empty(),
        "webhook conformance failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn signed_json_that_is_not_an_object_is_an_invalid_payload() {
    let secret = "whsec_array_body";
    let body = br#"[{"id":"826107ab-cbf7-4937-bd7b-f31c08be3283"}]"#;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC key");
    mac.update(b"1795521600.");
    mac.update(body);
    let signature: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();

    let mut headers = HeaderMap::new();
    headers.insert(
        "x-openhandle-timestamp",
        HeaderValue::from_static("1795521600"),
    );
    headers.insert(
        "x-openhandle-signature",
        HeaderValue::from_str(&format!("sha256={signature}")).expect("header value"),
    );
    let options =
        VerifyOptions::new().now(SystemTime::UNIX_EPOCH + Duration::from_secs(1795521600));

    let error = verify(body, &headers, secret, options).expect_err("array body must fail");
    assert_eq!(error.code(), WebhookErrorCode::InvalidPayload);
}
