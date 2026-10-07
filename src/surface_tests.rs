use std::collections::BTreeMap;

use serde_json::Value;

use crate::Client;
use crate::generated::surface::call_every_operation;
use crate::test_support::{MockResponse, MockServer};

struct ExpectedOperation {
    sdk_path: String,
    data_is_array: bool,
}

fn expected_operations() -> BTreeMap<String, ExpectedOperation> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/openapi/openhandle.json");
    let document: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("read the pinned contract"))
            .expect("the pinned contract is JSON");
    let mut expected = BTreeMap::new();
    for (api_path, item) in document["paths"].as_object().expect("paths") {
        for (method, operation) in item.as_object().expect("path item") {
            if !["get", "post", "put", "patch", "delete"].contains(&method.as_str()) {
                continue;
            }
            let mut bound = String::new();
            for (index, part) in api_path.split('{').enumerate() {
                match part.split_once('}') {
                    Some((name, rest)) if index > 0 => {
                        bound.push_str(&format!("sample-{name}{rest}"));
                    }
                    _ => bound.push_str(part),
                }
            }
            let schema = &operation["responses"]["200"]["content"]["application/json"]["schema"];
            let data_is_array = schema["allOf"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|part| part["properties"]["data"]["type"] == "array");
            let key = format!("{} {bound}", method.to_uppercase());
            let previous = expected.insert(
                key.clone(),
                ExpectedOperation {
                    sdk_path: operation["x-openhandle-sdk"]["path"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    data_is_array,
                },
            );
            assert!(previous.is_none(), "{key} appears twice in the contract");
        }
    }
    expected
}

#[tokio::test]
async fn every_openapi_operation_is_reachable_once_through_the_resource_graph() {
    let expected = expected_operations();
    let shapes: BTreeMap<String, bool> = expected
        .iter()
        .map(|(key, operation)| (key.clone(), operation.data_is_array))
        .collect();
    let server = MockServer::start(move |request, _| {
        let key = format!("{} {}", request.method, request.path);
        let body = match (request.path.as_str(), shapes.get(&key)) {
            ("/v1/urls/fetch", _) => r#"{"platform":"instagram","resource":"profile","data":{}}"#,
            (_, Some(true)) => r#"{"data":[],"meta":{"cursors":{"next":null}}}"#,
            (_, Some(false)) => r#"{"data":{}}"#,
            (_, None) => return MockResponse::json("{}").status(404),
        };
        MockResponse::json(body)
    })
    .await;
    let client = Client::builder("oh_test_surface")
        .base_url(server.url())
        .max_retries(0)
        .build()
        .expect("client");

    let results = call_every_operation(&client).await;

    let failures: Vec<String> = results
        .iter()
        .filter_map(|(path, result)| {
            result
                .as_ref()
                .err()
                .map(|error| format!("{path}: {error}"))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "operations failed:\n{}",
        failures.join("\n")
    );

    let mut requested: Vec<String> = server
        .requests()
        .iter()
        .map(|request| format!("{} {}", request.method, request.path))
        .collect();
    requested.sort();
    let expected_keys: Vec<String> = expected.keys().cloned().collect();
    assert_eq!(
        requested, expected_keys,
        "requests do not cover every operation exactly once"
    );

    let mut called: Vec<&str> = results.iter().map(|(path, _)| *path).collect();
    called.sort_unstable();
    let mut expected_paths: Vec<&str> = expected
        .values()
        .map(|operation| operation.sdk_path.as_str())
        .collect();
    expected_paths.sort_unstable();
    assert_eq!(
        called, expected_paths,
        "generated calls do not match the SDK paths in the contract"
    );
}
