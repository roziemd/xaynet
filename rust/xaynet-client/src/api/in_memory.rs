use crate::api::ApiClient;
use thiserror::Error;
use xaynet_core::{
    common::RoundParameters,
    mask::Model,
    SumDict,
    SumParticipantPublicKey,
    UpdateSeedDict,
};
use xaynet_server::services::{
    fetchers::{FetchError, Fetcher},
    messages::{PetMessageHandler, ServiceError},
};

/// A client that communicates with the coordinator's API via
/// in-memory channels.
pub struct InMemoryApiClient {
    fetcher: Box<dyn Fetcher + Send + Sync>,
    message_handler: PetMessageHandler,
}

impl InMemoryApiClient {
    #[allow(dead_code)]
    pub fn new(
        fetcher: impl Fetcher + 'static + Send + Sync,
        message_handler: PetMessageHandler,
    ) -> Self {
        Self {
            fetcher: Box::new(fetcher),
            message_handler: message_handler,
        }
    }
}

/// Error returned by an [`InMemoryApiClient`]
#[derive(Debug, Error)]
pub enum InMemoryApiClientError {
    #[error("a PET message could not be processed by the coordinator: {0}")]
    Message(#[from] ServiceError),

    #[error("failed to fetch data from the coordinator: {0}")]
    Fetch(#[from] FetchError),
}

#[async_trait]
impl ApiClient for InMemoryApiClient {
    type Error = InMemoryApiClientError;

    async fn get_round_params(&mut self) -> Result<RoundParameters, Self::Error> {
        Ok(self.fetcher.round_params().await?)
    }

    async fn get_sums(&mut self) -> Result<Option<SumDict>, Self::Error> {
        Ok(self.fetcher.sum_dict().await?.map(|arc| (*arc).clone()))
    }

    async fn get_seeds(
        &mut self,
        pk: SumParticipantPublicKey,
    ) -> Result<Option<UpdateSeedDict>, Self::Error> {
        Ok(self
            .fetcher
            .seed_dict()
            .await?
            .and_then(|dict| dict.get(&pk).cloned()))
    }

    async fn get_mask_length(&mut self) -> Result<Option<u64>, Self::Error> {
        Ok(self.fetcher.mask_length().await?.map(|res| res as u64))
    }

    async fn get_model(&mut self) -> Result<Option<Model>, Self::Error> {
        Ok(self.fetcher.model().await?.map(|arc| (*arc).clone()))
    }

    async fn send_message(&mut self, message: Vec<u8>) -> Result<(), Self::Error> {
        Ok(self.message_handler.handle_message(message).await?)
    }
}
