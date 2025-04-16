// Copyright (c) Zefchain Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use alloy_sol_types::{eip712_domain, sol, Eip712Domain};
use linera_sdk::{abis::fungible, bcs, linera_base_types::ApplicationId};

use crate::{ApplicationAbi, ETHEREUM_MAINNET_CHAIN_ID};

/// The EIP-712 domain for this application.
pub const AIRDROP_CLAIM_DOMAIN: Eip712Domain = eip712_domain! {
    name: "Linera AirDrop demo",
    version: "0.0.1",
    chain_id: ETHEREUM_MAINNET_CHAIN_ID,
};

sol! {
    /// EIP-712 representation of an airdrop claim.
    struct AirDropClaim {
        string appId;
        FungibleAccount claimer;
    }

    /// EIP-712 representation of a destination account.
    struct FungibleAccount {
        string chainId;
        string owner;
    }
}

impl AirDropClaim {
    /// Creates a new [`AirDropClaim`] to be used in a signature's payload.
    pub fn new(application_id: ApplicationId<ApplicationAbi>, claimer: &fungible::Account) -> Self {
        let application_id_bytes =
            bcs::to_bytes(&application_id).expect("`ApplicationId`s should be serializable");

        AirDropClaim {
            appId: hex::encode(application_id_bytes),
            claimer: claimer.into(),
        }
    }
}

impl From<&fungible::Account> for FungibleAccount {
    fn from(account: &fungible::Account) -> Self {
        FungibleAccount {
            chainId: account.chain_id.to_string(),
            owner: account.owner.to_string(),
        }
    }
}
