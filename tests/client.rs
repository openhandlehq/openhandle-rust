mod support;

use std::time::Duration;

use openhandle::{
    Client, Error, FetchOptions, FetchResource, Freshness, Id, InstagramProfileOptions, Platform,
    Source, TikTokSearchPostsOptions, TwitterProfileOptions, Url, Username,
};
use support::{MockResponse, MockServer};

const PROFILE_JSON: &str = r#"{"platform":"instagram","resource":"profile","capturedAt":"2026-08-27T12:00:00Z","source":"cache","data":{"id":"profile_1","handle":"northstar_forge_test","displayName":"Northstar","isVerified":false,"metrics":{}}}"#;

fn client(server: &MockServer, max_retries: u32) -> Client {
    Client::builder("oh_test_key")
        .base_url(server.url())
        .max_retries(max_retries)
        .build()
        .expect("client configuration is valid")
}

fn page_json(id: &str, cursor: Option<&str>) -> String {
    let next = cursor.map_or("null".to_owned(), |cursor| format!("{cursor:?}"));
    format!(
        r#"{{"platform":"instagram","resource":"post","capturedAt":"2026-08-27T12:00:00Z","source":"cache","data":[{{"id":"{id}"}}],"meta":{{"cursors":{{"next":{next}}}}}}}"#
    )
}

#[test]
fn builder_rejects_an_invalid_configuration() {
    let cases = [
        Client::builder(" ").build(),
        Client::builder("oh_test_key").base_url("relative").build(),
        Client::builder("oh_test_key")
            .base_url("ftp://api.openhandle.dev")
            .build(),
        Client::builder("oh_test_key")
            .timeout(Duration::ZERO)
            .build(),
    ];
    for result in cases {
        assert!(
            matches!(result, Err(Error::InvalidArgument(_))),
            "{result:?}"
        );
    }
}

#[tokio::test]
async fn profile_get_sends_key_freshness_and_returns_metadata() {
    let server = MockServer::start(|_, _| {
        MockResponse::json(PROFILE_JSON)
            .header("X-Request-ID", "request_123")
            .header("Openhandle-Cost", "0.000")
            .header("Openhandle-Environment", "test")
            .header("Openhandle-Billing-Disposition", "test")
            .header("Openhandle-Dataset-Version", "2026-09-24.1")
            .header("Openhandle-List-Price", "0.0005")
    })
    .await;

    let response = client(&server, 0)
        .instagram()
        .profile("northstar_forge_test")
        .get(InstagramProfileOptions::new().freshness(Freshness::TwentyFourHours))
        .await
        .expect("profile response");

    let request = &server.requests()[0];
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.path,
        "/v1/instagram/profiles/%40northstar_forge_test"
    );
    assert_eq!(request.query_value("freshness"), Some("24h"));
    assert_eq!(request.header("Authorization"), Some("Bearer oh_test_key"));
    assert_eq!(
        request.header("X-OpenHandle-Client"),
        Some(concat!("openhandle-rust/", env!("CARGO_PKG_VERSION")))
    );
    assert_eq!(response.data.handle, "northstar_forge_test");
    assert_eq!(response.data.display_name.as_deref(), Some("Northstar"));
    assert_eq!(response.platform, Some(Platform::Instagram));
    assert_eq!(response.resource.as_deref(), Some("profile"));
    assert_eq!(response.source, Some(Source::Cache));
    assert_eq!(
        response.captured_at.map(|value| value.to_rfc3339()),
        Some("2026-08-27T12:00:00+00:00".to_owned())
    );
    assert_eq!(response.request_id.as_deref(), Some("request_123"));
    assert_eq!(response.billing.cost.as_deref(), Some("0.000"));
    assert_eq!(response.billing.environment.as_deref(), Some("test"));
    assert_eq!(response.billing.disposition.as_deref(), Some("test"));
    assert_eq!(
        response.billing.dataset_version.as_deref(),
        Some("2026-09-24.1")
    );
    assert_eq!(response.billing.list_price.as_deref(), Some("0.0005"));
}

#[tokio::test]
async fn numeric_profile_string_is_a_username_and_ids_are_explicit() {
    let server = MockServer::start(|_, _| MockResponse::json(PROFILE_JSON)).await;
    let client = client(&server, 0);

    client
        .instagram()
        .profile("12356")
        .get(None)
        .await
        .expect("username");
    client
        .instagram()
        .profile(Id::new("12356"))
        .get(None)
        .await
        .expect("id");
    client
        .instagram()
        .profile(Username::new("@12356"))
        .get(None)
        .await
        .expect("explicit username");

    let paths: Vec<String> = server
        .requests()
        .into_iter()
        .map(|request| request.path)
        .collect();
    assert_eq!(
        paths,
        [
            "/v1/instagram/profiles/%4012356",
            "/v1/instagram/profiles/12356",
            "/v1/instagram/profiles/%4012356",
        ]
    );
}

