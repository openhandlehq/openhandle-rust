# Openhandle Rust SDK

The official Rust client for the Openhandle API. It reads public Instagram,
TikTok, X, and Reddit data. The crate is async, runs on Tokio, and needs Rust
1.85 or newer.

## Installation

```bash
cargo add openhandle
cargo add tokio --features macros,rt-multi-thread
```

## Usage

[Create a free account](https://app.openhandle.dev/signup). You get 100 free
requests, and no card is needed. Create a Test key in the dashboard, store it
as `OPENHANDLE_TEST_KEY`, and create one client that you reuse:

```rust
use openhandle::{Client, Freshness, InstagramProfileOptions};

#[tokio::main]
async fn main() -> Result<(), openhandle::Error> {
    let client = Client::new(std::env::var("OPENHANDLE_TEST_KEY").unwrap_or_default())?;
    let profile = client.instagram().profile("northstar_forge_test");

    let response = profile
        .get(InstagramProfileOptions::new().freshness(Freshness::TwentyFourHours))
        .await?;
    let posts = profile.posts().list(None).await?;

    println!("{} {}", response.data.handle, posts.data.len());
    println!("{:?} {:?}", response.request_id, response.billing.cost);
    Ok(())
}
```

The key picks the environment. Keys that start with `oh_test_` return free
synthetic data. Keys that start with `oh_live_` read real public data and are
billed. Keep keys on your server. Never ship one in a browser or mobile app.

## Freshness and pricing

Every read takes a freshness value. It sets how old the data may be. Older
cached answers cost less.

| Freshness                     | Price per answered request |
| ----------------------------- | -------------------------- |
| `Freshness::Live`             | $0.0025                    |
| `Freshness::TwentyFourHours`  | $0.0005 (the default)      |
| `Freshness::SevenDays`        | $0.0001                    |
| `Freshness::ThirtyDays`       | Free                       |

`response.source` tells you if the answer came from the platform just now
(`Source::Live`) or from the cache (`Source::Cache`).

## Resource selection

Build a call by choosing a platform, a resource, and an operation:

```text
client.<platform>().<resource>(reference).<subresource>().<operation>(options)
```

Only `get`, `list`, `search`, and `fetch` send a request.
You can select a resource once and reuse it:

```rust
let post = client.instagram().post("Db04otPRpRH");

let details = post.get(None).await?;
let comments = post.comments().list(None).await?;
let replies = post.comment("18120112390529134").replies().list(None).await?;
```

For profiles, a plain string is a username, even when it contains only digits.
You can also pass a supported link. The SDK reads the link locally before sending a request:

```rust
use openhandle::{Id, Url, Username};

client.instagram().profile("openai");
client.instagram().profile("https://www.instagram.com/openai/");
client.instagram().profile(Username::new("12356"));
client.instagram().profile(Id::new("25025320"));
client.instagram().profile(Url::new("https://www.instagram.com/openai/"));
```

The SDK never guesses an ID from a raw string. `profile("12356")` is the
username `12356`, and `profile(Id::new("12356"))` is the platform ID `12356`.
IDs are always strings, so a number does not compile. Post and comment
selectors take `Id` and `Url` but not `Username`, and the compiler checks
that too.

A link for the wrong platform or resource fails the first operation with
`Error::ReferenceMismatch`, before any request. TikTok short links such as
`tiktok.com/t/…` and `vm.tiktok.com/…` are rejected by selectors. Pass them to
`fetch`, which expands them on the server.

Use `fetch` when you do not know what a link points to:

```rust
use openhandle::FetchResource;

let response = client.fetch("https://www.instagram.com/p/Db04otPRpRH/", None).await?;
if let FetchResource::InstagramPost(post) = response.data {
    println!("{}", post.id);
}
```

## Options

Each operation takes one options value, or `None` for the defaults. Options
have a constructor and setters:

```rust
use std::time::Duration;
use openhandle::{Freshness, TikTokSearchPostsOptions, TwitterProfileOptions};

let results = client
    .tiktok()
    .search()
    .posts()
    .list(TikTokSearchPostsOptions::new("synthetic").freshness(Freshness::SevenDays))
    .await?;

let profile = client
    .twitter()
    .profile("openai")
    .get(TwitterProfileOptions::new().max_retries(0).timeout(Duration::from_secs(5)))
    .await?;
```

Required values, such as the search text `q`, are constructor arguments.

## Pagination

A `list` or `search` call returns one typed page. The page keeps the opaque
cursor and can ask for the next page with the same options:

```rust
let mut page = client.instagram().profile("northstar_forge_test").posts().list(None).await?;
loop {
    for post in &page.data {
        println!("{}", post.id);
    }
    match page.next().await? {
        Some(next) => page = next,
        None => break,
    }
}
```

Use `items` to loop through results. It loads the next page when needed
and implements `futures::Stream`:

```rust
let mut posts = client.instagram().profile("northstar_forge_test").posts().items(None);
while let Some(post) = posts.next().await {
    println!("{}", post?.id);
}
```

The stream never restarts a list by itself. After `UPSTREAM_SWITCHED` it
returns the error and ends, and you start the list again from the first page.

## Responses

Each response contains the requested data and metadata, including the request ID and cost.
The `data` field is a typed model from `openhandle::models`:

```rust
let response = client.instagram().profile("northstar_forge_test").get(None).await?;

response.platform;     // Some(Platform::Instagram)
response.resource;     // Some("profile")
response.captured_at;  // when the data was read
response.source;       // Some(Source::Live) or Some(Source::Cache)
response.request_id;   // include it in logs and support requests
response.billing.cost; // the charge, as a decimal string
```

A missing value is `None`. It is never `0`. Unknown fields from newer API
versions are ignored, so an older SDK keeps working.

## Errors and retries

Every failure is an `openhandle::Error`. Branch on `code`, never on
`message`:

```rust
use openhandle::Error;

match client.instagram().profile("quiet_harbor_test").get(None).await {
    Ok(response) => println!("{}", response.data.handle),
    Err(Error::Api(error)) => {
        eprintln!("{} {:?} {}", error.code, error.request_id, error.retryable);
    }
    Err(error) => eprintln!("{error}"),
}
```

`ApiError` holds `code`, `message`, `request_id`, `retryable`, `retry_after`,
`status`, `details`, and `cause`. `Error::Reference` and
`Error::ReferenceMismatch` mean a reference was invalid. No request was sent.

The client retries retryable failures twice by default. It follows
`Retry-After`, waits longer after each try with some random spread, and stops
at the timeout. Drop the future to cancel a request.

## Configuration

```rust
use std::time::Duration;

let client = openhandle::Client::builder(api_key)
    .base_url("https://api.openhandle.dev")
    .max_retries(2)
    .timeout(Duration::from_secs(30))
    .build()?;
```

You can also pass your own `reqwest::Client` with `.http_client(...)`. TLS uses
rustls.

## Generation

The pinned [`openapi/openhandle.json`](./openapi/openhandle.json) contract is
the source for every model, options type, response type, and resource method.
The generated code in `src/generated` is committed, so you do not need the
generator to use the crate.

```bash
cargo xtask generate
cargo xtask generate --check
cargo test
```

## Versions

The crate version is the version of the API contract it was generated from.
Every change, including an SDK-only fix, ships as a new patch version.

## License

[MIT](./LICENSE)
