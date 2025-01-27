// This file is part of Substrate.

// Copyright (C) 2020-2021 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License")
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! <!-- markdown-link-check-disable -->
//!
//! ## Overview
//!
//! Circuit MVP
#![feature(box_syntax)]
#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

pub use crate::pallet::*;
use crate::{optimistic::Optimistic, state::*};
use codec::{Decode, Encode};
use frame_support::{
    dispatch::{Dispatchable, GetDispatchInfo},
    traits::{Currency, ExistenceRequirement::AllowDeath, Get},
    weights::Weight,
    RuntimeDebug,
};
use frame_system::{
    ensure_signed,
    offchain::{SignedPayload, SigningTypes},
    pallet_prelude::OriginFor,
};
use pallet_xbi_portal::{
    primitives::xbi::XBIPortal,
    xbi_format::{XBICheckIn, XBICheckOut, XBIInstr},
};
use pallet_xbi_portal_enter::t3rn_sfx::xbi_result_2_sfx_confirmation;
use sp_runtime::{traits::Zero, KeyTypeId};
use sp_std::{boxed::Box, convert::TryInto, vec, vec::Vec};
use t3rn_primitives::account_manager::Outcome;

pub use t3rn_primitives::{
    abi::{GatewayABIConfig, HasherAlgo as HA, Type},
    account_manager::AccountManager,
    circuit_portal::CircuitPortal,
    claimable::{BenefitSource, CircuitRole},
    executors::Executors,
    portal::Portal,
    side_effect::{
        ConfirmedSideEffect, FullSideEffect, HardenedSideEffect, SecurityLvl, SideEffect,
        SideEffectId,
    },
    transfers::EscrowedBalanceOf,
    volatile::LocalState,
    xdns::Xdns,
    xtx::{Xtx, XtxId},
    GatewayType, *,
};

use t3rn_protocol::side_effects::{
    confirm::protocol::*,
    loader::{SideEffectsLazyLoader, UniversalSideEffectsProtocol},
};

pub use t3rn_protocol::{circuit_inbound::StepConfirmation, merklize::*};
pub use t3rn_sdk_primitives::signal::{ExecutionSignal, SignalKind};

#[cfg(test)]
pub mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod escrow;
pub mod optimistic;
pub mod state;
pub mod weights;

/// Defines application identifier for crypto keys of this module.
/// Every module that deals with signatures needs to declare its unique identifier for
/// its crypto keys.
/// When offchain worker is signing transactions it's going to request keys of type
/// `KeyTypeId` from the keystore and use the ones it finds to sign the transaction.
/// The keys can be inserted manually via RPC (see `author_insertKey`).
pub const KEY_TYPE: KeyTypeId = KeyTypeId(*b"circ");

pub type SystemHashing<T> = <T as frame_system::Config>::Hashing;
pub type EscrowCurrencyOf<T> = <<T as pallet::Config>::Escrowed as EscrowTrait<T>>::Currency;

type BalanceOf<T> = EscrowBalance<T>;

#[frame_support::pallet]
pub mod pallet {
    use super::*;

    use frame_support::{
        pallet_prelude::*,
        traits::{
            fungible::{Inspect, Mutate},
            Get,
        },
    };
    use frame_system::pallet_prelude::*;
    use pallet_xbi_portal::xbi_codec::{XBICheckOutStatus, XBIMetadata, XBINotificationKind};
    use pallet_xbi_portal_enter::t3rn_sfx::sfx_2_xbi;
    use sp_runtime::traits::Hash;

    use pallet_xbi_portal::{
        primitives::xbi::{XBIPromise, XBIStatus},
        sabi::Sabi,
    };
    use sp_std::borrow::ToOwned;

    use t3rn_primitives::{
        circuit::{LocalStateExecutionView, LocalTrigger, OnLocalTrigger},
        portal::Portal,
        xdns::Xdns,
    };

    pub use crate::weights::WeightInfo;

    pub type EscrowBalance<T> = EscrowedBalanceOf<T, <T as Config>::Escrowed>;

    /// Current Circuit's context of active insurance deposits
    ///
    #[pallet::storage]
    #[pallet::getter(fn get_insurance_deposits)]
    pub type InsuranceDeposits<T> = StorageDoubleMap<
        _,
        Identity,
        XExecSignalId<T>,
        Identity,
        SideEffectId<T>,
        InsuranceDeposit<
            <T as frame_system::Config>::AccountId,
            <T as frame_system::Config>::BlockNumber,
            EscrowedBalanceOf<T, <T as Config>::Escrowed>,
        >,
        OptionQuery,
    >;

    /// Current Circuit's context of active insurance deposits
    ///
    #[pallet::storage]
    #[pallet::getter(fn local_side_effect_to_xtx_id_links)]
    pub type LocalSideEffectToXtxIdLinks<T> =
        StorageMap<_, Identity, SideEffectId<T>, XExecSignalId<T>, OptionQuery>;

    /// Current Circuit's context of active insurance deposits
    ///
    #[pallet::storage]
    #[pallet::getter(fn get_active_timing_links)]
    pub type ActiveXExecSignalsTimingLinks<T> = StorageMap<
        _,
        Identity,
        XExecSignalId<T>,
        <T as frame_system::Config>::BlockNumber,
        OptionQuery,
    >;
    /// Current Circuit's context of active transactions
    ///
    #[pallet::storage]
    #[pallet::getter(fn get_x_exec_signals)]
    pub type XExecSignals<T> = StorageMap<
        _,
        Identity,
        XExecSignalId<T>,
        XExecSignal<
            <T as frame_system::Config>::AccountId,
            <T as frame_system::Config>::BlockNumber,
            EscrowedBalanceOf<T, <T as Config>::Escrowed>,
        >,
        OptionQuery,
    >;

    /// Current Circuit's context of active full side effects (requested + confirmation proofs)
    #[pallet::storage]
    #[pallet::getter(fn get_xtx_insurance_links)]
    pub type XtxInsuranceLinks<T> =
        StorageMap<_, Identity, XExecSignalId<T>, Vec<SideEffectId<T>>, ValueQuery>;

    /// Current Circuit's context of active full side effects (requested + confirmation proofs)
    #[pallet::storage]
    #[pallet::getter(fn get_local_xtx_state)]
    pub type LocalXtxStates<T> = StorageMap<_, Identity, XExecSignalId<T>, LocalState, OptionQuery>;

    /// Current Circuit's context of active full side effects (requested + confirmation proofs)
    #[pallet::storage]
    #[pallet::getter(fn get_full_side_effects)]
    pub type FullSideEffects<T> = StorageMap<
        _,
        Identity,
        XExecSignalId<T>,
        Vec<
            Vec<
                FullSideEffect<
                    <T as frame_system::Config>::AccountId,
                    <T as frame_system::Config>::BlockNumber,
                    EscrowedBalanceOf<T, <T as Config>::Escrowed>,
                >,
            >,
        >,
        OptionQuery,
    >;

    /// Handles queued signals
    ///
    /// This operation is performed lazily in `on_initialize`.
    #[pallet::storage]
    #[pallet::getter(fn get_signal_queue)]
    pub(crate) type SignalQueue<T: Config> = StorageValue<
        _,
        BoundedVec<(T::AccountId, ExecutionSignal<T::Hash>), T::SignalQueueDepth>,
        ValueQuery,
    >;

    /// This pallet's configuration trait
    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The Circuit's account id
        #[pallet::constant]
        type SelfAccountId: Get<Self::AccountId>;

        /// The Circuit's self gateway id
        #[pallet::constant]
        type SelfGatewayId: Get<[u8; 4]>;

        /// The Circuit's self parachain id
        #[pallet::constant]
        type SelfParaId: Get<u32>;

        /// The Circuit's Default Xtx timeout
        #[pallet::constant]
        type XtxTimeoutDefault: Get<Self::BlockNumber>;

        /// The Circuit's Xtx timeout check interval
        #[pallet::constant]
        type XtxTimeoutCheckInterval: Get<Self::BlockNumber>;

        /// The Circuit's deletion queue limit - preventing potential
        ///     delay when queue is too long in on_initialize
        #[pallet::constant]
        type DeletionQueueLimit: Get<u32>;

        /// The overarching event type.
        type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

        /// A dispatchable call.
        type Call: Parameter
            + Dispatchable<Origin = Self::Origin>
            + GetDispatchInfo
            + From<Call<Self>>
            + From<frame_system::Call<Self>>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: weights::WeightInfo;

        /// A type that provides inspection and mutation to some fungible assets
        type Balances: Inspect<Self::AccountId> + Mutate<Self::AccountId>;

        /// A type that provides access to Xdns
        type Xdns: Xdns<Self>;

        type XBIPortal: XBIPortal<Self>;

        type XBIPromise: XBIPromise<Self, <Self as Config>::Call>;

        type Executors: Executors<
            Self,
            <<Self::Escrowed as EscrowTrait<Self>>::Currency as frame_support::traits::Currency<
                Self::AccountId,
            >>::Balance,
        >;

