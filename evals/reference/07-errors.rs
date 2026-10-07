use openhandle::{Client, Error};

pub struct FailureMetadata {
    pub code: String,
    pub request_id: Option<String>,
    pub retryable: bool,
}

pub async fn failure_metadata(client: &Client) -> Option<FailureMetadata> {
    match client
        .instagram()
        .profile("quiet_harbor_test")
        .get(None)
        .await
    {
        Err(Error::Api(error)) => Some(FailureMetadata {
            code: error.code.clone(),
            request_id: error.request_id.clone(),
            retryable: error.retryable,
        }),
        _ => None,
    }
}
