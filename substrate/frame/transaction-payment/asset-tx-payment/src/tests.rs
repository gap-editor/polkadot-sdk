// Copyright (C) Parity Technologies (UK) Ltd.
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

use super::*;

use frame_support::{
	assert_ok,
	dispatch::{DispatchInfo, GetDispatchInfo, PostDispatchInfo},
	pallet_prelude::*,
	traits::{fungibles::Mutate, OriginTrait},
	weights::Weight,
};
use frame_system as system;
use mock::{ExtrinsicBaseWeight, *};
use pallet_balances::Call as BalancesCall;
use sp_runtime::{
	traits::{DispatchTransaction, StaticLookup},
	BuildStorage,
};

const CALL: &<Runtime as frame_system::Config>::RuntimeCall =
	&RuntimeCall::Balances(BalancesCall::transfer_allow_death { dest: 2, value: 69 });

pub struct ExtBuilder {
	balance_factor: u64,
	base_weight: Weight,
	byte_fee: u64,
	weight_to_fee: u64,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			balance_factor: 1,
			base_weight: Weight::from_parts(0, 0),
			byte_fee: 1,
			weight_to_fee: 1,
		}
	}
}

impl ExtBuilder {
	pub fn base_weight(mut self, base_weight: Weight) -> Self {
		self.base_weight = base_weight;
		self
	}
	pub fn balance_factor(mut self, factor: u64) -> Self {
		self.balance_factor = factor;
		self
	}
	fn set_constants(&self) {
		ExtrinsicBaseWeight::mutate(|v| *v = self.base_weight);
		TRANSACTION_BYTE_FEE.with(|v| *v.borrow_mut() = self.byte_fee);
		WEIGHT_TO_FEE.with(|v| *v.borrow_mut() = self.weight_to_fee);
	}
	pub fn build(self) -> sp_io::TestExternalities {
		self.set_constants();
		let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
		pallet_balances::GenesisConfig::<Runtime> {
			balances: if self.balance_factor > 0 {
				vec![
					(1, 10 * self.balance_factor),
					(2, 20 * self.balance_factor),
					(3, 30 * self.balance_factor),
					(4, 40 * self.balance_factor),
					(5, 50 * self.balance_factor),
					(6, 60 * self.balance_factor),
				]
			} else {
				vec![]
			},
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();
		t.into()
	}
}

/// create a transaction info struct from weight. Handy to avoid building the whole struct.
pub fn info_from_weight(w: Weight) -> DispatchInfo {
	// pays_fee: Pays::Yes -- class: DispatchClass::Normal
	DispatchInfo { call_weight: w, ..Default::default() }
}

fn post_info_from_weight(w: Weight) -> PostDispatchInfo {
	PostDispatchInfo { actual_weight: Some(w), pays_fee: Default::default() }
}

fn info_from_pays(p: Pays) -> DispatchInfo {
	DispatchInfo { pays_fee: p, ..Default::default() }
}

fn post_info_from_pays(p: Pays) -> PostDispatchInfo {
	PostDispatchInfo { actual_weight: None, pays_fee: p }
}

fn default_post_info() -> PostDispatchInfo {
	PostDispatchInfo { actual_weight: None, pays_fee: Default::default() }
}

#[test]
fn transaction_payment_in_native_possible() {
	let balance_factor = 100;
	ExtBuilder::default()
		.balance_factor(balance_factor)
		.base_weight(Weight::from_parts(5, 0))
		.build()
		.execute_with(|| {
			let len = 10;
			let mut info = info_from_weight(Weight::from_parts(5, 0));
			let ext = ChargeAssetTxPayment::<Runtime>::from(0, None);
			info.extension_weight = ext.weight(CALL);
			let (pre, _) = ext.validate_and_prepare(Some(1).into(), CALL, &info, len, 0).unwrap();
			let initial_balance = 10 * balance_factor;
			assert_eq!(Balances::free_balance(1), initial_balance - 5 - 5 - 15 - 10);

			assert_ok!(ChargeAssetTxPayment::<Runtime>::post_dispatch_details(
				pre,
				&info,
				&default_post_info(),
				len,
				&Ok(()),
			));
			assert_eq!(Balances::free_balance(1), initial_balance - 5 - 5 - 15 - 10);

			let mut info = info_from_weight(Weight::from_parts(100, 0));
			let ext = ChargeAssetTxPayment::<Runtime>::from(5 /* tipped */, None);
			info.extension_weight = ext.weight(CALL);
			let (pre, _) = ext.validate_and_prepare(Some(2).into(), CALL, &info, len, 0).unwrap();
			let initial_balance_for_2 = 20 * balance_factor;
			assert_eq!(Balances::free_balance(2), initial_balance_for_2 - 5 - 10 - 100 - 15 - 5);

			let call_actual_weight = Weight::from_parts(50, 0);
			// The extension weight refund should be taken into account in `post_dispatch`.
			let post_info = post_info_from_weight(call_actual_weight.saturating_add(
				ChargeAssetTxPayment::<Runtime>::from(5 /* tipped */, None).weight(CALL),
			));
			assert_ok!(ChargeAssetTxPayment::<Runtime>::post_dispatch_details(
				pre,
				&info,
				&post_info,
				len,
				&Ok(()),
			));
			assert_eq!(
				post_info.actual_weight,
				Some(
					call_actual_weight
						.saturating_add(MockWeights::charge_asset_tx_payment_native())
				)
			);
			assert_eq!(Balances::free_balance(2), initial_balance_for_2 - 5 - 10 - 50 - 15 - 5);
		});
}

#[test]
fn transaction_payment_in_asset_possible() {
	let base_weight = 5;
	let balance_factor = 100;
	ExtBuilder::default()
		.balance_factor(balance_factor)
		.base_weight(Weight::from_parts(base_weight, 0))
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			// create the asset
			let asset_id = 1;
			let min_balance = 2;
			assert_ok!(Assets::force_create(
				RuntimeOrigin::root(),
				asset_id.into(),
				42,   /* owner */
				true, /* is_sufficient */
				min_balance
			));

			// mint into the caller account
			let caller = 1;
			let beneficiary = <Runtime as system::Config>::Lookup::unlookup(caller);
			let balance = 100;
			assert_ok!(Assets::mint_into(asset_id.into(), &beneficiary, balance));
			assert_eq!(Assets::balance(asset_id, caller), balance);
			let weight = 5;
			let len = 10;
			// we convert the from weight to fee based on the ratio between asset min balance and
			// existential deposit
			let fee = (base_weight + weight + len as u64) * min_balance / ExistentialDeposit::get();
			let (pre, _) = ChargeAssetTxPayment::<Runtime>::from(0, Some(asset_id))
				.validate_and_prepare(
					Some(caller).into(),
					CALL,
					&info_from_weight(Weight::from_parts(weight, 0)),
					len,
					0,
				)
				.unwrap();
			// assert that native balance is not used
			assert_eq!(Balances::free_balance(caller), 10 * balance_factor);
			// check that fee was charged in the given asset
			assert_eq!(Assets::balance(asset_id, caller), balance - fee);
			assert_eq!(Assets::balance(asset_id, BLOCK_AUTHOR), 0);

			System::assert_has_event(RuntimeEvent::Assets(pallet_assets::Event::Withdrawn {
				asset_id,
				who: caller,
				amount: fee,
			}));

			assert_ok!(ChargeAssetTxPayment::<Runtime>::post_dispatch_details(
				pre,
				&info_from_weight(Weight::from_parts(weight, 0)),
				&default_post_info(),
				len,
				&Ok(()),
			));
			assert_eq!(Assets::balance(asset_id, caller), balance - fee);
			// check that the block author gets rewarded
			assert_eq!(Assets::balance(asset_id, BLOCK_AUTHOR), fee);

			System::assert_has_event(RuntimeEvent::Assets(pallet_assets::Event::Deposited {
				asset_id,
				who: BLOCK_AUTHOR,
				amount: fee,
			}));
		});
}