        /// A type that provides access to AccountManager
        type AccountManager: AccountManager<
            Self::AccountId,
            <<Self::Escrowed as EscrowTrait<Self>>::Currency as frame_support::traits::Currency<
                Self::AccountId,
            >>::Balance,
            Self::Hash,
            Self::BlockNumber,
        >;

        // type FreeVM: FreeVM<Self>;

        /// A type that manages escrow, and therefore balances
        type Escrowed: EscrowTrait<Self>;

        /// A type that gives access to the new portal functionality
        type Portal: Portal<Self>;

        /// The maximum number of signals that can be queued for handling.
        ///
        /// When a signal from 3vm is requested, we add it to the queue to be handled by on_initialize
        ///
        /// This allows us to process the highest priority and mitigate any race conditions from additional steps.
        ///
        /// The reasons for limiting the queue depth are:
        ///
        /// 1. The queue is in storage in order to be persistent between blocks. We want to limit
        /// 	the amount of storage that can be consumed.
        /// 2. The queue is stored in a vector and needs to be decoded as a whole when reading
        ///		it at the end of each block. Longer queues take more weight to decode and hence
        ///		limit the amount of items that can be deleted per block.
        #[pallet::constant]
        type SignalQueueDepth: Get<u32>;
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        // `on_initialize` is executed at the beginning of the block before any extrinsic are
        // dispatched.
        //
        // This function must return the weight consumed by `on_initialize` and `on_finalize`.
        fn on_initialize(n: T::BlockNumber) -> Weight {
            let weight = Self::process_signal_queue();

            // Check every XtxTimeoutCheckInterval blocks

            // what happens if the weight for the block is consumed, do these timeouts need to wait
            // for the next check interval to handle them? maybe we need an immediate queue
            //
            // Scenario 1: all the timeouts can be handled in the block space
            // Scenario 2: all but 5 timeouts can be handled
            //     - add the 5 timeouts to an immediate queue for the next block
            if n % T::XtxTimeoutCheckInterval::get() == T::BlockNumber::from(0u8) {
                let mut deletion_counter: u32 = 0;
                // Go over all unfinished Xtx to find those that timed out
                <ActiveXExecSignalsTimingLinks<T>>::iter()
                    .find(|(_xtx_id, timeout_at)| {
                        timeout_at <= &frame_system::Pallet::<T>::block_number()
                    })
                    .map(|(xtx_id, _timeout_at)| {
                        if deletion_counter > T::DeletionQueueLimit::get() {
                            return
                        }
                        let mut local_xtx_ctx = Self::setup(
                            CircuitStatus::RevertTimedOut,
                            &Self::account_id(),
                            Zero::zero(),
                            Some(xtx_id),
                        )
                        .unwrap();

                        Self::kill(&mut local_xtx_ctx, CircuitStatus::RevertTimedOut);

                        Self::emit(
                            local_xtx_ctx.xtx_id,
                            Some(local_xtx_ctx.xtx),
                            &Self::account_id(),
                            &vec![],
                            None,
                        );
                        deletion_counter += 1;
                    });
            }

            // Anything that needs to be done at the start of the block.
            // We don't do anything here.
            // ToDo: Do active xtx signals overview and Cancel if time elapsed
            weight
        }

        fn on_finalize(_n: T::BlockNumber) {
            // We don't do anything here.

            // if module block number
            // x-t3rn#4: Go over open Xtx and cancel if necessary
        }

