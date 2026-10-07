use openhandle::{Client, Error, FetchOptions, FetchResource, Freshness};

pub async fn fetch_unknown_resource(client: &Client) -> Result<Option<String>, Error> {
    let response = client
        .fetch(
            "https://www.instagram.com/p/Db04otPRpRH/",
            FetchOptions::new().freshness(Freshness::TwentyFourHours),
        )
        .await?;
    Ok(match response.data {
        FetchResource::InstagramPost(post) => Some(post.id),
        _ => None,
    })
}