#[test]
fn transaction_payment_without_fee() {
	let base_weight = 5;
	let balance_factor = 100;
	ExtBuilder::default()
		.balance_factor(balance_factor)
		.base_weight(Weight::from_parts(base_weight, 0))
		.build()
		.execute_with(|| {
			// create the asset
			let asset_id = 1;
			let min_balance = 2;
			assert_ok!(Assets::force_create(
				RuntimeOrigin::root(),
				asset_id.into(),
				42,   /* owner */
				true, /* is_sufficient */
				min_balance
			));

			// mint into the caller account
			let caller = 1;
			let beneficiary = <Runtime as system::Config>::Lookup::unlookup(caller);
			let balance = 100;
			assert_ok!(Assets::mint_into(asset_id.into(), &beneficiary, balance));
			assert_eq!(Assets::balance(asset_id, caller), balance);
			let weight = 5;
			let len = 10;
			// we convert the from weight to fee based on the ratio between asset min balance and
			// existential deposit
			let fee = (base_weight + weight + len as u64) * min_balance / ExistentialDeposit::get();
			let (pre, _) = ChargeAssetTxPayment::<Runtime>::from(0, Some(asset_id))
				.validate_and_prepare(
					Some(caller).into(),
					CALL,
					&info_from_weight(Weight::from_parts(weight, 0)),
					len,
					0,
				)
				.unwrap();
			// assert that native balance is not used
			assert_eq!(Balances::free_balance(caller), 10 * balance_factor);
			// check that fee was charged in the given asset
			assert_eq!(Assets::balance(asset_id, caller), balance - fee);
			assert_eq!(Assets::balance(asset_id, BLOCK_AUTHOR), 0);

			assert_ok!(ChargeAssetTxPayment::<Runtime>::post_dispatch_details(
				pre,
				&info_from_weight(Weight::from_parts(weight, 0)),
				&post_info_from_pays(Pays::No),
				len,
				&Ok(()),
			));
			// caller should be refunded
			assert_eq!(Assets::balance(asset_id, caller), balance);
			// check that the block author did not get rewarded
			assert_eq!(Assets::balance(asset_id, BLOCK_AUTHOR), 0);
		});
}