        // A runtime code run after every block and have access to extended set of APIs.
        //
        // For instance you can generate extrinsics for the upcoming produced block.
        fn offchain_worker(_n: T::BlockNumber) {
            // We don't do anything here.
            // but we could dispatch extrinsic (transaction/unsigned/inherent) using
            // sp_io::submit_extrinsic
        }
    }

    impl<T: Config> OnLocalTrigger<T, BalanceOf<T>> for Pallet<T> {
        fn load_local_state(
            origin: &OriginFor<T>,
            maybe_xtx_id: Option<T::Hash>,
        ) -> Result<LocalStateExecutionView<T, BalanceOf<T>>, DispatchError> {
            let requester = Self::authorize(origin.to_owned(), CircuitRole::ContractAuthor)?;

            let fresh_or_revoked_exec = match maybe_xtx_id {
                Some(_xtx_id) => CircuitStatus::Ready,
                None => CircuitStatus::Requested,
            };

            let mut local_xtx_ctx: LocalXtxCtx<T> = Self::setup(
                fresh_or_revoked_exec,
                &requester,
                Zero::zero(),
                maybe_xtx_id,
            )?;
            log::debug!(
                target: "runtime::circuit",
                "load_local_state with status: {:?}",
                local_xtx_ctx.xtx.status
            );

            // There should be no apply step since no change could have happen during the state access
            let hardened_side_effects = local_xtx_ctx
                .full_side_effects
                .iter()
                .map(|step| {
                    step.iter()
                        .map(|fsx| {
                            let effect: HardenedSideEffect<
                                T::AccountId,
                                T::BlockNumber,
                                BalanceOf<T>,
                            > = fsx.clone().try_into().map_err(|e| {
                                log::debug!(
                                    target: "runtime::circuit",
                                    "Error converting side effect to runtime: {:?}",
                                    e
                                );
                                Error::<T>::FailedToHardenFullSideEffect
                            })?;
                            Ok(effect)
                        })
                        .collect::<Result<
                            Vec<HardenedSideEffect<T::AccountId, T::BlockNumber, BalanceOf<T>>>,
                            Error<T>,
                        >>()
                })
                .collect::<Result<
                    Vec<Vec<HardenedSideEffect<T::AccountId, T::BlockNumber, BalanceOf<T>>>>,
                    Error<T>,
                >>()?;

            // We must apply the state only if its generated and fresh
            if maybe_xtx_id.is_none() {
                // Update local context
                let status_change = Self::update(&mut local_xtx_ctx)?;

                let _ = Self::apply(&mut local_xtx_ctx, None, None, status_change);
            }

            // There should be no apply step since no change could have happen during the state access
            Ok(LocalStateExecutionView::<T, BalanceOf<T>>::new(
                local_xtx_ctx.xtx_id,
                local_xtx_ctx.local_state.clone(),
                hardened_side_effects,
                local_xtx_ctx.xtx.steps_cnt,
            ))
        }

        fn on_local_trigger(origin: &OriginFor<T>, trigger: LocalTrigger<T>) -> DispatchResult {
            log::debug!(
                target: "runtime::circuit",
                "Handling on_local_trigger xtx: {:?}, contract: {:?}, side_effects: {:?}",
                trigger.maybe_xtx_id,
                trigger.contract,
                trigger.submitted_side_effects
            );
            // Authorize: Retrieve sender of the transaction.
            let requester = Self::authorize(origin.to_owned(), CircuitRole::ContractAuthor)?;

            let fresh_or_revoked_exec = match trigger.maybe_xtx_id {
                Some(_xtx_id) => CircuitStatus::Ready,
                None => CircuitStatus::Requested,
            };
            // Setup: new xtx context
            let mut local_xtx_ctx: LocalXtxCtx<T> = Self::setup(
                fresh_or_revoked_exec.clone(),
                &requester,
                Zero::zero(),
                trigger.maybe_xtx_id,
            )?;

            log::debug!(
                target: "runtime::circuit",
                "submit_side_effects xtx state with status: {:?}",
                local_xtx_ctx.xtx.status
            );

            // ToDo: This should be converting the side effect from local trigger to FSE
            let side_effects = Self::exec_in_xtx_ctx(
                local_xtx_ctx.xtx_id,
                local_xtx_ctx.local_state.clone(),
                local_xtx_ctx.full_side_effects.clone(),
                local_xtx_ctx.xtx.steps_cnt,
            )
            .map_err(|_e| {
                if fresh_or_revoked_exec == CircuitStatus::Ready {
                    Self::kill(&mut local_xtx_ctx, CircuitStatus::RevertKill)
                }
                Error::<T>::ContractXtxKilledRunOutOfFunds
            })?;

            // ToDo: Align whether 3vm wants enfore side effects sequence into steps
            let sequential = false;
            // Validate: Side Effects
            Self::validate(&side_effects, &mut local_xtx_ctx, &requester, sequential)?;

            // Account fees and charges
            Self::square_up(&mut local_xtx_ctx, Some(requester.clone()), None)?;

            // Update local context
            let status_change = Self::update(&mut local_xtx_ctx)?;

            // Apply: all necessary changes to state in 1 go
            let (_, added_full_side_effects) =
                Self::apply(&mut local_xtx_ctx, None, None, status_change);

            // Emit: From Circuit events
            Self::emit(
                local_xtx_ctx.xtx_id,
                Some(local_xtx_ctx.xtx),
                &requester,
                &side_effects,
                added_full_side_effects,
            );

            Ok(())
        }

        fn on_signal(origin: &OriginFor<T>, signal: ExecutionSignal<T::Hash>) -> DispatchResult {
            log::debug!(target: "runtime::circuit", "Handling on_signal {:?}", signal);
            let requester = Self::authorize(origin.to_owned(), CircuitRole::ContractAuthor)?;

            <SignalQueue<T>>::mutate(|q| {
                q.try_push((requester, signal))
                    .map_err(|_| Error::<T>::SignalQueueFull)
            })?;
            Ok(())
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Used by other pallets that want to create the exec order
        #[pallet::weight(<T as pallet::Config>::WeightInfo::on_local_trigger())]
        pub fn on_local_trigger(origin: OriginFor<T>, trigger: Vec<u8>) -> DispatchResult {
            <Self as OnLocalTrigger<T, BalanceOf<T>>>::on_local_trigger(
                &origin,
                LocalTrigger::<T>::decode(&mut &trigger[..])
                    .map_err(|_| Error::<T>::InsuranceBondNotRequired)?,
            )
        }

        #[pallet::weight(<T as pallet::Config>::WeightInfo::on_local_trigger())]
        pub fn on_xcm_trigger(_origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            // ToDo: Check TriggerAuthRights for local triggers
            unimplemented!();
        }

        #[pallet::weight(<T as pallet::Config>::WeightInfo::on_local_trigger())]
        pub fn on_remote_gateway_trigger(_origin: OriginFor<T>) -> DispatchResultWithPostInfo {
            unimplemented!();
        }

        #[pallet::weight(<T as pallet::Config>::WeightInfo::on_extrinsic_trigger())]
        pub fn on_extrinsic_trigger(
            origin: OriginFor<T>,
            side_effects: Vec<
                SideEffect<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>,
            >,
            fee: EscrowedBalanceOf<T, T::Escrowed>,
            sequential: bool,
        ) -> DispatchResultWithPostInfo {
            // Authorize: Retrieve sender of the transaction.
            let requester = Self::authorize(origin, CircuitRole::Requester)?;
            // Setup: new xtx context
            let mut local_xtx_ctx: LocalXtxCtx<T> =
                Self::setup(CircuitStatus::Requested, &requester, fee, None)?;

            // Validate: Side Effects
            Self::validate(&side_effects, &mut local_xtx_ctx, &requester, sequential).map_err(
                |e| {
                    log::error!("Self::validate hit an error -- {:?}", e);
                    Error::<T>::SideEffectsValidationFailed
                },
            )?;

            // Account fees and charges
            Self::square_up(&mut local_xtx_ctx, Some(requester.clone()), None)?;

            // Update local context
            let status_change = Self::update(&mut local_xtx_ctx)?;

            // Apply: all necessary changes to state in 1 go
            let (_, added_full_side_effects) =
                Self::apply(&mut local_xtx_ctx, None, None, status_change);

            // Emit: From Circuit events
            Self::emit(
                local_xtx_ctx.xtx_id,
                Some(local_xtx_ctx.xtx),
                &requester,
                &side_effects,
                added_full_side_effects,
            );

            Ok(().into())
        }

        #[pallet::weight(<T as pallet::Config>::WeightInfo::bond_insurance_deposit())]
        pub fn bond_insurance_deposit(
            origin: OriginFor<T>, // Active relayer
            xtx_id: XExecSignalId<T>,
            side_effect_id: SideEffectId<T>,
        ) -> DispatchResultWithPostInfo {
            // Authorize: Retrieve sender of the transaction.
            let executor = Self::authorize(origin, CircuitRole::Executor)?;

            // Setup: retrieve local xtx context
            let mut local_xtx_ctx: LocalXtxCtx<T> = Self::setup(
                CircuitStatus::PendingInsurance,
                &executor,
                Zero::zero(),
                Some(xtx_id),
            )?;

            let insurance_deposit_copy =
                Optimistic::<T>::bond_4_sfx(&executor, &mut local_xtx_ctx, side_effect_id)?;

            let status_change = Self::update(&mut local_xtx_ctx)?;

            let (maybe_xtx_changed, _assert_full_side_effects_changed) = Self::apply(
                &mut local_xtx_ctx,
                Some((side_effect_id, insurance_deposit_copy)),
                None,
                status_change,
            );

            Self::deposit_event(Event::SideEffectInsuranceReceived(
                side_effect_id,
                executor.clone(),
            ));

            // Emit: From Circuit events
            Self::emit(
                local_xtx_ctx.xtx_id,
                maybe_xtx_changed,
                &executor,
                &vec![],
                None,
            );

            Ok(().into())
        }

        #[pallet::weight(<T as pallet::Config>::WeightInfo::execute_side_effects_with_xbi())]
        pub fn execute_side_effects_with_xbi(
            origin: OriginFor<T>, // Active relayer
            xtx_id: XExecSignalId<T>,
            side_effect: SideEffect<
                <T as frame_system::Config>::AccountId,
                <T as frame_system::Config>::BlockNumber,
                EscrowedBalanceOf<T, <T as Config>::Escrowed>,
            >,
            max_exec_cost: u128,
            max_notifications_cost: u128,
        ) -> DispatchResultWithPostInfo {
            let sfx_id = side_effect.generate_id::<SystemHashing<T>>();

            if T::XBIPortal::get_status(sfx_id) != XBIStatus::UnknownId {
                return Err(Error::<T>::SideEffectIsAlreadyScheduledToExecuteOverXBI.into())
            }
            // Authorize: Retrieve sender of the transaction.
            let executor = Self::authorize(origin, CircuitRole::Executor)?;

            // Setup: retrieve local xtx context
            let mut local_xtx_ctx: LocalXtxCtx<T> = Self::setup(
                CircuitStatus::PendingExecution,
                &executor,
                Zero::zero(),
                Some(xtx_id),
            )?;

            let xbi =
                sfx_2_xbi::<T, T::Escrowed>(
                    &side_effect,
                    XBIMetadata::new_with_default_timeouts(
                        XbiId::<T>::local_hash_2_xbi_id(sfx_id)?,
                        T::Xdns::get_gateway_para_id(&side_effect.target)?,
                        T::SelfParaId::get(),
                        max_exec_cost,
                        max_notifications_cost,
                        Some(Sabi::account_bytes_2_account_32(executor.encode()).map_err(
                            |_| Error::<T>::FailedToCreateXBIMetadataDueToWrongAccountConversion,
                        )?),
                    ),
                )
                .map_err(|_e| Error::<T>::FailedToConvertSFX2XBI)?;

            // Use encoded XBI hash as ID for the executor's charge
            let charge_id = T::Hashing::hash(&xbi.encode()[..]);
            let total_max_fees = xbi.metadata.total_max_costs_in_local_currency()?;

            Self::square_up(
                &mut local_xtx_ctx,
                None,
                Some((charge_id, executor, total_max_fees)),
            )?;

            T::XBIPromise::then(
                xbi,
                pallet::Call::<T>::on_xbi_sfx_resolved { sfx_id }.into(),
            )?;

            Ok(().into())
        }

        #[pallet::weight(< T as Config >::WeightInfo::confirm_side_effect())]
        pub fn on_xbi_sfx_resolved(
            _origin: OriginFor<T>,
            sfx_id: T::Hash,
        ) -> DispatchResultWithPostInfo {
            Self::do_xbi_exit(
                T::XBIPortal::get_check_in(sfx_id)?,
                T::XBIPortal::get_check_out(sfx_id)?,
            )?;
            Ok(().into())
        }

        /// Blind version should only be used for testing - unsafe since skips inclusion proof check.
        #[pallet::weight(< T as Config >::WeightInfo::confirm_side_effect())]
        pub fn confirm_side_effect(
            origin: OriginFor<T>,
            xtx_id: XtxId<T>,
            side_effect: SideEffect<
                <T as frame_system::Config>::AccountId,
                <T as frame_system::Config>::BlockNumber,
                EscrowedBalanceOf<T, T::Escrowed>,
            >,
            confirmation: ConfirmedSideEffect<
                <T as frame_system::Config>::AccountId,
                <T as frame_system::Config>::BlockNumber,
                EscrowedBalanceOf<T, T::Escrowed>,
            >,
            _inclusion_proof: Option<Vec<Vec<u8>>>,
            _block_hash: Option<Vec<u8>>,
        ) -> DispatchResultWithPostInfo {
            // Authorize: Retrieve sender of the transaction.
            let relayer = Self::authorize(origin, CircuitRole::Relayer)?;

            // Setup: retrieve local xtx context
            let mut local_xtx_ctx: LocalXtxCtx<T> = Self::setup(
                CircuitStatus::PendingExecution,
                &relayer,
                Zero::zero(),
                Some(xtx_id),
            )?;

            Self::confirm(&mut local_xtx_ctx, &relayer, &side_effect, &confirmation)?;

            let status_change = Self::update(&mut local_xtx_ctx)?;

            // Apply: all necessary changes to state in 1 go
            let (maybe_xtx_changed, assert_full_side_effects_changed) =
                Self::apply(&mut local_xtx_ctx, None, None, status_change);

            Self::deposit_event(Event::SideEffectConfirmed(
                side_effect.generate_id::<SystemHashing<T>>(),
            ));

            // Emit: From Circuit events
            Self::emit(
                local_xtx_ctx.xtx_id,
                maybe_xtx_changed,
                &relayer,
                &vec![],
                assert_full_side_effects_changed,
            );

            Ok(().into())
        }
    }

    use pallet_xbi_portal::xbi_abi::{
        AccountId20, AccountId32, AssetId, Data, Gas, Value, ValueEvm, XbiId,
    };

    /// Events for the pallet.
    #[pallet::event]
    #[pallet::generate_deposit(pub (super) fn deposit_event)]
    pub enum Event<T: Config> {
        // XBI Exit events - consider moving to separate XBIPortalExit pallet.
        Transfer(T::AccountId, AccountId32, AccountId32, Value),
        TransferAssets(T::AccountId, AssetId, AccountId32, AccountId32, Value),
        TransferORML(T::AccountId, AssetId, AccountId32, AccountId32, Value),
        AddLiquidity(T::AccountId, AssetId, AssetId, Value, Value, Value),
        Swap(T::AccountId, AssetId, AssetId, Value, Value, Value),
        CallNative(T::AccountId, Data),
        CallEvm(
            T::AccountId,
            AccountId20,
            AccountId20,
            ValueEvm,
            Data,
            Gas,
            ValueEvm,
            Option<ValueEvm>,
            Option<ValueEvm>,
            Vec<(AccountId20, Vec<sp_core::H256>)>,
        ),
        CallWasm(T::AccountId, AccountId32, Value, Gas, Option<Value>, Data),
        CallCustom(
            T::AccountId,
            AccountId32,
            AccountId32,
            Value,
            Data,
            Gas,
            Data,
        ),
        Notification(T::AccountId, AccountId32, XBINotificationKind, Data, Data),
        Result(T::AccountId, AccountId32, XBICheckOutStatus, Data, Data),
        // Listeners - users + SDK + UI to know whether their request is accepted for exec and pending
        XTransactionReceivedForExec(XExecSignalId<T>),
        // Notifies that the bond for a specific side_effect has been bonded.
        SideEffectInsuranceReceived(XExecSignalId<T>, <T as frame_system::Config>::AccountId),
        // An executions SideEffect was confirmed.
        SideEffectConfirmed(XExecSignalId<T>),
        // Listeners - users + SDK + UI to know whether their request is accepted for exec and ready
        XTransactionReadyForExec(XExecSignalId<T>),
        // Listeners - users + SDK + UI to know whether their request is accepted for exec and finished
        XTransactionStepFinishedExec(XExecSignalId<T>),
        // Listeners - users + SDK + UI to know whether their request is accepted for exec and finished
        XTransactionXtxFinishedExecAllSteps(XExecSignalId<T>),
        // Listeners - users + SDK + UI to know whether their request is accepted for exec and finished
        XTransactionXtxRevertedAfterTimeOut(XExecSignalId<T>),
        // Listeners - executioners/relayers to know new challenges and perform offline risk/reward calc
        //  of whether side effect is worth picking up
        NewSideEffectsAvailable(
            <T as frame_system::Config>::AccountId,
            XExecSignalId<T>,
            Vec<
                SideEffect<
                    <T as frame_system::Config>::AccountId,
                    <T as frame_system::Config>::BlockNumber,
                    EscrowedBalanceOf<T, T::Escrowed>,
                >,
            >,
            Vec<SideEffectId<T>>,
        ),
        // Listeners - executioners/relayers to know that certain SideEffects are no longer valid
        // ToDo: Implement Xtx timeout!
        CancelledSideEffects(
            <T as frame_system::Config>::AccountId,
            XtxId<T>,
            Vec<
                SideEffect<
                    <T as frame_system::Config>::AccountId,
                    <T as frame_system::Config>::BlockNumber,
                    EscrowedBalanceOf<T, T::Escrowed>,
                >,
            >,
        ),
        // Listeners - executioners/relayers to know whether they won the confirmation challenge
        SideEffectsConfirmed(
            XtxId<T>,
            Vec<
                Vec<
                    FullSideEffect<
                        <T as frame_system::Config>::AccountId,
                        <T as frame_system::Config>::BlockNumber,
                        EscrowedBalanceOf<T, T::Escrowed>,
                    >,
                >,
            >,
        ),
        EscrowTransfer(
            // ToDo: Inspect if Xtx needs to be here and how to process from protocol
            T::AccountId,                                  // from
            T::AccountId,                                  // to
            EscrowedBalanceOf<T, <T as Config>::Escrowed>, // value
        ),
    }

    #[pallet::error]
    pub enum Error<T> {
        UpdateXtxTriggeredWithUnexpectedStatus,
        ApplyTriggeredWithUnexpectedStatus,
        RequesterNotEnoughBalance,
        ContractXtxKilledRunOutOfFunds,
        ChargingTransferFailed,
        FinalizeSquareUpFailed,
        CriticalStateSquareUpCalledToFinishWithoutFsxConfirmed,
        RewardTransferFailed,
        RefundTransferFailed,
        SideEffectsValidationFailed,
        InsuranceBondNotRequired,
        InsuranceBondTooLow,
        InsuranceBondAlreadyDeposited,
        SetupFailed,
        SetupFailedXtxNotFound,
        SetupFailedXtxStorageArtifactsNotFound,
        SetupFailedIncorrectXtxStatus,
        EnactSideEffectsCanOnlyBeCalledWithMin1StepFinished,
        FatalXtxTimeoutXtxIdNotMatched,
        RelayEscrowedFailedNothingToConfirm,
        FatalCommitSideEffectWithoutConfirmationAttempt,
        FatalErroredCommitSideEffectConfirmationAttempt,
        FatalErroredRevertSideEffectConfirmationAttempt,
        SetupFailedUnknownXtx,
        FailedToHardenFullSideEffect,
        SetupFailedDuplicatedXtx,
        SetupFailedEmptyXtx,
        ApplyFailed,
        DeterminedForbiddenXtxStatus,
        SideEffectIsAlreadyScheduledToExecuteOverXBI,
        LocalSideEffectExecutionNotApplicable,
        LocalExecutionUnauthorized,
        FailedToConvertSFX2XBI,
        FailedToCheckInOverXBI,
        FailedToCreateXBIMetadataDueToWrongAccountConversion,
        FailedToConvertXBIResult2SFXConfirmation,
        FailedToEnterXBIPortal,
        FailedToExitXBIPortal,
        XBIExitFailedOnSFXConfirmation,
        UnsupportedRole,
        InvalidLocalTrigger,
        SignalQueueFull,
    }
}

