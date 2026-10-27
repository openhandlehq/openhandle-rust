mod support;

use openhandle::{
    Client, Error, FetchResource, Url, YouTubePostCommentRepliesOptions, YouTubePostFrameOptions,
};
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

#[tokio::test]
async fn post_files_decode_every_stream() {
    let server = MockServer::start(|_, _| {
        MockResponse::json(
            r#"{"platform":"youtube","resource":"files","data":{"postId":"dQw4w9WgXcQ","author":null,"duration":213,"renditions":[{"itag":140,"quality":"audio","mimeType":"audio/mp4","hasVideo":false,"hasAudio":true,"size":3449447,"url":"https://media.openhandle.dev/b"}]}}"#,
        )
    })
    .await;

    let files = client(&server)
        .youtube()
        .post("https://www.youtube.com/shorts/dQw4w9WgXcQ")
        .files()
        .get(None)
        .await
        .expect("post files");

    assert_eq!(
        server.requests()[0].path,
        "/v1/youtube/posts/dQw4w9WgXcQ/files"
    );
    assert_eq!(files.data.post_id, "dQw4w9WgXcQ");
    let audio = &files.data.renditions[0];
    assert_eq!(audio.itag, 140);
    assert!(audio.has_audio && !audio.has_video);
    assert_eq!(audio.size, Some(3449447));
}

#[tokio::test]
async fn post_storyboard_decodes_sheets_and_frame_positions() {
    let server = MockServer::start(|_, _| {
        MockResponse::json(
            r#"{"platform":"youtube","resource":"storyboard","data":{"postId":"dQw4w9WgXcQ","author":null,"intervalMs":2000,"frameWidth":160,"frameHeight":90,"columns":5,"rows":5,"frameCount":2,"sheets":[{"index":0,"url":"https://media.openhandle.dev/s","width":800,"height":90,"startMs":0,"frameCount":2}],"frames":[{"timeMs":0,"sheet":0,"x":0,"y":0},{"timeMs":2000,"sheet":0,"x":160,"y":0}]}}"#,
        )
    })
    .await;

    let storyboard = client(&server)
        .youtube()
        .post("dQw4w9WgXcQ")
        .storyboard()
        .get(None)
        .await
        .expect("post storyboard");

    assert_eq!(
        server.requests()[0].path,
        "/v1/youtube/posts/dQw4w9WgXcQ/storyboard"
    );
    assert_eq!(storyboard.data.sheets.len(), 1);
    assert_eq!(storyboard.data.frames[1].x, 160);
}

#[tokio::test]
async fn post_frame_sends_the_moment_in_seconds() {
    let server = MockServer::start(|_, _| {
        MockResponse::json(
            r#"{"platform":"youtube","resource":"frame","data":{"postId":"dQw4w9WgXcQ","at":43,"renditions":[{"quality":"1080p","height":1080,"url":"https://media.openhandle.dev/f"}]}}"#,
        )
    })
    .await;

    let frame = client(&server)
        .youtube()
        .post("dQw4w9WgXcQ")
        .frame()
        .get(YouTubePostFrameOptions::new(43))
        .await
        .expect("post frame");

    let request = &server.requests()[0];
    assert_eq!(request.path, "/v1/youtube/posts/dQw4w9WgXcQ/frame");
    assert!(request.query.contains(&("at".to_owned(), "43".to_owned())));
    assert_eq!(frame.data.at, 43);
    assert_eq!(frame.data.renditions[0].quality.as_deref(), Some("1080p"));
}