#[test]
fn asset_transaction_payment_with_tip_and_refund() {
	let base_weight = 5;
	ExtBuilder::default()
		.balance_factor(100)
		.base_weight(Weight::from_parts(base_weight, 0))
		.build()
		.execute_with(|| {
			System::set_block_number(1);

			// create the asset
			let asset_id = 1;
			let min_balance = 2;
			assert_ok!(Assets::force_create(
				RuntimeOrigin::root(),
				asset_id.into(),
				42,   /* owner */
				true, /* is_sufficient */
				min_balance
			));

			// mint into the caller account
			let caller = 2;
			let beneficiary = <Runtime as system::Config>::Lookup::unlookup(caller);
			let balance = 1000;
			assert_ok!(Assets::mint_into(asset_id.into(), &beneficiary, balance));
			assert_eq!(Assets::balance(asset_id, caller), balance);
			let weight = 100;
			let tip = 5;
			let ext = ChargeAssetTxPayment::<Runtime>::from(tip, Some(asset_id));
			let ext_weight = ext.weight(CALL);
			let len = 10;
			// we convert the from weight to fee based on the ratio between asset min balance and
			// existential deposit
			let fee_with_tip = (base_weight + weight + ext_weight.ref_time() + len as u64 + tip) *
				min_balance / ExistentialDeposit::get();
			let mut info = info_from_weight(Weight::from_parts(weight, 0));
			info.extension_weight = ext_weight;
			let (pre, _) =
				ext.validate_and_prepare(Some(caller).into(), CALL, &info, len, 0).unwrap();
			assert_eq!(Assets::balance(asset_id, caller), balance - fee_with_tip);

			System::assert_has_event(RuntimeEvent::Assets(pallet_assets::Event::Withdrawn {
				asset_id,
				who: caller,
				amount: fee_with_tip,
			}));

			let final_weight = 50;
			let mut post_info = post_info_from_weight(Weight::from_parts(final_weight, 0));
			post_info
				.actual_weight
				.as_mut()
				.map(|w| w.saturating_accrue(MockWeights::charge_asset_tx_payment_asset()));
			assert_ok!(ChargeAssetTxPayment::<Runtime>::post_dispatch_details(
				pre,
				&info,
				&post_info,
				len,
				&Ok(()),
			));
			let final_fee = fee_with_tip -
				(weight - final_weight + ext_weight.ref_time() -
					MockWeights::charge_asset_tx_payment_asset().ref_time()) *
					min_balance / ExistentialDeposit::get();
			assert_eq!(Assets::balance(asset_id, caller), balance - (final_fee));
			assert_eq!(Assets::balance(asset_id, BLOCK_AUTHOR), final_fee);

			System::assert_has_event(RuntimeEvent::Assets(pallet_assets::Event::Deposited {
				asset_id,
				who: caller,
				amount: fee_with_tip - final_fee,
			}));
		});
}

