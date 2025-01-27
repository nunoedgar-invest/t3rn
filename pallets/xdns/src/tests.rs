// This file is part of Substrate.

// Copyright (C) 2019-2021 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
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

//! Runtimes for pallet-xdns.

use super::*;
use circuit_mock_runtime::{ExtBuilder, *};
use codec::Decode;
use frame_support::{assert_err, assert_noop, assert_ok};
use frame_system::Origin;
use sp_runtime::DispatchError;
use t3rn_primitives::{abi::Type, xdns::Xdns, GatewayType, GatewayVendor};

const DEFAULT_GATEWAYS_IN_STORAGE_COUNT: usize = 7;
const STANDARD_SIDE_EFFECTS_COUNT: usize = 9;

#[test]
fn genesis_should_seed_circuit_gateway_polkadot_and_kusama_nodes() {
    ExtBuilder::default()
        .with_standard_side_effects()
        .with_default_xdns_records()
        .build()
        .execute_with(|| {
            assert_eq!(
                pallet_xdns::XDNSRegistry::<Runtime>::iter().count(),
                DEFAULT_GATEWAYS_IN_STORAGE_COUNT
            );
            assert!(pallet_xdns::XDNSRegistry::<Runtime>::get([3, 3, 3, 3]).is_some());
            assert!(pallet_xdns::XDNSRegistry::<Runtime>::get(b"gate").is_some());
            assert!(pallet_xdns::XDNSRegistry::<Runtime>::get(b"pdot").is_some());
            assert!(pallet_xdns::XDNSRegistry::<Runtime>::get(b"ksma").is_some());
        });
}

#[test]
fn should_add_a_new_xdns_record_if_it_doesnt_exist() {
    ExtBuilder::default().build().execute_with(|| {
        assert_ok!(XDNS::add_new_xdns_record(
            Origin::<Runtime>::Root.into(),
            b"some_url".to_vec(),
            *b"test",
            None,
            Default::default(),
            GatewayVendor::Rococo,
            GatewayType::TxOnly(0),
            Default::default(),
            Default::default(),
            vec![],
            vec![],
        ));
        assert_eq!(pallet_xdns::XDNSRegistry::<Runtime>::iter().count(), 1);
        assert!(pallet_xdns::XDNSRegistry::<Runtime>::get(b"test").is_some());
    });
}

#[test]
fn should_not_add_a_new_side_effect_if_it_exist() {
    ExtBuilder::default()
        .with_standard_side_effects()
        .with_default_xdns_records()
        .build()
        .execute_with(|| {
            assert_noop!(
                XDNS::add_side_effect(
                    Origin::<Runtime>::Root.into(),
                    *b"aliq",
                    b"add_liquidity".to_vec(),
                    vec![
                        Type::DynamicAddress,    // argument_0: caller
                        Type::DynamicAddress,    // argument_1: to
                        Type::DynamicBytes,      // argument_2: asset_left
                        Type::DynamicBytes,      // argument_3: asset_right
                        Type::DynamicBytes,      // argument_4: liquidity_token
                        Type::Value,             // argument_5: amount_left
                        Type::Value,             // argument_6: amount_right
                        Type::Value,             // argument_7: amount_liquidity_token
                        Type::OptionalInsurance, // argument_8: insurance
                    ],
                    vec![
                        b"caller".to_vec(),
                        b"to".to_vec(),
                        b"asset_left".to_vec(),
                        b"assert_right".to_vec(),
                        b"liquidity_token".to_vec(),
                        b"amount_left".to_vec(),
                        b"amount_right".to_vec(),
                        b"amount_liquidity_token".to_vec(),
                        b"insurance".to_vec(),
                    ],
                    vec![
                        b"ExecuteToken(executor,to,liquidity_token,amount_liquidity_token)"
                            .to_vec()
                    ],
                    vec![
                        b"ExecuteToken(xtx_id,to,liquidity_token,amount_liquidity_token)".to_vec()
                    ],
                    vec![
                        b"MultiTransfer(executor,to,liquidity_token,amount_liquidity_token)"
                            .to_vec()
                    ],
                    vec![
                        b"MultiTransfer(executor,caller,asset_left,amount_left)".to_vec(),
                        b"MultiTransfer(executor,caller,asset_right,amount_right)".to_vec()
                    ]
                ),
                pallet_xdns::pallet::Error::<Runtime>::SideEffectInterfaceAlreadyExists
            );
            assert_eq!(pallet_xdns::CustomSideEffects::<Runtime>::iter().count(), 0);
        });
}

#[test]
fn should_add_standard_side_effects() {
    ExtBuilder::default()
        .with_standard_side_effects()
        .with_default_xdns_records()
        .build()
        .execute_with(|| {
            assert_eq!(
                pallet_xdns::StandardSideEffects::<Runtime>::iter().count(),
                9
            );
        });
}