pub fn get_xtx_status() {}

/// Payload used by this example crate to hold price
/// data required to submit a transaction.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Payload<Public, BlockNumber> {
    block_number: BlockNumber,
    public: Public,
}

impl<T: SigningTypes> SignedPayload<T> for Payload<T::Public, T::BlockNumber> {
    fn public(&self) -> T::Public {
        self.public.clone()
    }
}

impl<T: Config> Pallet<T> {
    fn setup(
        current_status: CircuitStatus,
        requester: &T::AccountId,
        reward: EscrowedBalanceOf<T, T::Escrowed>,
        xtx_id: Option<XExecSignalId<T>>,
    ) -> Result<LocalXtxCtx<T>, Error<T>> {
        match current_status {
            CircuitStatus::Requested => {
                if let Some(id) = xtx_id {
                    if <Self as Store>::XExecSignals::contains_key(id) {
                        return Err(Error::<T>::SetupFailedDuplicatedXtx)
                    }
                }
                // ToDo: Introduce default delay
                let (timeouts_at, delay_steps_at): (T::BlockNumber, Option<Vec<T::BlockNumber>>) = (
                    T::XtxTimeoutDefault::get() + frame_system::Pallet::<T>::block_number(),
                    None,
                );

                let (x_exec_signal_id, x_exec_signal) = XExecSignal::<
                    T::AccountId,
                    T::BlockNumber,
                    EscrowedBalanceOf<T, T::Escrowed>,
                >::setup_fresh::<T>(
                    requester,
                    timeouts_at,
                    delay_steps_at,
                    Some(reward),
                );

                Ok(LocalXtxCtx {
                    local_state: LocalState::new(),
                    use_protocol: UniversalSideEffectsProtocol::new(),
                    xtx_id: x_exec_signal_id,
                    xtx: x_exec_signal,
                    insurance_deposits: vec![],
                    full_side_effects: vec![],
                })
            },
            CircuitStatus::Ready
            | CircuitStatus::PendingExecution
            | CircuitStatus::PendingInsurance
            | CircuitStatus::Finished
            | CircuitStatus::RevertTimedOut => {
                if let Some(id) = xtx_id {
                    let xtx = <Self as Store>::XExecSignals::get(id)
                        .ok_or(Error::<T>::SetupFailedUnknownXtx)?;
                    // Make sure in case of commit_relay to only check finished Xtx
                    if current_status == CircuitStatus::Finished
                        && xtx.status < CircuitStatus::Finished
                    {
                        log::debug!(
                            "Incorrect status current_status: {:?} xtx_status {:?}",
                            current_status,
                            xtx.status
                        );
                        return Err(Error::<T>::SetupFailedIncorrectXtxStatus)
                    }
                    let insurance_deposits = <Self as Store>::XtxInsuranceLinks::get(id)
                        .iter()
                        .map(|&se_id| {
                            (
                                se_id,
                                <Self as Store>::InsuranceDeposits::get(id, se_id)
                                    .expect("Should not be state inconsistency"),
                            )
                        })
                        .collect::<Vec<(
                            SideEffectId<T>,
                            InsuranceDeposit<
                                T::AccountId,
                                T::BlockNumber,
                                EscrowedBalanceOf<T, T::Escrowed>,
                            >,
                        )>>();

                    let full_side_effects = <Self as Store>::FullSideEffects::get(id)
                        .ok_or(Error::<T>::SetupFailedXtxStorageArtifactsNotFound)?;
                    let local_state = <Self as Store>::LocalXtxStates::get(id)
                        .ok_or(Error::<T>::SetupFailedXtxStorageArtifactsNotFound)?;

                    Ok(LocalXtxCtx {
                        local_state,
                        use_protocol: UniversalSideEffectsProtocol::new(),
                        xtx_id: id,
                        xtx,
                        insurance_deposits,
                        // We need to retrieve full side effects to validate the confirmation order
                        full_side_effects,
                    })
                } else {
                    Err(Error::<T>::SetupFailedEmptyXtx)
                }
            },
            _ => unimplemented!(),
        }
    }

