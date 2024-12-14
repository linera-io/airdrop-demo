// Copyright (c) Zefchain Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![cfg_attr(target_arch = "wasm32", no_main)]

#[cfg(test)]
mod contract_unit_tests;
mod state;

use airdrop_demo::{AirDropClaim, AirDropId, Parameters};
use alloy_primitives::Address;
use linera_sdk::{
    abis::fungible::{self, Account},
    base::{AccountOwner, Amount, WithContractAbi},
    views::{RootView, View},
    Contract, ContractRuntime,
};
use serde::{Deserialize, Serialize};
use log::{info, warn, error}; // Added logging

use self::state::Application;

pub struct ApplicationContract {
    state: Application,
    runtime: ContractRuntime<Self>,
}

linera_sdk::contract!(ApplicationContract);

impl WithContractAbi for ApplicationContract {
    type Abi = airdrop_demo::ApplicationAbi;
}

impl Contract for ApplicationContract {
    type Message = ApprovedAirDrop;
    type Parameters = Parameters;
    type InstantiationArgument = ();

    /// Loads the contract state.
    async fn load(runtime: ContractRuntime<Self>) -> Result<Self, String> {
        let state = Application::load(runtime.root_view_storage_context())
            .await
            .map_err(|e| format!("Failed to load state: {e}"))?;
        Ok(ApplicationContract { state, runtime })
    }

    /// Instantiates the application.
    async fn instantiate(&mut self, _argument: Self::InstantiationArgument) {
        // Check if parameters are valid.
        let _parameters = self.runtime.application_parameters();
    }

    /// Executes the operation related to the airdrop claim.
    async fn execute_operation(&mut self, claim: Self::Operation) -> Self::Response {
        let creator_chain = self.runtime.application_creator_chain_id();
        let amount = self.airdrop_amount(&claim).await;
        let application_id = self.runtime.application_id();
        let claimer = claim
            .signer_address(application_id)
            .expect("Failed to verify signature");

        self.assert_eligibility(&claimer, &claim.api_token).await;

        // Send message to the creator chain to deliver the tokens.
        self.runtime
            .prepare_message(ApprovedAirDrop {
                id: claimer.into(),
                amount,
                destination: claim.destination,
            })
            .with_authentication()
            .send_to(creator_chain);
    }

    /// Handles the message if the airdrop was successfully approved.
    async fn execute_message(&mut self, airdrop: Self::Message) {
        self.track_claim(&airdrop.id).await.unwrap();

        let parameters = self.runtime.application_parameters();
        let source_account = AccountOwner::Application(self.runtime.application_id().forget_abi());

        let transfer = fungible::Operation::Transfer {
            owner: source_account,
            amount: airdrop.amount,
            target_account: airdrop.destination,
        };

        self.runtime
            .call_application(true, parameters.token_id, &transfer);
    }

    /// Stores the contract state.
    async fn store(mut self) {
        self.state.save().await.expect("Failed to save state");
    }
}

impl ApplicationContract {
    /// Checks if the address is eligible for the airdrop.
    pub async fn assert_eligibility(&mut self, address: &Address, api_token: &str) {
        match self.query_eligibility(&address.to_string(), api_token).await {
            Ok(is_eligible) => {
                if !is_eligible {
                    warn!("Address {} is not eligible for airdrop.", address);
                }
                assert!(is_eligible, "Address is not eligible for airdrop");
            }
            Err(err) => {
                error!("Failed to query eligibility: {}", err);
                panic!("Eligibility check failed.");
            }
        }
    }

    /// Queries the service to check eligibility for the airdrop.
    async fn query_eligibility(&self, address: &str, api_token: &str) -> Result<bool, String> {
        let query = format!(
            r#"query {{ checkEligibility(address: "{address}", apiToken: "{api_token}") }}"#
        );
        let request = async_graphql::Request::new(query);

        let response = self.runtime.query_service(self.runtime.application_id(), request).await;
        let data = response
            .data
            .get("checkEligibility")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| "Failed to get eligibility from response".to_string())?;
        
        Ok(data)
    }

    /// Calculates the amount to be airdropped for a single claim.
    async fn airdrop_amount(&mut self, _claim: &AirDropClaim) -> Amount {
        Amount::ONE // You can implement your own logic for calculating the amount
    }

    /// Tracks the claim and aborts execution if it has already been processed.
    async fn track_claim(&mut self, airdrop: &AirDropId) -> Result<(), String> {
        if self.has_claim_been_processed(airdrop).await? {
            return Err("Airdrop has already been paid".into());
        }

        self.state.handled_airdrops.insert(airdrop).map_err(|e| format!("Failed to insert claim: {e}"))?;
        Ok(())
    }

    /// Checks if the claim has already been processed.
    async fn has_claim_been_processed(&self, airdrop: &AirDropId) -> Result<bool, String> {
        self.state
            .handled_airdrops
            .contains(airdrop)
            .await
            .map_err(|e| format!("Failed to check claim status: {e}"))
    }

    /// Saves the updated contract state.
    async fn store_state(&mut self) {
        self.state.save().await.expect("Failed to save state");
    }
}

/// An approved airdrop that is sent to the creator chain for token delivery.
#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(test, derive(Clone, Eq, PartialEq))]
pub struct ApprovedAirDrop {
    id: AirDropId,
    amount: Amount,
    destination: Account,
}

