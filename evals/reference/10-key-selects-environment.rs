use openhandle::{Client, Error};

pub fn create_client(api_key: &str) -> Result<Client, Error> {
    Client::new(api_key)
}
