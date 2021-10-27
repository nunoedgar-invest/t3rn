//! Autogenerated weights for pallet_circuit_execution_delivery
//!
//! THIS FILE WAS AUTO-GENERATED USING THE SUBSTRATE BENCHMARK CLI VERSION 3.0.0
//! DATE: 2021-09-24, STEPS: `[50, ]`, REPEAT: 100, LOW RANGE: `[]`, HIGH RANGE: `[]`
//! EXECUTION: Some(Wasm), WASM-EXECUTION: Compiled, CHAIN: Some("dev"), DB CACHE: 128

// Executed Command:
// target/release/circuit
// benchmark
// --chain=dev
// --steps=50
// --repeat=100
// --pallet=pallet_circuit_execution_delivery
// --extrinsic=*
// --execution=wasm
// --wasm-execution=compiled
// --heap-pages=4096
// --output=./src/execution-delivery/src/weights.rs
// --template=../benchmarking/frame-weight-template.hbs

#![allow(unused_parens)]
#![allow(unused_imports)]

use frame_support::{
    traits::Get,
    weights::{constants::RocksDbWeight, Weight},
};
use sp_std::marker::PhantomData;

/// Weight functions needed for pallet_circuit_execution_delivery.
pub trait WeightInfo {
    fn decompose_io_schedule() -> Weight;
    fn register_gateway_default_polka() -> Weight;
    fn register_gateway_polka_u64() -> Weight;
    fn register_gateway_default_eth() -> Weight;
    fn register_gateway_eth_u64() -> Weight;
    fn dry_run_whole_xtx_one_component() -> Weight;
    fn dry_run_whole_xtx_three_components() -> Weight;
}

/// Weights for pallet_circuit_execution_delivery using the Substrate node and recommended hardware.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
    fn decompose_io_schedule() -> Weight {
        (6_984_000 as Weight)
    }
    fn register_gateway_default_polka() -> Weight {
        (68_373_000 as Weight)
            .saturating_add(T::DbWeight::get().reads(4 as Weight))
            .saturating_add(T::DbWeight::get().writes(7 as Weight))
    }
    fn register_gateway_polka_u64() -> Weight {
        (68_058_000 as Weight)
            .saturating_add(T::DbWeight::get().reads(4 as Weight))
            .saturating_add(T::DbWeight::get().writes(7 as Weight))
    }
    fn register_gateway_default_eth() -> Weight {
        (68_073_000 as Weight)
            .saturating_add(T::DbWeight::get().reads(4 as Weight))
            .saturating_add(T::DbWeight::get().writes(7 as Weight))
    }
    fn register_gateway_eth_u64() -> Weight {
        (67_939_000 as Weight)
            .saturating_add(T::DbWeight::get().reads(4 as Weight))
            .saturating_add(T::DbWeight::get().writes(7 as Weight))
    }
    fn dry_run_whole_xtx_one_component() -> Weight {
        (14_094_000 as Weight).saturating_add(T::DbWeight::get().reads(1 as Weight))
    }
    fn dry_run_whole_xtx_three_components() -> Weight {
        (14_757_000 as Weight).saturating_add(T::DbWeight::get().reads(1 as Weight))
    }
}

// For backwards compatibility and tests
impl WeightInfo for () {
    fn decompose_io_schedule() -> Weight {
        (6_984_000 as Weight)
    }
    fn register_gateway_default_polka() -> Weight {
        (68_373_000 as Weight)
            .saturating_add(RocksDbWeight::get().reads(4 as Weight))
            .saturating_add(RocksDbWeight::get().writes(7 as Weight))
    }
    fn register_gateway_polka_u64() -> Weight {
        (68_058_000 as Weight)
            .saturating_add(RocksDbWeight::get().reads(4 as Weight))
            .saturating_add(RocksDbWeight::get().writes(7 as Weight))
    }
    fn register_gateway_default_eth() -> Weight {
        (68_073_000 as Weight)
            .saturating_add(RocksDbWeight::get().reads(4 as Weight))
            .saturating_add(RocksDbWeight::get().writes(7 as Weight))
    }
    fn register_gateway_eth_u64() -> Weight {
        (67_939_000 as Weight)
            .saturating_add(RocksDbWeight::get().reads(4 as Weight))
            .saturating_add(RocksDbWeight::get().writes(7 as Weight))
    }
    fn dry_run_whole_xtx_one_component() -> Weight {
        (14_094_000 as Weight).saturating_add(RocksDbWeight::get().reads(1 as Weight))
    }
    fn dry_run_whole_xtx_three_components() -> Weight {
        (14_757_000 as Weight).saturating_add(RocksDbWeight::get().reads(1 as Weight))
    }
}