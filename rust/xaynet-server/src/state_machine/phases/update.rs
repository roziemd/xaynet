use std::sync::Arc;

use xaynet_core::{
    mask::{Aggregation, MaskObject},
    LocalSeedDict,
    SeedDict,
    SumDict,
    UpdateParticipantPublicKey,
};

use crate::state_machine::{
    events::{DictionaryUpdate, MaskLengthUpdate},
    phases::{Handler, Phase, PhaseName, PhaseState, Shared, StateError, Sum2},
    requests::{StateMachineRequest, UpdateRequest},
    StateMachine,
    StateMachineError,
};

#[cfg(feature = "metrics")]
use crate::metrics;

use tokio::time::{timeout, Duration};

/// Update state
#[derive(Debug)]
pub struct Update {
    /// The frozen sum dictionary built during the sum phase.
    frozen_sum_dict: SumDict,

    /// The seed dictionary built during the update phase.
    seed_dict: SeedDict,

    /// The aggregator for masked models.
    model_agg: Aggregation,

    /// The aggregator for masked scalars.
    scalar_agg: Aggregation,
}

#[cfg(test)]
impl Update {
    pub fn frozen_sum_dict(&self) -> &SumDict {
        &self.frozen_sum_dict
    }
    pub fn seed_dict(&self) -> &SeedDict {
        &self.seed_dict
    }
    pub fn aggregation(&self) -> &Aggregation {
        &self.model_agg
    }
}

#[async_trait]
impl Phase for PhaseState<Update>
where
    Self: Handler,
{
    const NAME: PhaseName = PhaseName::Update;

    /// Moves from the update state to the next state.
    ///
    /// See the [module level documentation](../index.html) for more details.
    async fn run(&mut self) -> Result<(), StateError> {
        let min_time = self.shared.state.min_update_time;
        debug!("in update phase for a minimum of {} seconds", min_time);
        self.process_during(Duration::from_secs(min_time)).await?;

        let time_left = self.shared.state.max_update_time - min_time;
        timeout(Duration::from_secs(time_left), self.process_until_enough()).await??;

        info!(
            "{} update messages handled (min {} required)",
            self.updater_count(),
            self.shared.state.min_update_count
        );
        Ok(())
    }

    fn next(self) -> Option<StateMachine> {
        let PhaseState {
            inner:
                Update {
                    frozen_sum_dict,
                    seed_dict,
                    model_agg,
                    scalar_agg,
                },
            mut shared,
        } = self;

        info!("broadcasting mask length");
        shared
            .io
            .events
            .broadcast_mask_length(MaskLengthUpdate::New(model_agg.len()));

        info!("broadcasting the global seed dictionary");
        shared
            .io
            .events
            .broadcast_seed_dict(DictionaryUpdate::New(Arc::new(seed_dict)));

        Some(PhaseState::<Sum2>::new(shared, frozen_sum_dict, model_agg, scalar_agg).into())
    }
}

impl PhaseState<Update>
where
    Self: Handler + Phase,
{
    /// Processes requests until there are enough.
    async fn process_until_enough(&mut self) -> Result<(), StateError> {
        while !self.has_enough_updates() {
            debug!(
                "{} update messages handled (min {} required)",
                self.updater_count(),
                self.shared.state.min_update_count
            );
            self.process_single().await?;
        }
        Ok(())
    }
}

impl Handler for PhaseState<Update> {
    /// Handles a [`StateMachineRequest`].
    ///
    /// If the request is a [`StateMachineRequest::Sum`] or
    /// [`StateMachineRequest::Sum2`] request, the request sender will
    /// receive a [`StateMachineError::MessageRejected`].
    fn handle_request(&mut self, req: StateMachineRequest) -> Result<(), StateMachineError> {
        match req {
            StateMachineRequest::Update(update_req) => {
                metrics!(
                    self.shared.io.metrics_tx,
                    metrics::message::update::increment(self.shared.state.round_id, Self::NAME)
                );
                self.handle_update(update_req)
            }
            _ => Err(StateMachineError::MessageRejected),
        }
    }
}

