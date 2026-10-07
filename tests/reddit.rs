mod support;

use openhandle::{Client, FetchResource};
use support::{MockResponse, MockServer};

fn client(server: &MockServer) -> Client {
    Client::builder("oh_test_sdk")
        .base_url(server.url())
        .max_retries(0)
        .build()
        .expect("client configuration is valid")
}

#[tokio::test]
async fn wiki_selectors_bind_a_nested_page_as_one_segment() {
    let server = MockServer::start(|request, _| match request.path.as_str() {
        "/v1/reddit/subreddits/python/wiki-pages" => MockResponse::json(
            r#"{"platform":"reddit","resource":"wiki","data":[{"title":"config/sidebar"}],"meta":{"cursors":{"next":null}}}"#,
        ),
        "/v1/reddit/subreddits/python/wiki-pages/config%2Fsidebar" => MockResponse::json(
            r#"{"platform":"reddit","resource":"wiki","data":{"title":"config/sidebar","content":{"markdown":"Sidebar content"}}}"#,
        ),
        _ => MockResponse::json("{}").status(404),
    })
    .await;
    let client = client(&server);
    let subreddit = client.reddit().subreddit("python");

    let pages = subreddit.wiki_pages().list(None).await.expect("wiki pages");
    let page = subreddit
        .wiki_page("config/sidebar")
        .get(None)
        .await
        .expect("wiki page");

    assert_eq!(pages.data.len(), 1);
    assert_eq!(pages.data[0].title, "config/sidebar");
    assert_eq!(page.data.title, "config/sidebar");
}

#[tokio::test]
async fn subreddit_url_selects_the_subreddit_name() {
    let server = MockServer::start(|_, _| {
        MockResponse::json(r#"{"platform":"reddit","resource":"subreddit","data":{}}"#)
    })
    .await;

    client(&server)
        .reddit()
        .subreddit("https://www.reddit.com/r/python/")
        .get(None)
        .await
        .expect("subreddit");

    assert_eq!(server.requests()[0].path, "/v1/reddit/subreddits/python");
}

#[tokio::test]
async fn fetch_decodes_a_reddit_profile() {
    let server = MockServer::start(|_, _| {
        MockResponse::json(
            r#"{"platform":"reddit","resource":"profile","data":{"handle":"reddit-user","karma":{"comment":-2}}}"#,
        )
    })
    .await;

    let response = client(&server)
        .fetch("https://reddit.com/user/reddit-user", None)
        .await
        .expect("fetch response");

    let FetchResource::RedditProfile(profile) = response.data else {
        panic!("expected a Reddit profile, got {:?}", response.data);
    };
    assert_eq!(profile.handle, "reddit-user");
}