    // Updates local xtx context without touching the storage.
    fn update(
        mut local_ctx: &mut LocalXtxCtx<T>,
    ) -> Result<(CircuitStatus, CircuitStatus), Error<T>> {
        let current_status = local_ctx.xtx.status.clone();

        // Apply will try to move the status of Xtx from the current to the closest valid one.
        match current_status {
            CircuitStatus::Requested => {
                local_ctx.xtx.steps_cnt = (0, local_ctx.full_side_effects.len() as u32);
            },
            CircuitStatus::PendingInsurance | CircuitStatus::Bonded => {
                local_ctx.xtx.status = CircuitStatus::determine_effects_insurance_status::<T>(
                    &local_ctx.insurance_deposits,
                );
            },
            CircuitStatus::RevertTimedOut => {},
            CircuitStatus::Ready | CircuitStatus::PendingExecution | CircuitStatus::Finished => {
                // Check whether all of the side effects in this steps are confirmed - the status now changes to CircuitStatus::Finished
                if !Self::get_current_step_fsx(local_ctx)
                    .iter()
                    .any(|fsx| fsx.confirmed.is_none())
                {
                    local_ctx.xtx.steps_cnt =
                        (local_ctx.xtx.steps_cnt.0 + 1, local_ctx.xtx.steps_cnt.1);

                    local_ctx.xtx.status = CircuitStatus::Finished;

                    // All of the steps are completed - the xtx has been finalized
                    if local_ctx.xtx.steps_cnt.0 == local_ctx.xtx.steps_cnt.1 {
                        local_ctx.xtx.status = CircuitStatus::FinishedAllSteps;
                        return Ok((current_status, CircuitStatus::FinishedAllSteps))
                    }
                }
            },
            _ => {},
        }

        let new_status = CircuitStatus::determine_xtx_status(
            &local_ctx.full_side_effects,
            &local_ctx.insurance_deposits,
        )?;
        local_ctx.xtx.status = new_status.clone();

        Ok((current_status, new_status))
    }

    /// Returns: Returns changes written to the state if there are any.
    ///     For now only returns Xtx and FullSideEffects that changed.
    /// FixMe: Make Apply Infallible
    fn apply(
        mut local_ctx: &mut LocalXtxCtx<T>,
        maybe_insurance_tuple: Option<(
            SideEffectId<T>,
            InsuranceDeposit<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>,
        )>,
        _maybe_escrowed_confirmation: Option<(
            Vec<FullSideEffect<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>>,
            &SideEffect<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>,
            &T::AccountId,
            CircuitStatus,
        )>,
        status_change: (CircuitStatus, CircuitStatus),
    ) -> (
        Option<XExecSignal<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>>,
        Option<
            Vec<
                Vec<
                    FullSideEffect<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>,
                >,
            >,
        >,
    ) {
        // let current_status = local_ctx.xtx.status.clone();
        let (old_status, new_status) = (status_change.0, status_change.1);

        match old_status {
            CircuitStatus::Requested => {
                // Iterate over full side effects to detect ones to execute locally.
                fn is_local<T: Config>(gateway_id: &[u8; 4]) -> bool {
                    if *gateway_id == T::SelfGatewayId::get() {
                        return true
                    }
                    let gateway_type = <T as Config>::Xdns::get_gateway_type_unsafe(gateway_id);
                    gateway_type == GatewayType::ProgrammableInternal(0)
                }

                let steps_side_effects_ids: Vec<(
                    usize,
                    SideEffectId<T>,
                    XExecStepSideEffectId<T>,
                )> = local_ctx
                    .full_side_effects
                    .clone()
                    .iter()
                    .enumerate()
                    .flat_map(|(cnt, fsx_step)| {
                        fsx_step
                            .iter()
                            .map(|full_side_effect| full_side_effect.input.clone())
                            .filter(|side_effect| is_local::<T>(&side_effect.target))
                            .map(|side_effect| side_effect.generate_id::<SystemHashing<T>>())
                            .map(|side_effect_hash| {
                                (
                                    cnt,
                                    side_effect_hash,
                                    XExecSignal::<
                                        T::AccountId,
                                        T::BlockNumber,
                                        EscrowedBalanceOf<T, <T as Config>::Escrowed>,
                                    >::generate_step_id::<T>(
                                        side_effect_hash, cnt
                                    ),
                                )
                            })
                            .collect::<Vec<(usize, SideEffectId<T>, XExecStepSideEffectId<T>)>>()
                    })
                    .collect();

                <FullSideEffects<T>>::insert::<
                    XExecSignalId<T>,
                    Vec<
                        Vec<
                            FullSideEffect<
                                T::AccountId,
                                T::BlockNumber,
                                EscrowedBalanceOf<T, T::Escrowed>,
                            >,
                        >,
                    >,
                >(local_ctx.xtx_id, local_ctx.full_side_effects.clone());

                for (_step_cnt, side_effect_id, _step_side_effect_id) in steps_side_effects_ids {
                    <LocalSideEffectToXtxIdLinks<T>>::insert::<SideEffectId<T>, XExecSignalId<T>>(
                        side_effect_id,
                        local_ctx.xtx_id,
                    );
                }

                let mut ids_with_insurance: Vec<SideEffectId<T>> = vec![];
                for (side_effect_id, insurance_deposit) in &local_ctx.insurance_deposits {
                    <InsuranceDeposits<T>>::insert::<
                        XExecSignalId<T>,
                        SideEffectId<T>,
                        InsuranceDeposit<
                            T::AccountId,
                            T::BlockNumber,
                            EscrowedBalanceOf<T, T::Escrowed>,
                        >,
                    >(
                        local_ctx.xtx_id, *side_effect_id, insurance_deposit.clone()
                    );
                    ids_with_insurance.push(*side_effect_id);
                }
                <XtxInsuranceLinks<T>>::insert::<XExecSignalId<T>, Vec<SideEffectId<T>>>(
                    local_ctx.xtx_id,
                    ids_with_insurance,
                );
                <LocalXtxStates<T>>::insert::<XExecSignalId<T>, LocalState>(
                    local_ctx.xtx_id,
                    local_ctx.local_state.clone(),
                );
                <ActiveXExecSignalsTimingLinks<T>>::insert::<XExecSignalId<T>, T::BlockNumber>(
                    local_ctx.xtx_id,
                    local_ctx.xtx.timeouts_at,
                );

                <XExecSignals<T>>::insert::<
                    XExecSignalId<T>,
                    XExecSignal<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>,
                >(local_ctx.xtx_id, local_ctx.xtx.clone());

                (
                    Some(local_ctx.xtx.clone()),
                    Some(local_ctx.full_side_effects.to_vec()),
                )
            },
            CircuitStatus::PendingInsurance => {
                if let Some((side_effect_id, insurance_deposit)) = maybe_insurance_tuple {
                    <Self as Store>::InsuranceDeposits::mutate(
                        local_ctx.xtx_id,
                        side_effect_id,
                        |x| *x = Some(insurance_deposit),
                    );
                }
                if old_status != new_status {
                    local_ctx.xtx.status = new_status;
                    <Self as Store>::XExecSignals::mutate(local_ctx.xtx_id, |x| {
                        *x = Some(local_ctx.xtx.clone())
                    });
                    (Some(local_ctx.xtx.clone()), None)
                } else {
                    (None, None)
                }
            },
            CircuitStatus::RevertTimedOut => {
                <Self as Store>::XExecSignals::mutate(local_ctx.xtx_id, |x| {
                    *x = Some(local_ctx.xtx.clone())
                });

                <Self as Store>::ActiveXExecSignalsTimingLinks::remove(local_ctx.xtx_id);
                (
                    Some(local_ctx.xtx.clone()),
                    Some(local_ctx.full_side_effects.clone()),
                )
            },
            CircuitStatus::Ready
            | CircuitStatus::Bonded
            | CircuitStatus::PendingExecution
            | CircuitStatus::Finished => {
                // Update set of full side effects assuming the new confirmed has appeared
                <Self as Store>::FullSideEffects::mutate(local_ctx.xtx_id, |x| {
                    *x = Some(local_ctx.full_side_effects.clone())
                });

                <Self as Store>::XExecSignals::mutate(local_ctx.xtx_id, |x| {
                    *x = Some(local_ctx.xtx.clone())
                });
                if local_ctx.xtx.status.clone() > CircuitStatus::Ready {
                    (
                        Some(local_ctx.xtx.clone()),
                        Some(local_ctx.full_side_effects.clone()),
                    )
                } else {
                    (None, Some(local_ctx.full_side_effects.to_vec()))
                }
            },
            CircuitStatus::FinishedAllSteps => {
                // todo: cleanup all of the local storage
                <Self as Store>::XExecSignals::mutate(local_ctx.xtx_id, |x| {
                    *x = Some(local_ctx.xtx.clone())
                });

                <Self as Store>::ActiveXExecSignalsTimingLinks::remove(local_ctx.xtx_id);
                (
                    Some(local_ctx.xtx.clone()),
                    Some(local_ctx.full_side_effects.clone()),
                )
            },
            _ => (None, None),
        }
    }