impl PhaseState<Update> {
    /// Creates a new update state.
    pub fn new(shared: Shared, frozen_sum_dict: SumDict, seed_dict: SeedDict) -> Self {
        info!("state transition");
        Self {
            inner: Update {
                frozen_sum_dict,
                seed_dict,
                model_agg: Aggregation::new(shared.state.mask_config, shared.state.model_size),
                // TODO separate config for scalars
                scalar_agg: Aggregation::new(shared.state.mask_config, 1),
            },
            shared,
        }
    }

    /// Handles an update request.
    /// If the handling of the update message fails, an error is returned to the request sender.
    fn handle_update(&mut self, req: UpdateRequest) -> Result<(), StateMachineError> {
        let UpdateRequest {
            participant_pk,
            local_seed_dict,
            masked_model,
            masked_scalar,
        } = req;
        self.update_seed_dict_and_aggregate_mask(
            &participant_pk,
            &local_seed_dict,
            masked_model,
            masked_scalar,
        )
    }

    /// Updates the local seed dict and aggregates the masked model.
    fn update_seed_dict_and_aggregate_mask(
        &mut self,
        pk: &UpdateParticipantPublicKey,
        local_seed_dict: &LocalSeedDict,
        masked_model: MaskObject,
        masked_scalar: MaskObject,
    ) -> Result<(), StateMachineError> {
        // Check if aggregation can be performed. It is important to
        // do that _before_ updating the seed dictionary, because we
        // don't want to add the local seed dict if the corresponding
        // masked model is invalid
        debug!("checking whether the masked model can be aggregated");
        self.inner
            .model_agg
            .validate_aggregation(&masked_model)
            .map_err(|e| {
                warn!("model aggregation error: {}", e);
                StateMachineError::AggregationFailed
            })?;

        debug!("checking whether the masked scalar can be aggregated");
        self.inner
            .scalar_agg
            .validate_aggregation(&masked_scalar)
            .map_err(|e| {
                warn!("scalar aggregation error: {}", e);
                StateMachineError::AggregationFailed
            })?;

        // Try to update local seed dict first. If this fail, we do
        // not want to aggregate the model.
        info!("updating the global seed dictionary");
        self.add_local_seed_dict(pk, local_seed_dict)
            .map_err(|err| {
                warn!("invalid local seed dictionary, ignoring update message");
                err
            })?;

        info!("aggregating the masked model and scalar");
        self.inner.model_agg.aggregate(masked_model);
        self.inner.scalar_agg.aggregate(masked_scalar);
        Ok(())
    }

    /// Adds a local seed dictionary to the seed dictionary.
    ///
    /// # Error
    /// Fails if it contains invalid keys or it is a repetition.
    fn add_local_seed_dict(
        &mut self,
        pk: &UpdateParticipantPublicKey,
        local_seed_dict: &LocalSeedDict,
    ) -> Result<(), StateMachineError> {
        if local_seed_dict.keys().len() == self.inner.frozen_sum_dict.keys().len()
            && local_seed_dict
                .keys()
                .all(|pk| self.inner.frozen_sum_dict.contains_key(pk))
            && self
                .inner
                .seed_dict
                .values()
                .next()
                .map_or(true, |dict| !dict.contains_key(pk))
        {
            debug!("adding local seed dictionary");
            for (sum_pk, seed) in local_seed_dict {
                self.inner
                    .seed_dict
                    .get_mut(sum_pk)
                    // FIXME: the error is not very adapted here, it's
                    // more an internal error. Could we not unwrap
                    // here per the checks above?
                    .ok_or(StateMachineError::InvalidLocalSeedDict)?
                    .insert(*pk, seed.clone());
            }
            Ok(())
        } else {
            warn!("invalid seed dictionary");
            Err(StateMachineError::InvalidLocalSeedDict)
        }
    }

    /// Returns the number of update participants that sent a valid update message.
    fn updater_count(&self) -> usize {
        self.inner
            .seed_dict
            .values()
            .next()
            .map(|dict| dict.len())
            .unwrap_or(0)
    }

