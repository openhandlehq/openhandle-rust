use openhandle::{Client, Error, Id, InstagramPostResponse, TikTokProfileResponse, Url};

pub async fn get_explicit_references(
    client: &Client,
) -> Result<(TikTokProfileResponse, InstagramPostResponse), Error> {
    let profile = client
        .tiktok()
        .profile(Id::new("920000000001"))
        .get(None)
        .await?;
    let post = client
        .instagram()
        .post(Url::new("https://www.instagram.com/p/Db04otPRpRH/"))
        .get(None)
        .await?;
    Ok((profile, post))
}