#[tokio::test]
async fn explicit_profile_url_is_parsed_locally() {
    let server = MockServer::start(|_, _| MockResponse::json(PROFILE_JSON)).await;

    client(&server, 0)
        .instagram()
        .profile(Url::new("https://www.instagram.com/openai/?hl=en"))
        .get(None)
        .await
        .expect("profile response");

    assert_eq!(
        server.requests()[0].path,
        "/v1/instagram/profiles/%40openai"
    );
}

#[tokio::test]
async fn reference_mismatch_fails_before_any_request() {
    let server = MockServer::start(|_, _| MockResponse::json(PROFILE_JSON)).await;

    let error = client(&server, 0)
        .instagram()
        .profile("https://x.com/openai")
        .get(None)
        .await
        .expect_err("a post URL is not a profile");

    let Error::ReferenceMismatch(mismatch) = error else {
        panic!("expected a reference mismatch, got {error:?}");
    };
    assert_eq!(mismatch.actual_platform, "twitter");
    assert_eq!(mismatch.expected_platform, "instagram");
    assert_eq!(server.request_count(), 0);
}

#[tokio::test]
async fn tiktok_short_link_selector_fails_before_any_request() {
    let server = MockServer::start(|_, _| MockResponse::json(PROFILE_JSON)).await;

    let error = client(&server, 0)
        .tiktok()
        .post("https://vm.tiktok.com/ZTUHFuaK6/")
        .get(None)
        .await
        .expect_err("short links need fetch");

    assert!(matches!(error, Error::Reference(_)), "{error:?}");
    assert_eq!(server.request_count(), 0);
}

#[tokio::test]
async fn retryable_api_error_is_retried_after_retry_after() {
    let server = MockServer::start(|_, index| {
        if index == 0 {
            return MockResponse::json(
                r#"{"error":{"code":"UPSTREAM_UNAVAILABLE","message":"try again","requestId":"request_retry","retryable":true}}"#,
            )
            .status(503)
            .header("Retry-After", "0.001");
        }
        MockResponse::json(PROFILE_JSON)
    })
    .await;

    client(&server, 1)
        .instagram()
        .profile("https://www.instagram.com/openai/")
        .get(None)
        .await
        .expect("second attempt succeeds");

    assert_eq!(server.request_count(), 2);
}

#[tokio::test]
async fn upstream_switched_is_not_retried() {
    let server = MockServer::start(|_, _| {
        MockResponse::json(
            r#"{"error":{"code":"UPSTREAM_SWITCHED","message":"restart the list","requestId":"request_switch","retryable":false}}"#,
        )
        .status(409)
    })
    .await;

    let error = client(&server, 2)
        .instagram()
        .profile("openai")
        .posts()
        .list(None)
        .await
        .expect_err("the cursor sequence changed");

    let api = error.as_api().expect("an API error");
    assert_eq!(api.code, "UPSTREAM_SWITCHED");
    assert!(!api.retryable);
    assert_eq!(server.request_count(), 1);
}

#[tokio::test]
async fn api_error_exposes_contract_fields() {
    let server = MockServer::start(|_, _| {
        MockResponse::json(
            r#"{"error":{"code":"INVALID_REFERENCE","message":"invalid","requestId":"request_body","retryable":false,"details":{"field":"identifier"}}}"#,
        )
        .status(400)
        .header("X-Request-ID", "request_header")
        .header("Retry-After", "7")
    })
    .await;

    let error = client(&server, 0)
        .instagram()
        .profile("openai")
        .get(None)
        .await
        .expect_err("the API rejected the request");

    let Error::Api(api) = error else {
        panic!("expected an API error, got {error:?}");
    };
    assert_eq!(api.code, "INVALID_REFERENCE");
    assert_eq!(api.message, "invalid");
    assert_eq!(api.request_id.as_deref(), Some("request_body"));
    assert_eq!(api.status, Some(400));
    assert!(!api.retryable);
    assert_eq!(api.retry_after, Some(Duration::from_secs(7)));
    assert_eq!(
        api.details
            .and_then(|details| details.get("field").cloned()),
        Some(serde_json::json!("identifier"))
    );
}

