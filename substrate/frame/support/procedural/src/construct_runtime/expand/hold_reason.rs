// This file is part of Substrate.

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
// limitations under the License

use super::composite_helper;
use crate::construct_runtime::Pallet;
use proc_macro2::TokenStream;
use quote::quote;

pub fn expand_outer_hold_reason(pallet_decls: &[Pallet], scrate: &TokenStream) -> TokenStream {
	let mut conversion_fns = Vec::new();
	let mut hold_reason_variants = Vec::new();
	let mut hold_reason_variants_count = Vec::new();
	for decl in pallet_decls {
		if let Some(_) = decl.find_part("HoldReason") {
			let variant_name = &decl.name;
			let path = &decl.path;
			let index = decl.index;
			let instance = decl.instance.as_ref();

			conversion_fns.push(composite_helper::expand_conversion_fn(
				"HoldReason",
				path,
				instance,
				variant_name,
			));

			hold_reason_variants.push(composite_helper::expand_variant(
				"HoldReason",
				index,
				path,
				instance,
				variant_name,
			));

			hold_reason_variants_count.push(composite_helper::expand_variant_count(
				"HoldReason",
				path,
				instance,
			));
		}
	}

	quote! {
		/// A reason for placing a hold on funds.
		#[derive(
			Copy, Clone, Eq, PartialEq,
			#scrate::__private::codec::Encode,
			#scrate::__private::codec::Decode,
			#scrate::__private::codec::DecodeWithMemTracking,
			#scrate::__private::codec::MaxEncodedLen,
			#scrate::__private::scale_info::TypeInfo,
			#scrate::__private::RuntimeDebug,
		)]
		pub enum RuntimeHoldReason {
			#( #hold_reason_variants )*
		}

		impl #scrate::traits::VariantCount for RuntimeHoldReason {
			const VARIANT_COUNT: u32 = 0 #( + #hold_reason_variants_count )*;
		}

		#( #conversion_fns )*
	}
}
