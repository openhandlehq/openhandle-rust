use openhandle::models::InstagramProfile;
use openhandle::{Client, Error, Freshness, InstagramProfileOptions};

pub async fn get_profile(client: &Client) -> Result<InstagramProfile, Error> {
    let response = client
        .instagram()
        .profile("northstar_forge_test")
        .get(InstagramProfileOptions::new().freshness(Freshness::TwentyFourHours))
        .await?;
    Ok(response.data)
}