#[tokio::test]
async fn non_json_error_falls_back_to_the_http_status() {
    let server = MockServer::start(|_, _| {
        MockResponse::json("bad gateway")
            .status(502)
            .header("X-Request-ID", "request_gateway")
    })
    .await;

    let error = client(&server, 0)
        .instagram()
        .profile("openai")
        .get(None)
        .await
        .expect_err("the gateway failed");

    let api = error.as_api().expect("an API error");
    assert_eq!(api.code, "HTTP_502");
    assert_eq!(api.request_id.as_deref(), Some("request_gateway"));
    assert!(api.retryable);
}

#[tokio::test]
async fn page_next_repeats_the_operation_with_the_opaque_cursor() {
    let server = MockServer::start(|request, index| {
        let body = match (index, request.query_value("cursor")) {
            (0, None) => page_json("post_1", Some("opaque+cursor/1")),
            (1, Some("opaque+cursor/1")) => page_json("post_2", None),
            _ => return MockResponse::json("{}").status(400),
        };
        MockResponse::json(body).header("X-Request-ID", &format!("request_{index}"))
    })
    .await;

    let page = client(&server, 0)
        .instagram()
        .profile("northstar_forge_test")
        .posts()
        .list(None)
        .await
        .expect("first page");
    assert!(page.has_next_page());
    assert_eq!(page.next_cursor(), Some("opaque+cursor/1"));

    let next = page
        .next()
        .await
        .expect("second page")
        .expect("a second page exists");
    assert_eq!(next.data[0].id, "post_2");
    assert_eq!(next.request_id.as_deref(), Some("request_1"));
    assert!(!next.has_next_page());

    let last = next.next().await.expect("no request at the end");
    assert!(last.is_none());
    assert_eq!(server.request_count(), 2);
}

#[tokio::test]
async fn items_request_no_page_before_the_first_item() {
    let server = MockServer::start(|_, _| MockResponse::json(page_json("post_1", None))).await;
    let client = client(&server, 0);

    let mut items = client
        .instagram()
        .profile("northstar_forge_test")
        .posts()
        .items(None);
    assert_eq!(server.request_count(), 0);

    let first = items
        .next()
        .await
        .expect("one item")
        .expect("item succeeds");
    assert_eq!(first.id, "post_1");
    assert!(items.next().await.is_none());
    assert_eq!(server.request_count(), 1);
}

#[tokio::test]
async fn items_follow_cursors_one_page_at_a_time() {
    let server = MockServer::start(|request, _| {
        let body = match request.query_value("cursor") {
            None => page_json("post_1", Some("cursor_2")),
            Some(_) => page_json("post_2", None),
        };
        MockResponse::json(body)
    })
    .await;
    let client = client(&server, 0);
    let mut items = client
        .instagram()
        .profile("northstar_forge_test")
        .posts()
        .items(None);

    let first = items
        .next()
        .await
        .expect("first item")
        .expect("item succeeds");
    assert_eq!((first.id.as_str(), server.request_count()), ("post_1", 1));
    let second = items
        .next()
        .await
        .expect("second item")
        .expect("item succeeds");
    assert_eq!((second.id.as_str(), server.request_count()), ("post_2", 2));
    assert!(items.next().await.is_none());
}

