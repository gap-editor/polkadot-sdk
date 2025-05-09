// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Requests and responses as sent over the wire for the individual protocols.

use codec::{Decode, Encode};

use polkadot_node_primitives::{
	AvailableData, DisputeMessage, ErasureChunk, PoV, Proof, UncheckedDisputeMessage,
};
use polkadot_primitives::{
	vstaging::CandidateReceiptV2 as CandidateReceipt, CandidateHash, Hash, HeadData, Id as ParaId,
	ValidatorIndex,
};

use super::{IsRequest, Protocol};

/// Request an availability chunk.
#[derive(Debug, Copy, Clone, Encode, Decode)]
pub struct ChunkFetchingRequest {
	/// Hash of candidate we want a chunk for.
	pub candidate_hash: CandidateHash,
	/// The validator index we are requesting from. This must be identical to the index of the
	/// chunk we'll receive. For v2, this may not be the case.
	pub index: ValidatorIndex,
}

/// Receive a requested erasure chunk.
#[derive(Debug, Clone, Encode, Decode)]
pub enum ChunkFetchingResponse {
	/// The requested chunk data.
	#[codec(index = 0)]
	Chunk(ChunkResponse),
	/// Node was not in possession of the requested chunk.
	#[codec(index = 1)]
	NoSuchChunk,
}

impl From<Option<ChunkResponse>> for ChunkFetchingResponse {
	fn from(x: Option<ChunkResponse>) -> Self {
		match x {
			Some(c) => ChunkFetchingResponse::Chunk(c),
			None => ChunkFetchingResponse::NoSuchChunk,
		}
	}
}

impl From<ChunkFetchingResponse> for Option<ChunkResponse> {
	fn from(x: ChunkFetchingResponse) -> Self {
		match x {
			ChunkFetchingResponse::Chunk(c) => Some(c),
			ChunkFetchingResponse::NoSuchChunk => None,
		}
	}
}

/// Skimmed down variant of `ErasureChunk`.
///
/// Instead of transmitting a full `ErasureChunk` we transmit `ChunkResponse` in
/// `ChunkFetchingResponse`, which omits the chunk's index. The index is already known by
/// the requester and by not transmitting it, we ensure the requester is going to use his index
/// value for validating the response, thus making sure he got what he requested.
#[derive(Debug, Clone, Encode, Decode)]
pub struct ChunkResponse {
	/// The erasure-encoded chunk of data belonging to the candidate block.
	pub chunk: Vec<u8>,
	/// Proof for this chunk's branch in the Merkle tree.
	pub proof: Proof,
}

impl From<ErasureChunk> for ChunkResponse {
	fn from(ErasureChunk { chunk, index: _, proof }: ErasureChunk) -> Self {
		ChunkResponse { chunk, proof }
	}
}

impl ChunkResponse {
	/// Re-build an `ErasureChunk` from response and request.
	pub fn recombine_into_chunk(self, req: &ChunkFetchingRequest) -> ErasureChunk {
		ErasureChunk { chunk: self.chunk, proof: self.proof, index: req.index.into() }
	}
}

impl IsRequest for ChunkFetchingRequest {
	type Response = ChunkFetchingResponse;
	const PROTOCOL: Protocol = Protocol::ChunkFetchingV1;
}

/// Request the advertised collation at that relay-parent.
#[derive(Debug, Clone, Encode, Decode)]
pub struct CollationFetchingRequest {
	/// Relay parent we want a collation for.
	pub relay_parent: Hash,
	/// The `ParaId` of the collation.
	pub para_id: ParaId,
}

/// Responses as sent by collators.
#[derive(Debug, Clone, Encode, Decode)]
pub enum CollationFetchingResponse {
	/// Deliver requested collation.
	#[codec(index = 0)]
	Collation(CandidateReceipt, PoV),

	/// Deliver requested collation along with parent head data.
	#[codec(index = 1)]
	CollationWithParentHeadData {
		/// The receipt of the candidate.
		receipt: CandidateReceipt,
		/// Candidate's proof of validity.
		pov: PoV,
		/// The head data of the candidate's parent.
		/// This is needed for elastic scaling to work.
		parent_head_data: HeadData,
	},
}

impl IsRequest for CollationFetchingRequest {
	type Response = CollationFetchingResponse;
	const PROTOCOL: Protocol = Protocol::CollationFetchingV1;
}

/// Request the advertised collation at that relay-parent.
#[derive(Debug, Clone, Encode, Decode)]
pub struct PoVFetchingRequest {
	/// Candidate we want a PoV for.
	pub candidate_hash: CandidateHash,
}

/// Responses to `PoVFetchingRequest`.
#[derive(Debug, Clone, Encode, Decode)]
pub enum PoVFetchingResponse {
	/// Deliver requested PoV.
	#[codec(index = 0)]
	PoV(PoV),
	/// PoV was not found in store.
	#[codec(index = 1)]
	NoSuchPoV,
}

impl IsRequest for PoVFetchingRequest {
	type Response = PoVFetchingResponse;
	const PROTOCOL: Protocol = Protocol::PoVFetchingV1;
}

/// Request the entire available data for a candidate.
#[derive(Debug, Clone, Encode, Decode)]
pub struct AvailableDataFetchingRequest {
	/// The candidate hash to get the available data for.
	pub candidate_hash: CandidateHash,
}

/// Receive a requested available data.
#[derive(Debug, Clone, Encode, Decode)]
pub enum AvailableDataFetchingResponse {
	/// The requested data.
	#[codec(index = 0)]
	AvailableData(AvailableData),
	/// Node was not in possession of the requested data.
	#[codec(index = 1)]
	NoSuchData,
}

impl From<Option<AvailableData>> for AvailableDataFetchingResponse {
	fn from(x: Option<AvailableData>) -> Self {
		match x {
			Some(data) => AvailableDataFetchingResponse::AvailableData(data),
			None => AvailableDataFetchingResponse::NoSuchData,
		}
	}
}

impl IsRequest for AvailableDataFetchingRequest {
	type Response = AvailableDataFetchingResponse;
	const PROTOCOL: Protocol = Protocol::AvailableDataFetchingV1;
}

/// A dispute request.
///
/// Contains an invalid vote a valid one for a particular candidate in a given session.
#[derive(Clone, Encode, Decode, Debug)]
pub struct DisputeRequest(pub UncheckedDisputeMessage);

impl From<DisputeMessage> for DisputeRequest {
	fn from(msg: DisputeMessage) -> Self {
		Self(msg.into())
	}
}

/// Possible responses to a `DisputeRequest`.
#[derive(Encode, Decode, Debug, PartialEq, Eq)]
pub enum DisputeResponse {
	/// Recipient successfully processed the dispute request.
	#[codec(index = 0)]
	Confirmed,
}

impl IsRequest for DisputeRequest {
	type Response = DisputeResponse;
	const PROTOCOL: Protocol = Protocol::DisputeSendingV1;
}