#[test]
fn should_add_a_new_side_effect_if_it_doesnt_exist() {
    ExtBuilder::default()
        .with_standard_side_effects()
        .with_default_xdns_records()
        .build()
        .execute_with(|| {
            assert_ok!(XDNS::add_side_effect(
                Origin::<Runtime>::Root.into(),
                *b"cust",
                b"custom_side_effect".to_vec(),
                vec![
                    Type::DynamicAddress,    // argument_0: caller
                    Type::DynamicAddress,    // argument_1: to
                    Type::DynamicBytes,      // argument_2: asset_left
                    Type::DynamicBytes,      // argument_3: asset_right
                    Type::DynamicBytes,      // argument_4: liquidity_token
                    Type::Value,             // argument_5: amount_left
                    Type::Value,             // argument_6: amount_right
                    Type::Value,             // argument_7: amount_liquidity_token
                    Type::OptionalInsurance, // argument_8: insurance
                ],
                vec![
                    b"caller".to_vec(),
                    b"to".to_vec(),
                    b"asset_left".to_vec(),
                    b"assert_right".to_vec(),
                    b"liquidity_token".to_vec(),
                    b"amount_left".to_vec(),
                    b"amount_right".to_vec(),
                    b"amount_liquidity_token".to_vec(),
                    b"insurance".to_vec(),
                ],
                vec![b"ExecuteToken(executor,to,liquidity_token,amount_liquidity_token)".to_vec()],
                vec![b"ExecuteToken(xtx_id,to,liquidity_token,amount_liquidity_token)".to_vec()],
                vec![b"MultiTransfer(executor,to,liquidity_token,amount_liquidity_token)".to_vec()],
                vec![
                    b"MultiTransfer(executor,caller,asset_left,amount_left)".to_vec(),
                    b"MultiTransfer(executor,caller,asset_right,amount_right)".to_vec()
                ]
            ));
            assert_eq!(pallet_xdns::CustomSideEffects::<Runtime>::iter().count(), 1);
            let side_effect = pallet_xdns::CustomSideEffects::<Runtime>::get(
                <Runtime as frame_system::Config>::Hashing::hash(b"cust"),
            )
            .unwrap();
            assert_eq!(side_effect.get_id(), *b"cust");
            assert_eq!(side_effect.get_name(), *b"custom_side_effect");
        });
}

#[test]
fn should_not_add_a_new_xdns_record_if_it_already_exists() {
    ExtBuilder::default()
        .with_standard_side_effects()
        .with_default_xdns_records()
        .build()
        .execute_with(|| {
            assert_noop!(
                XDNS::add_new_xdns_record(
                    Origin::<Runtime>::Root.into(),
                    b"some_url".to_vec(),
                    [3, 3, 3, 3],
                    None,
                    Default::default(),
                    GatewayVendor::Rococo,
                    GatewayType::TxOnly(0),
                    Default::default(),
                    Default::default(),
                    vec![],
                    vec![],
                ),
                pallet_xdns::pallet::Error::<Runtime>::XdnsRecordAlreadyExists
            );
            assert_eq!(
                pallet_xdns::XDNSRegistry::<Runtime>::iter().count(),
                DEFAULT_GATEWAYS_IN_STORAGE_COUNT
            );
        });
}

#[test]
fn should_purge_a_xdns_record_successfully() {
    ExtBuilder::default()
        .with_standard_side_effects()
        .with_default_xdns_records()
        .build()
        .execute_with(|| {
            assert_eq!(
                pallet_xdns::XDNSRegistry::<Runtime>::iter().count(),
                DEFAULT_GATEWAYS_IN_STORAGE_COUNT
            );
            assert_ok!(XDNS::purge_xdns_record(
                Origin::<Runtime>::Root.into(),
                ALICE,
                *b"gate"
            ));
            assert_eq!(
                pallet_xdns::XDNSRegistry::<Runtime>::iter().count(),
                DEFAULT_GATEWAYS_IN_STORAGE_COUNT - 1
            );
            assert!(pallet_xdns::XDNSRegistry::<Runtime>::get(b"gate").is_none());
        });
}

#[test]
fn finds_correct_amount_of_allowed_side_effects() {
    ExtBuilder::default()
        .with_standard_side_effects()
        .with_default_xdns_records()
        .build()
        .execute_with(|| {
            assert_eq!(
                XDNS::allowed_side_effects(&[3, 3, 3, 3]).len(),
                STANDARD_SIDE_EFFECTS_COUNT
            )
        });
}

#[test]
fn should_error_trying_to_purge_a_missing_xdns_record() {
    let _missing_hash = <Runtime as frame_system::Config>::Hashing::hash(b"miss");

    ExtBuilder::default()
        .with_standard_side_effects()
        .with_default_xdns_records()
        .build()
        .execute_with(|| {
            assert_noop!(
                XDNS::purge_xdns_record(Origin::<Runtime>::Root.into(), ALICE, *b"miss"),
                pallet_xdns::pallet::Error::<Runtime>::UnknownXdnsRecord
            );
            assert_eq!(
                pallet_xdns::XDNSRegistry::<Runtime>::iter().count(),
                DEFAULT_GATEWAYS_IN_STORAGE_COUNT
            );
        });
}

