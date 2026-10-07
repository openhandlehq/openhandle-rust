use openhandle::{Client, Error, InstagramProfileResponse};

pub struct BillingMetadata {
    pub cost: Option<String>,
    pub environment: Option<String>,
    pub request_id: Option<String>,
}

pub async fn get_billing_metadata(client: &Client) -> Result<BillingMetadata, Error> {
    let response: InstagramProfileResponse = client
        .instagram()
        .profile("northstar_forge_test")
        .get(None)
        .await?;
    Ok(BillingMetadata {
        cost: response.billing.cost,
        environment: response.billing.environment,
        request_id: response.request_id,
    })
}