#[test]
fn payment_from_account_with_only_assets() {
	let base_weight = 5;
	ExtBuilder::default()
		.balance_factor(100)
		.base_weight(Weight::from_parts(base_weight, 0))
		.build()
		.execute_with(|| {
			// create the asset
			let asset_id = 1;
			let min_balance = 2;
			assert_ok!(Assets::force_create(
				RuntimeOrigin::root(),
				asset_id.into(),
				42,   /* owner */
				true, /* is_sufficient */
				min_balance
			));

			// mint into the caller account
			let caller = 333;
			let beneficiary = <Runtime as system::Config>::Lookup::unlookup(caller);
			let balance = 100;
			assert_ok!(Assets::mint_into(asset_id.into(), &beneficiary, balance));
			assert_eq!(Assets::balance(asset_id, caller), balance);
			// assert that native balance is not necessary
			assert_eq!(Balances::free_balance(caller), 0);
			let weight = 5;
			let len = 10;
			// we convert the from weight to fee based on the ratio between asset min balance and
			// existential deposit
			let fee = (base_weight + weight + len as u64) * min_balance / ExistentialDeposit::get();
			let (pre, _) = ChargeAssetTxPayment::<Runtime>::from(0, Some(asset_id))
				.validate_and_prepare(
					Some(caller).into(),
					CALL,
					&info_from_weight(Weight::from_parts(weight, 0)),
					len,
					0,
				)
				.unwrap();
			assert_eq!(Balances::free_balance(caller), 0);
			// check that fee was charged in the given asset
			assert_eq!(Assets::balance(asset_id, caller), balance - fee);

			assert_ok!(ChargeAssetTxPayment::<Runtime>::post_dispatch_details(
				pre,
				&info_from_weight(Weight::from_parts(weight, 0)),
				&default_post_info(),
				len,
				&Ok(()),
			));
			assert_eq!(Assets::balance(asset_id, caller), balance - fee);
			assert_eq!(Balances::free_balance(caller), 0);
		});
}

#[test]
fn payment_only_with_existing_sufficient_asset() {
	let base_weight = 5;
	ExtBuilder::default()
		.balance_factor(100)
		.base_weight(Weight::from_parts(base_weight, 0))
		.build()
		.execute_with(|| {
			let asset_id = 1;
			let caller = 1;
			let weight = 5;
			let len = 10;
			// pre_dispatch fails for non-existent asset
			assert!(ChargeAssetTxPayment::<Runtime>::from(0, Some(asset_id))
				.validate_and_prepare(
					Some(caller).into(),
					CALL,
					&info_from_weight(Weight::from_parts(weight, 0)),
					len,
					0,
				)
				.is_err());

			// create the non-sufficient asset
			let min_balance = 2;
			assert_ok!(Assets::force_create(
				RuntimeOrigin::root(),
				asset_id.into(),
				42,    /* owner */
				false, /* is_sufficient */
				min_balance
			));
			// pre_dispatch fails for non-sufficient asset
			assert!(ChargeAssetTxPayment::<Runtime>::from(0, Some(asset_id))
				.validate_and_prepare(
					Some(caller).into(),
					CALL,
					&info_from_weight(Weight::from_parts(weight, 0)),
					len,
					0,
				)
				.is_err());
		});
}

#[test]
fn converted_fee_is_never_zero_if_input_fee_is_not() {
	let base_weight = 1;
	ExtBuilder::default()
		.balance_factor(100)
		.base_weight(Weight::from_parts(base_weight, 0))
		.build()
		.execute_with(|| {
			// create the asset
			let asset_id = 1;
			let min_balance = 1;
			assert_ok!(Assets::force_create(
				RuntimeOrigin::root(),
				asset_id.into(),
				42,   /* owner */
				true, /* is_sufficient */
				min_balance
			));

			// mint into the caller account
			let caller = 333;
			let beneficiary = <Runtime as system::Config>::Lookup::unlookup(caller);
			let balance = 100;
			assert_ok!(Assets::mint_into(asset_id.into(), &beneficiary, balance));
			assert_eq!(Assets::balance(asset_id, caller), balance);
			let weight = 1;
			let len = 1;
			// we convert the from weight to fee based on the ratio between asset min balance and
			// existential deposit
			let fee = (base_weight + weight + len as u64) * min_balance / ExistentialDeposit::get();
			// naive fee calculation would round down to zero
			assert_eq!(fee, 0);
			{
				let (pre, _) = ChargeAssetTxPayment::<Runtime>::from(0, Some(asset_id))
					.validate_and_prepare(
						Some(caller).into(),
						CALL,
						&info_from_pays(Pays::No),
						len,
						0,
					)
					.unwrap();
				// `Pays::No` still implies no fees
				assert_eq!(Assets::balance(asset_id, caller), balance);

				assert_ok!(ChargeAssetTxPayment::<Runtime>::post_dispatch_details(
					pre,
					&info_from_pays(Pays::No),
					&post_info_from_pays(Pays::No),
					len,
					&Ok(()),
				));
				assert_eq!(Assets::balance(asset_id, caller), balance);
			}
			let (pre, _) = ChargeAssetTxPayment::<Runtime>::from(0, Some(asset_id))
				.validate_and_prepare(
					Some(caller).into(),
					CALL,
					&info_from_weight(Weight::from_parts(weight, 0)),
					len,
					0,
				)
				.unwrap();
			// check that at least one coin was charged in the given asset
			assert_eq!(Assets::balance(asset_id, caller), balance - 1);

			assert_ok!(ChargeAssetTxPayment::<Runtime>::post_dispatch_details(
				pre,
				&info_from_weight(Weight::from_parts(weight, 0)),
				&default_post_info(),
				len,
				&Ok(()),
			));
			assert_eq!(Assets::balance(asset_id, caller), balance - 1);
		});
}