    fn emit(
        xtx_id: XExecSignalId<T>,
        maybe_xtx: Option<
            XExecSignal<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>,
        >,
        subjected_account: &T::AccountId,
        side_effects: &Vec<
            SideEffect<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>,
        >,
        maybe_full_side_effects: Option<
            Vec<
                Vec<
                    FullSideEffect<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>,
                >,
            >,
        >,
    ) {
        if !side_effects.is_empty() {
            Self::deposit_event(Event::NewSideEffectsAvailable(
                subjected_account.clone(),
                xtx_id,
                // ToDo: Emit circuit outbound messages -> side effects
                side_effects.to_vec(),
                side_effects
                    .iter()
                    .map(|se| se.generate_id::<SystemHashing<T>>())
                    .collect::<Vec<SideEffectId<T>>>(),
            ));
        }
        if let Some(xtx) = maybe_xtx {
            match xtx.status {
                CircuitStatus::PendingInsurance =>
                    Self::deposit_event(Event::XTransactionReceivedForExec(xtx_id)),
                CircuitStatus::Ready =>
                    Self::deposit_event(Event::XTransactionReadyForExec(xtx_id)),
                CircuitStatus::Finished =>
                    Self::deposit_event(Event::XTransactionStepFinishedExec(xtx_id)),
                CircuitStatus::FinishedAllSteps =>
                    Self::deposit_event(Event::XTransactionXtxFinishedExecAllSteps(xtx_id)),
                CircuitStatus::RevertTimedOut =>
                    Self::deposit_event(Event::XTransactionXtxRevertedAfterTimeOut(xtx_id)),
                _ => {},
            }
            if xtx.status >= CircuitStatus::PendingExecution {
                if let Some(full_side_effects) = maybe_full_side_effects {
                    Self::deposit_event(Event::SideEffectsConfirmed(xtx_id, full_side_effects));
                }
            }
        }
    }

    fn kill(local_ctx: &mut LocalXtxCtx<T>, cause: CircuitStatus) {
        local_ctx.xtx.status = cause.clone();
        Optimistic::<T>::try_slash(local_ctx);

        Self::square_up(local_ctx, None, None)
            .expect("Expect Revert and RevertKill options to square up to be infallible");

        Self::apply(local_ctx, None, None, (cause.clone(), cause));
    }

    fn square_up(
        local_ctx: &mut LocalXtxCtx<T>,
        maybe_requester_charge: Option<<T as frame_system::Config>::AccountId>,
        maybe_xbi_execution_charge: Option<(
            T::Hash,
            <T as frame_system::Config>::AccountId,
            EscrowedBalanceOf<T, T::Escrowed>,
        )>,
    ) -> Result<(), Error<T>> {
        match local_ctx.xtx.status {
            CircuitStatus::Requested => {
                let requester = maybe_requester_charge.ok_or(Error::<T>::ChargingTransferFailed)?;
                for fsx in Self::get_current_step_fsx(local_ctx).iter() {
                    let charge_id = fsx.input.generate_id::<SystemHashing<T>>();
                    let offered_reward = fsx.input.prize;
                    if offered_reward > Zero::zero() {
                        <T as Config>::AccountManager::deposit(
                            charge_id,
                            &requester,
                            Zero::zero(),
                            offered_reward,
                            BenefitSource::TrafficRewards,
                            CircuitRole::Requester,
                            None,
                        )
                        .map_err(|_e| Error::<T>::ChargingTransferFailed)?;
                    }
                }
            },
            CircuitStatus::PendingExecution | CircuitStatus::Ready => {
                let (charge_id, executor_payee, charge_fee) =
                    maybe_xbi_execution_charge.ok_or(Error::<T>::ChargingTransferFailed)?;
                <T as Config>::AccountManager::deposit(
                    charge_id,
                    &executor_payee,
                    charge_fee,
                    Zero::zero(),
                    BenefitSource::TrafficFees,
                    CircuitRole::Executor,
                    None,
                )
                .map_err(|_e| Error::<T>::ChargingTransferFailed)?;
            },
            // todo: make sure callable once
            // todo: distinct between RevertTimedOut to iterate over all steps vs single step for Revert
            CircuitStatus::RevertTimedOut
            | CircuitStatus::Reverted
            | CircuitStatus::RevertMisbehaviour => {
                Optimistic::<T>::try_slash(local_ctx);
                for fsx in Self::get_current_step_fsx(local_ctx).iter() {
                    let charge_id = fsx.input.generate_id::<SystemHashing<T>>();
                    <T as Config>::AccountManager::try_finalize(
                        charge_id,
                        Outcome::Revert,
                        None,
                        None,
                    );
                }
            },
            CircuitStatus::Finished | CircuitStatus::FinishedAllSteps => {
                Optimistic::<T>::try_unbond(local_ctx)?;
                for fsx in Self::get_current_step_fsx(local_ctx).iter() {
                    let charge_id = fsx.input.generate_id::<SystemHashing<T>>();
                    let confirmed = if let Some(confirmed) = &fsx.confirmed {
                        Ok(confirmed)
                    } else {
                        Err(Error::<T>::CriticalStateSquareUpCalledToFinishWithoutFsxConfirmed)
                    }?;
                    // todo: Verify that cost can be repatriated on this occation and whether XBI Exec resoliution is expected for particular FSX
                    <T as Config>::AccountManager::finalize(
                        charge_id,
                        Outcome::Commit,
                        Some(confirmed.executioner.clone()),
                        confirmed.cost,
                    )
                    .map_err(|_e| Error::<T>::FinalizeSquareUpFailed)?;
                }
            },
            _ => {},
        }

        Ok(())
    }

    fn authorize(
        origin: OriginFor<T>,
        role: CircuitRole,
    ) -> Result<T::AccountId, sp_runtime::traits::BadOrigin> {
        match role {
            CircuitRole::Requester | CircuitRole::ContractAuthor => ensure_signed(origin),
            // ToDo: Handle active Relayer authorisation
            CircuitRole::Relayer => ensure_signed(origin),
            // ToDo: Handle bonded Executor authorisation
            CircuitRole::Executor => ensure_signed(origin),
            // ToDo: Handle other CircuitRoles
            _ => unimplemented!(),
        }
    }

    fn validate(
        side_effects: &[SideEffect<
            T::AccountId,
            T::BlockNumber,
            EscrowedBalanceOf<T, T::Escrowed>,
        >],
        local_ctx: &mut LocalXtxCtx<T>,
        requester: &T::AccountId,
        _sequential: bool,
    ) -> Result<(), &'static str> {
        // ToDo: Consuder burn_validation_fee
        // Fees::<T>::burn_validation_fee(requester, side_effects)?;

        let mut full_side_effects: Vec<
            FullSideEffect<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>,
        > = vec![];

