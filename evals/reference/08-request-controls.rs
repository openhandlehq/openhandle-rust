use std::time::Duration;

use openhandle::{Client, Error, TwitterProfileOptions, TwitterProfileResponse};

pub async fn get_with_controls(client: &Client) -> Result<TwitterProfileResponse, Error> {
    client
        .twitter()
        .profile("northstar_test")
        .get(
            TwitterProfileOptions::new()
                .max_retries(0)
                .timeout(Duration::from_secs(5)),
        )
        .await
}