#[test]
fn should_error_trying_to_purge_an_xdns_record_if_not_root() {
    ExtBuilder::default()
        .with_standard_side_effects()
        .with_default_xdns_records()
        .build()
        .execute_with(|| {
            assert_noop!(
                XDNS::purge_xdns_record(Origin::<Runtime>::Signed(ALICE).into(), ALICE, *b"gate"),
                DispatchError::BadOrigin
            );
            assert_eq!(
                pallet_xdns::XDNSRegistry::<Runtime>::iter().count(),
                DEFAULT_GATEWAYS_IN_STORAGE_COUNT
            );
            assert!(pallet_xdns::XDNSRegistry::<Runtime>::get(b"gate").is_some());
        });
}

#[test]
fn should_update_ttl_for_a_known_xdns_record() {
    ExtBuilder::default()
        .with_standard_side_effects()
        .with_default_xdns_records()
        .build()
        .execute_with(|| {
            assert_ok!(XDNS::update_ttl(
                Origin::<Runtime>::Root.into(),
                *b"gate",
                2
            ));
            assert_eq!(
                pallet_xdns::XDNSRegistry::<Runtime>::iter().count(),
                DEFAULT_GATEWAYS_IN_STORAGE_COUNT
            );
            assert_eq!(
                pallet_xdns::XDNSRegistry::<Runtime>::get(b"gate")
                    .unwrap()
                    .last_finalized,
                Some(2)
            );
        });
}

#[test]
fn should_error_when_trying_to_update_ttl_for_a_missing_xdns_record() {
    ExtBuilder::default().build().execute_with(|| {
        assert_noop!(
            XDNS::update_ttl(Origin::<Runtime>::Root.into(), *b"miss", 2),
            pallet_xdns::pallet::Error::<Runtime>::XdnsRecordNotFound
        );
    });
}

#[test]
fn should_error_when_trying_to_update_ttl_as_non_root() {
    ExtBuilder::default().build().execute_with(|| {
        assert_noop!(
            XDNS::update_ttl(Origin::<Runtime>::Signed(ALICE).into(), *b"gate", 2),
            DispatchError::BadOrigin
        );
    });
}

#[test]
fn should_contain_gateway_system_properties() {
    ExtBuilder::default()
        .with_standard_side_effects()
        .with_default_xdns_records()
        .build()
        .execute_with(|| {
            let polkadot_xdns_record = pallet_xdns::XDNSRegistry::<Runtime>::get(b"pdot").unwrap();
            let kusama_xdns_record = pallet_xdns::XDNSRegistry::<Runtime>::get(b"ksma").unwrap();
            let polkadot_symbol: Vec<u8> =
                Decode::decode(&mut &polkadot_xdns_record.gateway_sys_props.token_symbol[..])
                    .unwrap();
            let kusama_symbol: Vec<u8> =
                Decode::decode(&mut &kusama_xdns_record.gateway_sys_props.token_symbol[..])
                    .unwrap();

            assert_eq!(polkadot_xdns_record.gateway_sys_props.ss58_format, 0u16);
            assert_eq!(kusama_xdns_record.gateway_sys_props.ss58_format, 2u16);
            assert_eq!(&String::from_utf8_lossy(&polkadot_symbol), "DOT");
            assert_eq!(&String::from_utf8_lossy(&kusama_symbol), "KSM");
            assert_eq!(polkadot_xdns_record.gateway_sys_props.token_decimals, 10u8);
            assert_eq!(kusama_xdns_record.gateway_sys_props.token_decimals, 12u8);
        });
}

#[test]
fn fetch_abi_should_return_abi_for_a_known_xdns_record() {
    ExtBuilder::default()
        .with_standard_side_effects()
        .with_default_xdns_records()
        .build()
        .execute_with(|| {
            let actual = XDNS::get_abi(*b"pdot");
            assert_ok!(actual);
        });
}

#[test]
fn fetch_abi_should_error_for_unknown_xdns_record() {
    ExtBuilder::default()
        .with_standard_side_effects()
        .with_default_xdns_records()
        .build()
        .execute_with(|| {
            let actual = XDNS::get_abi(*b"rand");
            assert_err!(actual, pallet_xdns::Error::<Runtime>::XdnsRecordNotFound);
        });
}

#[test]
fn gate_gateway_vendor_returns_error_for_unknown_record() {
    ExtBuilder::default()
        .with_default_xdns_records()
        .build()
        .execute_with(|| {
            let actual = XDNS::get_gateway_vendor(b"rand");
            assert_err!(actual, pallet_xdns::Error::<Runtime>::XdnsRecordNotFound);
        });
}

#[test]
fn gate_gateway_vendor_returns_vendor_for_known_record() {
    ExtBuilder::default()
        .with_default_xdns_records()
        .build()
        .execute_with(|| {
            let actual = XDNS::get_gateway_vendor(b"pdot");
            assert_ok!(actual, GatewayVendor::Rococo);
        });
}