        for side_effect in side_effects.iter() {
            let gateway_abi = <T as Config>::Xdns::get_abi(side_effect.target)?;
            let allowed_side_effects =
                <T as Config>::Xdns::allowed_side_effects(&side_effect.target);

            local_ctx
                .use_protocol
                .notice_gateway(side_effect.target, allowed_side_effects);

            local_ctx
            .use_protocol
            .validate_args::<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>, SystemHashing<T>>(
                side_effect.clone(),
                gateway_abi,
                &mut local_ctx.local_state,
            ).map_err(|e| {
                log::debug!(target: "runtime::circuit", "validate -- error validating side effects {:?}", e);
                e
            })?;

            if let Some(insurance_and_reward) =
                UniversalSideEffectsProtocol::check_if_insurance_required::<
                    T::AccountId,
                    T::BlockNumber,
                    EscrowedBalanceOf<T, T::Escrowed>,
                    SystemHashing<T>,
                >(side_effect.clone(), &mut local_ctx.local_state)?
            {
                let (insurance, reward) = (insurance_and_reward[0], insurance_and_reward[1]);
                // ToDo: Consider to remove the assignment below and move OptinalInsurance to SFX fields:
                //      sfx.reward = Option<Balance>
                //      sfx.insurance = Option<Balance>
                if side_effect.prize != reward {
                    return Err("Side_effect prize must be equal to reward of Optional Insurance")
                }

                local_ctx.insurance_deposits.push((
                    side_effect.generate_id::<SystemHashing<T>>(),
                    InsuranceDeposit::new(
                        insurance,
                        reward,
                        Zero::zero(), // To be modified by Optimistic::bond_4_sfx when set in the step context.
                        requester.clone(),
                        <frame_system::Pallet<T>>::block_number(),
                    ),
                ));
                let submission_target_height =
                    T::Portal::get_latest_finalized_height(side_effect.target)?
                        .ok_or("target height not found")?;

                full_side_effects.push(FullSideEffect {
                    input: side_effect.clone(),
                    confirmed: None,
                    security_lvl: SecurityLvl::Optimistic,
                    submission_target_height,
                })
            } else {
                fn determine_dirty_vs_escrowed_lvl<T: Config>(
                    side_effect: &SideEffect<
                        <T as frame_system::Config>::AccountId,
                        <T as frame_system::Config>::BlockNumber,
                        EscrowedBalanceOf<T, T::Escrowed>,
                    >,
                ) -> SecurityLvl {
                    let gateway_type =
                        <T as Config>::Xdns::get_gateway_type_unsafe(&side_effect.target);
                    let is_escrowed = gateway_type == GatewayType::ProgrammableInternal(0)
                        || gateway_type == GatewayType::OnCircuit(0);

                    if is_escrowed {
                        SecurityLvl::Escrowed
                    } else {
                        SecurityLvl::Dirty
                    }
                }
                let submission_target_height =
                    T::Portal::get_latest_finalized_height(side_effect.target)?
                        .ok_or("target height not found")?;

                full_side_effects.push(FullSideEffect {
                    input: side_effect.clone(),
                    confirmed: None,
                    security_lvl: determine_dirty_vs_escrowed_lvl::<T>(side_effect),
                    submission_target_height,
                });
            }
        }

        // Circuit's automatic side effect ordering: execute escrowed asap, then line up optimistic ones
        full_side_effects.sort_by(|a, b| b.security_lvl.partial_cmp(&a.security_lvl).unwrap());

        let mut full_side_effects_steps: Vec<
            Vec<FullSideEffect<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>>,
        > = vec![vec![]];

        for sorted_fse in full_side_effects {
            let current_step = full_side_effects_steps
                .last_mut()
                .expect("Vector initialized at declaration");

            // Push to the single step as long as there's no Dirty side effect
            //  Or if there was no Optimistic/Escrow side effects before

            if sorted_fse.security_lvl != SecurityLvl::Dirty || current_step.is_empty() {
                current_step.push(sorted_fse);
            } else if sorted_fse.security_lvl == SecurityLvl::Dirty {
                // R#1: there only can be max 1 dirty side effect at each step.
                full_side_effects_steps.push(vec![sorted_fse])
            }
        }

        local_ctx.full_side_effects = full_side_effects_steps.clone();