    fn has_enough_updates(&self) -> bool {
        self.updater_count() >= self.shared.state.min_update_count
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::*;
    use crate::state_machine::{
        events::Event,
        tests::{builder::StateMachineBuilder, utils},
    };
    use xaynet_core::{
        common::RoundSeed,
        crypto::{ByteObject, EncryptKeyPair},
        mask::{FromPrimitives, MaskObject, Model},
        SumDict,
        UpdateSeedDict,
    };

    #[tokio::test]
    pub async fn update_to_sum2() {
        let n_updaters = 1;
        let n_summers = 1;
        let seed = RoundSeed::generate();
        let sum_ratio = 0.5;
        let update_ratio = 1.0;
        let coord_keys = EncryptKeyPair::generate();
        let model_size = 4;

        // Find a sum participant and an update participant for the
        // given seed and ratios.
        let mut summer = utils::generate_summer(&seed, sum_ratio, update_ratio);
        let updater = utils::generate_updater(&seed, sum_ratio, update_ratio);

        // Initialize the update phase state
        let sum_msg = summer.compose_sum_message(coord_keys.public);
        let summer_ephm_pk = utils::ephm_pk(&sum_msg);

        let mut frozen_sum_dict = SumDict::new();
        frozen_sum_dict.insert(summer.pk, summer_ephm_pk);

        let mut seed_dict = SeedDict::new();
        seed_dict.insert(summer.pk, HashMap::new());
        let aggregation = Aggregation::new(utils::mask_settings().into(), model_size);
        let scalar_agg = Aggregation::new(utils::mask_settings().into(), 1);
        let update = Update {
            frozen_sum_dict: frozen_sum_dict.clone(),
            seed_dict: seed_dict.clone(),
            model_agg: aggregation.clone(),
            scalar_agg,
        };

        // Create the state machine
        let (state_machine, request_tx, events) = StateMachineBuilder::new()
            .with_seed(seed.clone())
            .with_phase(update)
            .with_sum_ratio(sum_ratio)
            .with_update_ratio(update_ratio)
            .with_min_sum(n_summers)
            .with_min_update(n_updaters)
            .with_mask_config(utils::mask_settings().into())
            .build();

        assert!(state_machine.is_update());

        // Create an update request.
        let scalar = 1.0 / (n_updaters as f64 * update_ratio);
        let model = Model::from_primitives(vec![0; model_size].into_iter()).unwrap();
        let update_msg = updater.compose_update_message(
            coord_keys.public,
            &frozen_sum_dict,
            scalar,
            model.clone(),
        );
        let masked_model = utils::masked_model(&update_msg);
        let request_fut = async { request_tx.msg(&update_msg).await.unwrap() };

        // Have the state machine process the request
        let transition_fut = async { state_machine.next().await.unwrap() };
        let (_response, state_machine) = tokio::join!(request_fut, transition_fut);

        // Extract state of the state machine
        let PhaseState {
            inner: sum2_state, ..
        } = state_machine.into_sum2_phase_state();

        // Check the initial state of the sum2 phase.

        // The sum dict should be unchanged
        assert_eq!(sum2_state.sum_dict(), &frozen_sum_dict);
        // We have only one updater, so the aggregation should contain
        // the masked model from that updater
        assert_eq!(
            <Aggregation as Into<MaskObject>>::into(sum2_state.aggregation().clone().into()),
            masked_model
        );
        assert!(sum2_state.mask_dict().is_empty());

        // Check all the events that should be emitted during the update
        // phase
        assert_eq!(
            events.phase_listener().get_latest(),
            Event {
                round_id: 0,
                event: PhaseName::Update,
            }
        );
        assert_eq!(
            events.mask_length_listener().get_latest(),
            Event {
                round_id: 0,
                event: MaskLengthUpdate::New(model.len()),
            }
        );

        // Compute the global seed dictionary that we expect to be
        // broadcasted. It has a single entry for our sum
        // participant. That entry is an UpdateSeedDictionary that
        // contains the encrypted mask seed from our update
        // participant.
        let mut global_seed_dict = SeedDict::new();
        let mut entry = UpdateSeedDict::new();
        let encrypted_mask_seed = utils::local_seed_dict(&update_msg)
            .values()
            .next()
            .unwrap()
            .clone();
        entry.insert(updater.pk, encrypted_mask_seed);
        global_seed_dict.insert(summer.pk, entry);
        assert_eq!(
            events.seed_dict_listener().get_latest(),
            Event {
                round_id: 0,
                event: DictionaryUpdate::New(Arc::new(global_seed_dict)),
            }
        );
    }
}
