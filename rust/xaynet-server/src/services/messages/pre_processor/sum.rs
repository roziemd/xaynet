use std::task::Poll;

use futures::{
    future::{ready, Ready},
    task::Context,
};
use tower::Service;
use xaynet_core::{
    common::RoundParameters,
    crypto::ByteObject,
    message::{Header, Message, Payload, Sum},
};

use crate::services::messages::pre_processor::{PreProcessorError, PreProcessorResponse};

/// Request type for [`SumPreProcessorService`]
pub type SumRequest = (Header, Sum, RoundParameters);

/// A service for performing sanity checks and preparing a sum request
/// to be handled by the state machine. At the moment, this is limited
/// to verifying the participant's eligibility for the sum task.
#[derive(Debug, Clone)]
pub struct SumPreProcessorService;

impl Service<SumRequest> for SumPreProcessorService {
    type Response = PreProcessorResponse;
    type Error = ::std::convert::Infallible;
    type Future = Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, (header, message, params): SumRequest) -> Self::Future {
        let pre_processor = SumPreProcessor {
            header,
            message,
            params,
        };
        ready(Ok(pre_processor.call()))
    }
}

struct SumPreProcessor {
    header: Header,
    message: Sum,
    params: RoundParameters,
}

impl SumPreProcessor {
    fn call(self) -> PreProcessorResponse {
        if !self.has_valid_sum_signature() {
            return Err(PreProcessorError::InvalidSumSignature);
        }
        if !self.is_eligible_for_sum_task() {
            return Err(PreProcessorError::NotSumEligible);
        }

        let Self {
            header, message, ..
        } = self;
        Ok(Message {
            header,
            payload: Payload::Sum(message),
        })
    }
    /// Check whether this request contains a valid sum signature
    fn has_valid_sum_signature(&self) -> bool {
        let seed = &self.params.seed;
        let signature = &self.message.sum_signature;
        let pk = &self.header.participant_pk;
        pk.verify_detached(&signature, &[seed.as_slice(), b"sum"].concat())
    }

    /// Check whether this request comes from a participant that is eligible for the sum task.
    fn is_eligible_for_sum_task(&self) -> bool {
        self.message.sum_signature.is_eligible(self.params.sum)
    }
}
