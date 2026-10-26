mod support;

use openhandle::{Client, Error, FetchResource, Url, YouTubePostCommentRepliesOptions};
use support::{MockResponse, MockServer};

fn client(server: &MockServer) -> Client {
    Client::builder("oh_test_sdk")
        .base_url(server.url())
        .max_retries(0)
        .build()
        .expect("client configuration is valid")
}

#[tokio::test]
async fn selectors_resolve_youtube_urls_and_bind_nested_comment_replies() {
    let server = MockServer::start(|_, _| {
        MockResponse::json(
            r#"{"platform":"youtube","resource":"post","data":[],"meta":{"cursors":{"next":null}}}"#,
        )
    })
    .await;
    let client = client(&server);

    client
        .youtube()
        .profile("https://www.youtube.com/@MrBeast/videos")
        .videos()
        .list(None)
        .await
        .expect("profile videos");
    client
        .youtube()
        .post("https://youtu.be/dQw4w9WgXcQ")
        .comment("UgzComment.123")
        .replies()
        .list(YouTubePostCommentRepliesOptions::new("replies-token"))
        .await
        .expect("comment replies");

    let requests = server.requests();
    assert_eq!(requests[0].path, "/v1/youtube/profiles/%40MrBeast/videos");
    assert_eq!(
        requests[1].path,
        "/v1/youtube/posts/dQw4w9WgXcQ/comments/UgzComment.123/replies"
    );
    assert!(
        requests[1]
            .query
            .contains(&("token".to_owned(), "replies-token".to_owned()))
    );
}

#[tokio::test]
async fn profile_selector_rejects_a_video_url_before_any_request() {
    let server = MockServer::start(|_, _| MockResponse::json("{}")).await;

    let error = client(&server)
        .youtube()
        .profile(Url::new("https://www.youtube.com/watch?v=dQw4w9WgXcQ"))
        .get(None)
        .await
        .expect_err("a video URL is not a profile");

    let Error::ReferenceMismatch(mismatch) = error else {
        panic!("expected a reference mismatch, got {error:?}");
    };
    assert_eq!(mismatch.actual_platform, "youtube");
    assert_eq!(mismatch.actual_resource, "post");
    assert_eq!(server.request_count(), 0);
}

#[tokio::test]
async fn fetch_decodes_a_youtube_profile_and_post_by_resource() {
    let server = MockServer::start(|request, _| {
        if String::from_utf8_lossy(&request.body).contains("youtu.be") {
            return MockResponse::json(
                r#"{"platform":"youtube","resource":"post","data":{"id":"dQw4w9WgXcQ"}}"#,
            );
        }
        MockResponse::json(
            r#"{"platform":"youtube","resource":"profile","data":{"id":"UCX6OQ3DkcsbYNE6H8uQQuVA","handle":"MrBeast"}}"#,
        )
    })
    .await;
    let client = client(&server);

    let profile = client
        .fetch("https://www.youtube.com/@MrBeast", None)
        .await
        .expect("profile fetch");
    let post = client
        .fetch("https://youtu.be/dQw4w9WgXcQ", None)
        .await
        .expect("post fetch");

    let FetchResource::YouTubeProfile(profile) = profile.data else {
        panic!("expected a YouTube profile, got {:?}", profile.data);
    };
    assert_eq!(profile.id, "UCX6OQ3DkcsbYNE6H8uQQuVA");
    let FetchResource::YouTubePost(post) = post.data else {
        panic!("expected a YouTube post, got {:?}", post.data);
    };
    assert_eq!(post.id, "dQw4w9WgXcQ");
}