        Ok(())
    }

    fn confirm(
        local_ctx: &mut LocalXtxCtx<T>,
        _relayer: &T::AccountId,
        side_effect: &SideEffect<
            <T as frame_system::Config>::AccountId,
            <T as frame_system::Config>::BlockNumber,
            EscrowedBalanceOf<T, T::Escrowed>,
        >,
        confirmation: &ConfirmedSideEffect<
            <T as frame_system::Config>::AccountId,
            <T as frame_system::Config>::BlockNumber,
            EscrowedBalanceOf<T, <T as Config>::Escrowed>,
        >,
    ) -> Result<(), &'static str> {
        fn confirm_order<T: Config>(
            side_effect: &SideEffect<
                <T as frame_system::Config>::AccountId,
                <T as frame_system::Config>::BlockNumber,
                EscrowedBalanceOf<T, T::Escrowed>,
            >,
            confirmation: &ConfirmedSideEffect<
                <T as frame_system::Config>::AccountId,
                <T as frame_system::Config>::BlockNumber,
                EscrowedBalanceOf<T, T::Escrowed>,
            >,
            step_side_effects: &mut Vec<
                FullSideEffect<
                    <T as frame_system::Config>::AccountId,
                    <T as frame_system::Config>::BlockNumber,
                    EscrowedBalanceOf<T, T::Escrowed>,
                >,
            >,
        ) -> Result<
            FullSideEffect<
                <T as frame_system::Config>::AccountId,
                <T as frame_system::Config>::BlockNumber,
                EscrowedBalanceOf<T, T::Escrowed>,
            >,
            &'static str,
        > {
            // ToDo: Extract as a separate function and migrate tests from Xtx
            let input_side_effect_id = side_effect.generate_id::<SystemHashing<T>>();

            // Double check there are some side effects for that Xtx - should have been checked at API level tho already
            if step_side_effects.is_empty() {
                return Err("Xtx has an empty single step.")
            }

            // Find sfx object index in the current step
            match step_side_effects
                .iter()
                .position(|sfx| sfx.input.generate_id::<SystemHashing<T>>() == input_side_effect_id)
            {
                Some(index) => {
                    // side effect found in current step
                    if step_side_effects[index].confirmed.is_none() {
                        // side effect unconfirmed currently
                        step_side_effects[index].confirmed = Some(confirmation.clone());
                        Ok(step_side_effects[index].clone())
                    } else {
                        Err("Side Effect already confirmed")
                    }
                },
                None => Err("Unable to find matching Side Effect in given Xtx to confirm"),
            }
        }

        let mut side_effect_id: [u8; 4] = [0, 0, 0, 0];
        side_effect_id.copy_from_slice(&side_effect.encoded_action[0..4]);

        // confirm order of current season, by passing the side_effects of it to confirm order.
        let fsx = confirm_order::<T>(
            side_effect,
            confirmation,
            &mut local_ctx.full_side_effects[local_ctx.xtx.steps_cnt.0 as usize],
        )?;
        log::debug!("Order confirmed!");
        // confirm the payload is included in the specified block, and return the SideEffect params as defined in XDNS.
        // this could be multiple events!
        let (params, source) = <T as Config>::Portal::confirm_and_decode_payload_params(
            side_effect.target,
            fsx.submission_target_height,
            confirmation.inclusion_data.clone(),
            side_effect_id,
        )
        .map_err(|_| "SideEffect confirmation failed!")?;
        // ToDo: handle misbehaviour
        log::debug!("SFX confirmation params: {:?}", params);

        let mut side_effect_id: [u8; 4] = [0, 0, 0, 0];
        side_effect_id.copy_from_slice(&side_effect.encoded_action[0..4]);
        let side_effect_interface =
            <T as Config>::Xdns::fetch_side_effect_interface(side_effect_id);

        log::debug!("Found SFX interface!");

        confirmation_plug::<T>(
            &Box::new(side_effect_interface.unwrap()),
            params,
            source,
            &local_ctx.local_state,
            Some(
                side_effect
                    .generate_id::<SystemHashing<T>>()
                    .as_ref()
                    .to_vec(),
            ),
            fsx.security_lvl,
            <T as Config>::Xdns::get_gateway_security_coordinates(&side_effect.target)?,
        )
        .map_err(|_| "Execution can't be confirmed.")?;
        log::debug!("confirmation plug ok");

        Ok(())
    }

    // ToDo: This should be called as a 3vm trait injection @Don
    pub fn exec_in_xtx_ctx(
        _xtx_id: T::Hash,
        _local_state: LocalState,
        _full_side_effects: Vec<
            Vec<FullSideEffect<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>>,
        >,
        _steps_cnt: (u32, u32),
    ) -> Result<
        Vec<SideEffect<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>>,
        &'static str,
    > {
        Ok(vec![])
    }

    /// The account ID of the Circuit Vault.
    pub fn account_id() -> T::AccountId {
        <T as Config>::SelfAccountId::get()
    }

    pub fn convert_side_effects(
        side_effects: Vec<Vec<u8>>,
    ) -> Result<
        Vec<SideEffect<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>>,
        &'static str,
    > {
        let side_effects: Vec<
            SideEffect<T::AccountId, T::BlockNumber, EscrowedBalanceOf<T, T::Escrowed>>,
        > = side_effects
            .into_iter()
            .filter_map(|se| se.try_into().ok()) // TODO: maybe not
            .collect();
        if side_effects.is_empty() {
            Err("No side effects provided")
        } else {
            Ok(side_effects)
        }
    }

    // TODO: we also want to save some space for timeouts, split the weight distribution 50-50
    pub(crate) fn process_signal_queue() -> Weight {
        let queue_len = <SignalQueue<T>>::decode_len().unwrap_or(0);
        if queue_len == 0 {
            return 0
        }
        let db_weight = T::DbWeight::get();

        let mut queue = <SignalQueue<T>>::get();

        // We can do an easy process and only process CONSTANT / something signals for now
        let mut remaining_key_budget = T::SignalQueueDepth::get() / 4;
        let mut processed_weight = 0;

        while !queue.is_empty() && remaining_key_budget > 0 {
            // Cannot panic due to loop condition
            let (requester, signal) = &mut queue[0];

            let intended_status = match signal.kind {
                SignalKind::Complete => CircuitStatus::Finished, // Fails bc no executor tried to execute, maybe a new enum?
                SignalKind::Kill(_) => CircuitStatus::RevertKill,
            };

            // worst case 4 from setup
            processed_weight += db_weight.reads(4 as Weight);
            match Self::setup(
                CircuitStatus::Ready,
                requester,
                Zero::zero(),
                Some(signal.execution_id),
            ) {
                Ok(mut local_xtx_ctx) => {
                    Self::kill(&mut local_xtx_ctx, intended_status);

                    queue.swap_remove(0);

                    remaining_key_budget -= 1;
                    // apply has 2
                    processed_weight += db_weight.reads_writes(2 as Weight, 1 as Weight);
                },
                Err(_err) => {
                    log::error!("Could not handle signal");
                    // Slide the erroneous signal to the back
                    queue.slide(0, queue.len());
                },
            }
        }
        // Initial read of queue and update
        processed_weight += db_weight.reads_writes(1 as Weight, 1 as Weight);

        <SignalQueue<T>>::put(queue);

        processed_weight
    }

    pub(self) fn get_current_step_fsx(
        local_ctx: &mut LocalXtxCtx<T>,
    ) -> &Vec<
        FullSideEffect<
            <T as frame_system::Config>::AccountId,
            <T as frame_system::Config>::BlockNumber,
            EscrowedBalanceOf<T, <T as Config>::Escrowed>,
        >,
    > {
        let current_step = local_ctx.xtx.steps_cnt.0;
        &local_ctx.full_side_effects[current_step as usize]
    }

    pub(self) fn get_current_step_fsx_by_security_lvl(
        local_ctx: &mut LocalXtxCtx<T>,
        security_lvl: SecurityLvl,
    ) -> Vec<
        FullSideEffect<
            <T as frame_system::Config>::AccountId,
            <T as frame_system::Config>::BlockNumber,
            EscrowedBalanceOf<T, <T as Config>::Escrowed>,
        >,
    > {
        let current_step = local_ctx.xtx.steps_cnt.0;
        local_ctx.full_side_effects[current_step as usize]
            .iter()
            .filter(|&fsx| fsx.security_lvl == security_lvl)
            .cloned()
            .collect()
    }

    pub(self) fn get_fsx_total_rewards(
        fsxs: &[FullSideEffect<
            <T as frame_system::Config>::AccountId,
            <T as frame_system::Config>::BlockNumber,
            EscrowedBalanceOf<T, <T as Config>::Escrowed>,
        >],
    ) -> EscrowedBalanceOf<T, <T as Config>::Escrowed> {
        let mut acc_rewards: EscrowedBalanceOf<T, <T as Config>::Escrowed> = Zero::zero();

        for fsx in fsxs {
            acc_rewards += fsx.input.prize;
        }

        acc_rewards
    }

    pub(self) fn recover_fsx_by_id(
        sfx_id: SideEffectId<T>,
        local_ctx: &LocalXtxCtx<T>,
    ) -> Result<
        FullSideEffect<
            <T as frame_system::Config>::AccountId,
            <T as frame_system::Config>::BlockNumber,
            EscrowedBalanceOf<T, <T as Config>::Escrowed>,
        >,
        Error<T>,
    > {
        let current_step = local_ctx.xtx.steps_cnt.0;
        let maybe_fsx = local_ctx.full_side_effects[current_step as usize]
            .iter()
            .filter(|&fsx| fsx.confirmed.is_none())
            .find(|&fsx| fsx.input.generate_id::<SystemHashing<T>>() == sfx_id);

        if let Some(fsx) = maybe_fsx {
            Ok(fsx.clone())
        } else {
            Err(Error::<T>::LocalSideEffectExecutionNotApplicable)
        }
    }

    pub(self) fn recover_local_ctx_by_sfx_id(
        sfx_id: SideEffectId<T>,
    ) -> Result<LocalXtxCtx<T>, Error<T>> {
        let xtx_id = <Self as Store>::LocalSideEffectToXtxIdLinks::get(sfx_id)
            .ok_or(Error::<T>::LocalSideEffectExecutionNotApplicable)?;
        Self::setup(
            CircuitStatus::PendingExecution,
            &Self::account_id(),
            Zero::zero(),
            Some(xtx_id),
        )
    }

    pub fn do_xbi_exit(
        xbi_checkin: XBICheckIn<T::BlockNumber>,
        _xbi_checkout: XBICheckOut,
    ) -> Result<(), Error<T>> {
        // Recover SFX ID from XBI Metadata
        let sfx_id: SideEffectId<T> =
            Decode::decode(&mut &xbi_checkin.xbi.metadata.id.encode()[..])
                .expect("XBI metadata id conversion should always decode to Sfx ID");

        let mut local_xtx_ctx: LocalXtxCtx<T> = Self::recover_local_ctx_by_sfx_id(sfx_id)?;

        let fsx = Self::recover_fsx_by_id(sfx_id, &local_xtx_ctx)?;

        // todo#2: local fail Xtx if xbi_checkout::result errored

        let escrow_source = Self::account_id();
        let executor = if let Some(ref known_origin) = xbi_checkin.xbi.metadata.maybe_known_origin {
            known_origin.clone()
        } else {
            return Err(Error::<T>::FailedToExitXBIPortal)
        };
        let executor_decoded = Decode::decode(&mut &executor.encode()[..])
            .expect("XBI metadata executor conversion should always decode to local Account ID");

        let xbi_exit_event = match xbi_checkin.clone().xbi.instr {
            XBIInstr::CallNative { payload } => Ok(Event::<T>::CallNative(escrow_source, payload)),
            XBIInstr::CallEvm {
                source,
                target,
                value,
                input,
                gas_limit,
                max_fee_per_gas,
                max_priority_fee_per_gas,
                nonce,
                access_list,
            } => Ok(Event::<T>::CallEvm(
                escrow_source,
                source,
                target,
                value,
                input,
                gas_limit,
                max_fee_per_gas,
                max_priority_fee_per_gas,
                nonce,
                access_list,
            )),
            XBIInstr::CallWasm {
                dest,
                value,
                gas_limit,
                storage_deposit_limit,
                data,
            } => Ok(Event::<T>::CallWasm(
                escrow_source,
                dest,
                value,
                gas_limit,
                storage_deposit_limit,
                data,
            )),
            XBIInstr::CallCustom {
                caller,
                dest,
                value,
                input,
                limit,
                additional_params,
            } => Ok(Event::<T>::CallCustom(
                escrow_source,
                caller,
                dest,
                value,
                input,
                limit,
                additional_params,
            )),
            XBIInstr::Transfer { dest, value } =>
                Ok(Event::<T>::Transfer(escrow_source, executor, dest, value)),
            XBIInstr::TransferORML {
                currency_id,
                dest,
                value,
            } => Ok(Event::<T>::TransferORML(
                escrow_source,
                currency_id,
                executor,
                dest,
                value,
            )),
            XBIInstr::TransferAssets {
                currency_id,
                dest,
                value,
            } => Ok(Event::<T>::TransferAssets(
                escrow_source,
                currency_id,
                executor,
                dest,
                value,
            )),
            XBIInstr::Result {
                outcome,
                output,
                witness,
            } => Ok(Event::<T>::Result(
                escrow_source,
                executor,
                outcome,
                output,
                witness,
            )),
            XBIInstr::Notification {
                kind,
                instruction_id,
                extra,
            } => Ok(Event::<T>::Notification(
                escrow_source,
                executor,
                kind,
                instruction_id,
                extra,
            )),
            _ => Err(Error::<T>::FailedToExitXBIPortal),
        }?;

        Self::deposit_event(xbi_exit_event.clone());

        let confirmation = xbi_result_2_sfx_confirmation::<T, T::Escrowed>(
            xbi_checkin.xbi,
            xbi_exit_event.encode(),
            executor_decoded,
        )
        .map_err(|_| Error::<T>::FailedToConvertXBIResult2SFXConfirmation)?;

        Self::confirm(
            &mut local_xtx_ctx,
            &Self::account_id(),
            &fsx.input,
            &confirmation,
        )
        .map_err(|_e| Error::<T>::XBIExitFailedOnSFXConfirmation)?;
        Ok(())
    }
}
