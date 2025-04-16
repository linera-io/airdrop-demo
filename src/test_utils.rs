// Copyright (c) Zefchain Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Helper functions used in tests.

use alloy_primitives::Signature;
use alloy_sol_types::SolStruct;
use k256::ecdsa::SigningKey;
use linera_sdk::{
    abis::fungible,
    linera_base_types::{ApplicationId, CryptoHash},
};

use crate::{
    signature_payload::{self, AIRDROP_CLAIM_DOMAIN},
    ApplicationAbi,
};

/// Creates a dummy [`ApplicationId`] to use as the Fungible Token for testing.
pub fn create_dummy_token_id<Abi>() -> ApplicationId<Abi> {
    create_dummy_application_id("fungible token")
}

/// Creates a dummy [`ApplicationId`] to use for testing.
pub fn create_dummy_application_id<Abi>(name: &str) -> ApplicationId<Abi> {
    ApplicationId::new(CryptoHash::test_hash(name)).with_abi()
}

/// Creates a [`Signature`] for an airdrop claim.
pub fn sign_claim(
    signer: &SigningKey,
    application_id: ApplicationId<ApplicationAbi>,
    claimer: fungible::Account,
) -> Signature {
    let payload = signature_payload::AirDropClaim::new(application_id, &claimer);

    let hash = payload.eip712_signing_hash(&AIRDROP_CLAIM_DOMAIN);

    signer
        .sign_prehash_recoverable(hash.as_slice())
        .expect("Payload hash should be signable with `SigningKey`")
        .into()
}