#[test]
fn post_dispatch_fee_is_zero_if_pre_dispatch_fee_is_zero() {
	let base_weight = 1;
	ExtBuilder::default()
		.balance_factor(100)
		.base_weight(Weight::from_parts(base_weight, 0))
		.build()
		.execute_with(|| {
			// create the asset
			let asset_id = 1;
			let min_balance = 100;
			assert_ok!(Assets::force_create(
				RuntimeOrigin::root(),
				asset_id.into(),
				42,   /* owner */
				true, /* is_sufficient */
				min_balance
			));

			// mint into the caller account
			let caller = 333;
			let beneficiary = <Runtime as system::Config>::Lookup::unlookup(caller);
			let balance = 100;
			assert_ok!(Assets::mint_into(asset_id.into(), &beneficiary, balance));
			assert_eq!(Assets::balance(asset_id, caller), balance);
			let weight = 1;
			let len = 1;
			// we convert the from weight to fee based on the ratio between asset min balance and
			// existential deposit
			let fee = (base_weight + weight + len as u64) * min_balance / ExistentialDeposit::get();
			// calculated fee is greater than 0
			assert!(fee > 0);
			let (pre, _) = ChargeAssetTxPayment::<Runtime>::from(0, Some(asset_id))
				.validate_and_prepare(Some(caller).into(), CALL, &info_from_pays(Pays::No), len, 0)
				.unwrap();
			// `Pays::No` implies no pre-dispatch fees
			assert_eq!(Assets::balance(asset_id, caller), balance);
			let Pre::Charge { initial_payment, .. } = &pre else {
				panic!("Expected Charge");
			};
			let not_paying = match initial_payment {
				&InitialPayment::Nothing => true,
				_ => false,
			};
			assert!(not_paying, "initial payment should be Nothing if we pass Pays::No");

			// `Pays::Yes` on post-dispatch does not mean we pay (we never charge more than the
			// initial fee)
			assert_ok!(ChargeAssetTxPayment::<Runtime>::post_dispatch_details(
				pre,
				&info_from_pays(Pays::No),
				&post_info_from_pays(Pays::Yes),
				len,
				&Ok(()),
			));
			assert_eq!(Assets::balance(asset_id, caller), balance);
		});
}

#[test]
fn no_fee_and_no_weight_for_other_origins() {
	ExtBuilder::default().build().execute_with(|| {
		let ext = ChargeAssetTxPayment::<Runtime>::from(0, None);

		let mut info = CALL.get_dispatch_info();
		info.extension_weight = ext.weight(CALL);

		// Ensure we test the refund.
		assert!(info.extension_weight != Weight::zero());

		let len = CALL.encoded_size();

		let origin = frame_system::RawOrigin::Root.into();
		let (pre, origin) = ext.validate_and_prepare(origin, CALL, &info, len, 0).unwrap();

		assert!(origin.as_system_ref().unwrap().is_root());

		let pd_res = Ok(());
		let mut post_info = frame_support::dispatch::PostDispatchInfo {
			actual_weight: Some(info.total_weight()),
			pays_fee: Default::default(),
		};

		<ChargeAssetTxPayment<Runtime> as TransactionExtension<RuntimeCall>>::post_dispatch(
			pre,
			&info,
			&mut post_info,
			len,
			&pd_res,
		)
		.unwrap();

		assert_eq!(post_info.actual_weight, Some(info.call_weight));
	})
}