#[tokio::test]
async fn items_end_after_an_error() {
    let server = MockServer::start(|_, _| {
        MockResponse::json(r#"{"error":{"code":"UPSTREAM_SWITCHED","message":"restart","requestId":"request_1","retryable":false}}"#)
            .status(409)
    })
    .await;
    let client = client(&server, 0);
    let mut items = client
        .instagram()
        .profile("northstar_forge_test")
        .posts()
        .items(None);

    let error = items
        .next()
        .await
        .expect("an error item")
        .expect_err("the list failed");
    assert_eq!(
        error.as_api().map(|api| api.code.as_str()),
        Some("UPSTREAM_SWITCHED")
    );
    assert!(items.next().await.is_none());
    assert_eq!(server.request_count(), 1);
}

#[tokio::test]
async fn per_request_timeout_ends_the_attempt() {
    let server =
        MockServer::start(|_, _| MockResponse::json(PROFILE_JSON).delay(Duration::from_secs(1)))
            .await;

    let error = client(&server, 0)
        .twitter()
        .profile("openai")
        .get(
            TwitterProfileOptions::new()
                .max_retries(0)
                .timeout(Duration::from_millis(20)),
        )
        .await
        .expect_err("the request timed out");

    let api = error.as_api().expect("a transport error");
    assert_eq!(api.code, "TRANSPORT_ERROR");
    assert!(!api.retryable);
}

#[tokio::test]
async fn client_timeout_applies_to_a_custom_http_client() {
    let server =
        MockServer::start(|_, _| MockResponse::json(PROFILE_JSON).delay(Duration::from_secs(1)))
            .await;
    let client = Client::builder("oh_test_key")
        .base_url(server.url())
        .http_client(reqwest::Client::new())
        .timeout(Duration::from_millis(20))
        .max_retries(0)
        .build()
        .expect("client configuration is valid");

    let error = client
        .instagram()
        .profile("openai")
        .get(None)
        .await
        .expect_err("the request timed out");

    assert_eq!(
        error.as_api().map(|api| api.code.as_str()),
        Some("TRANSPORT_ERROR")
    );
}

#[tokio::test]
async fn search_sends_required_query_and_rejects_an_empty_one_locally() {
    let server = MockServer::start(|_, _| MockResponse::json(page_json("post_1", None))).await;
    let client = client(&server, 0);

    client
        .tiktok()
        .search()
        .posts()
        .list(TikTokSearchPostsOptions::new("synthetic").freshness(Freshness::SevenDays))
        .await
        .expect("search page");
    let error = client
        .tiktok()
        .search()
        .posts()
        .list(TikTokSearchPostsOptions::new(""))
        .await
        .expect_err("q is required");

    let request = &server.requests()[0];
    assert_eq!(request.path, "/v1/tiktok/search/posts");
    assert_eq!(request.query_value("q"), Some("synthetic"));
    assert_eq!(request.query_value("freshness"), Some("7d"));
    assert!(matches!(error, Error::InvalidArgument(_)), "{error:?}");
    assert_eq!(server.request_count(), 1);
}

#[tokio::test]
async fn fetch_posts_the_url_and_decodes_the_concrete_resource() {
    let server = MockServer::start(|_, _| MockResponse::json(PROFILE_JSON)).await;

    let response = client(&server, 0)
        .fetch(
            "https://www.instagram.com/openai/",
            FetchOptions::new().freshness(Freshness::TwentyFourHours),
        )
        .await
        .expect("fetch response");

    let request = &server.requests()[0];
    assert_eq!(
        (request.method.as_str(), request.path.as_str()),
        ("POST", "/v1/urls/fetch")
    );
    assert_eq!(
        request.json(),
        serde_json::json!({"url": "https://www.instagram.com/openai/", "freshness": "24h"})
    );
    let FetchResource::InstagramProfile(profile) = response.data else {
        panic!("expected an Instagram profile, got {:?}", response.data);
    };
    assert_eq!(profile.handle, "northstar_forge_test");
}

#[tokio::test]
async fn fetch_decodes_stories_and_highlights_by_resource() {
    let cases = [
        ("highlight", "17900000000000001"),
        ("story", "3100000000000000001"),
    ];
    for (resource, id) in cases {
        let body = format!(
            r#"{{"platform":"instagram","resource":"{resource}","capturedAt":"2026-08-27T12:00:00Z","source":"live","data":{{"id":"{id}","stories":null}}}}"#
        );
        let server = MockServer::start(move |_, _| MockResponse::json(body.clone())).await;

        let response = client(&server, 0)
            .fetch(
                "https://www.instagram.com/stories/highlights/17900000000000001/",
                None,
            )
            .await
            .expect("fetch response");

        let decoded = match &response.data {
            FetchResource::InstagramHighlight(highlight) => ("highlight", highlight.id.as_str()),
            FetchResource::InstagramStory(story) => ("story", story.id.as_str()),
            other => panic!("unexpected variant {other:?}"),
        };
        assert_eq!(decoded, (resource, id));
    }
}

#[test]
fn operation_futures_and_item_streams_can_move_across_threads() {
    fn assert_send<T: Send>(_: &T) {}
    let client = Client::new("oh_test_key").expect("client configuration is valid");
    let profile = client.instagram().profile("openai");
    let posts = profile.posts();

    let get = profile.get(None);
    let list = posts.list(None);
    let items = posts.items(None);
    let fetch = client.fetch("https://www.instagram.com/openai/", None);

    assert_send(&get);
    assert_send(&list);
    assert_send(&items);
    assert_send(&fetch);
}

#[tokio::test]
async fn follower_page_keeps_the_limited_flag_without_a_cursor() {
    for limited in [false, true] {
        let body = format!(
            r#"{{"platform":"instagram","resource":"profile","data":[],"meta":{{"cursors":{{"next":null}},"isLimited":{limited}}}}}"#
        );
        let server = MockServer::start(move |_, _| MockResponse::json(body.clone())).await;

        let page = client(&server, 0)
            .instagram()
            .profile("example")
            .followers()
            .list(None)
            .await
            .expect("follower page");

        assert_eq!(page.meta.is_limited, Some(limited));
        assert!(!page.has_next_page());
    }
}
