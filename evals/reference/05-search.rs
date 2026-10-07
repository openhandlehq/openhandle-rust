use openhandle::{Client, Error, Freshness, TikTokSearchPostsOptions, TikTokSearchPostsPage};

pub async fn search_posts(client: &Client) -> Result<TikTokSearchPostsPage, Error> {
    let options = TikTokSearchPostsOptions::new("synthetic").freshness(Freshness::TwentyFourHours);
    client.tiktok().search().posts().list(options).await
}
