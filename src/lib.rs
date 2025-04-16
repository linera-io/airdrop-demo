// Copyright (c) Zefchain Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod signature_payload;
#[cfg(feature = "test")]
pub mod test_utils;

use std::str::FromStr;

use alloy_primitives::{Address, Signature, SignatureError, U256};
use alloy_sol_types::SolStruct;
use indexmap::IndexMap;
use linera_sdk::{
    abis::fungible::{Account, FungibleTokenAbi},
    linera_base_types::{ApplicationId, ContractAbi, ServiceAbi},
};
use serde::{Deserialize, Serialize};

use self::signature_payload::AIRDROP_CLAIM_DOMAIN;

pub struct ApplicationAbi;

impl ContractAbi for ApplicationAbi {
    type Operation = AirDropClaim;
    type Response = ();
}

impl ServiceAbi for ApplicationAbi {
    type Query = async_graphql::Request;
    type QueryResponse = async_graphql::Response;
}

/// The shared parameters that are specified when the application is instantiated.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Parameters {
    pub token_id: ApplicationId<FungibleTokenAbi>,
    pub snapshot_block: u64,
    pub minimum_balance: U256,
}

/// The information necessary to identify an airdrop.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AirDropId {
    external_address: Address,
}

impl From<Address> for AirDropId {
    fn from(external_address: Address) -> Self {
        AirDropId { external_address }
    }
}

#[async_graphql::Scalar]
impl async_graphql::ScalarType for AirDropId {
    fn parse(value: async_graphql::Value) -> async_graphql::InputValueResult<Self> {
        match value {
            async_graphql::Value::List(bytes_list) => {
                let parse_bytes_result = bytes_list
                    .into_iter()
                    .map(<u8 as async_graphql::ScalarType>::parse)
                    .collect::<Result<Vec<u8>, _>>();

                let prepare_bytes_result = parse_bytes_result.and_then(|bytes| {
                    <[u8; 20]>::try_from(bytes).map_err(|_| {
                        async_graphql::InputValueError::custom(
                            "Ethereum address needs exactly 20 bytes",
                        )
                    })
                });

                let external_address = match prepare_bytes_result {
                    Ok(bytes) => Address::from(bytes),
                    Err(error) => return Err(error.propagate()),
                };

                Ok(AirDropId { external_address })
            }
            async_graphql::Value::String(address_string) => {
                let hex_string = address_string.strip_prefix("0x").unwrap_or(&address_string);
                let external_address = Address::from_str(hex_string)?;

                Ok(AirDropId { external_address })
            }
            _ => Err(async_graphql::InputValueError::expected_type(value)),
        }
    }

    fn to_value(&self) -> async_graphql::Value {
        self.external_address
            .to_checksum(Some(ETHEREUM_MAINNET_CHAIN_ID))
            .into()
    }
}

/// An airdrop claim.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct AirDropClaim {
    pub signature: Signature,
    pub destination: Account,
    pub api_token: String,
}

impl AirDropClaim {
    /// Returns the signer's Ethereum [`Address`] for this [`AirDropClaim`].
    pub fn signer_address(
        &self,
        application_id: ApplicationId<ApplicationAbi>,
    ) -> Result<Address, SignatureError> {
        let payload = signature_payload::AirDropClaim::new(application_id, &self.destination);

        let hash = payload.eip712_signing_hash(&AIRDROP_CLAIM_DOMAIN);

        self.signature.recover_address_from_prehash(&hash)
    }
}

#[async_graphql::Scalar]
impl async_graphql::ScalarType for AirDropClaim {
    fn parse(value: async_graphql::Value) -> async_graphql::InputValueResult<Self> {
        let async_graphql::Value::Object(mut fields) = value else {
            return Err(async_graphql::InputValueError::expected_type(value));
        };

        if fields.len() != 2 {
            return Err(async_graphql::InputValueError::custom(
                "`AirDropClaim` object must have exactly three fields: \
                `signature`, `destination` and `apiToken`",
            ));
        }

        let Some(signature_value) = fields.swap_remove("signature") else {
            return Err(async_graphql::InputValueError::custom(
                "`AirDropClaim` object is missing an `signature` field",
            ));
        };

        let async_graphql::Value::String(signature_string) = signature_value else {
            return Err(async_graphql::InputValueError::custom(
                "`AirDropClaim`'s `signature` is not a string",
            ));
        };

        let signature = Signature::from_str(&signature_string).map_err(|_| {
            async_graphql::InputValueError::custom(
                "`AirDropClaim`'s `signature` is not a valid signature string",
            )
        })?;

        let Some(destination_value) = fields.swap_remove("destination") else {
            return Err(async_graphql::InputValueError::custom(
                "`AirDropClaim` object is missing an `destination` field",
            ));
        };

        let destination =
            match <Account as async_graphql::InputType>::parse(Some(destination_value)) {
                Ok(destination) => destination,
                Err(error) => return Err(error.propagate()),
            };

        let Some(api_token_value) = fields.swap_remove("apiToken") else {
            return Err(async_graphql::InputValueError::custom(
                "`AirDropClaim` object is missing an `signature` field",
            ));
        };

        let async_graphql::Value::String(api_token) = api_token_value else {
            return Err(async_graphql::InputValueError::custom(
                "`AirDropClaim`'s `apiToken` is not a string",
            ));
        };

        Ok(AirDropClaim {
            signature,
            destination,
            api_token,
        })
    }

    fn to_value(&self) -> async_graphql::Value {
        let mut fields = IndexMap::new();

        let signature_string = hex::encode(self.signature.as_bytes());
        let signature = async_graphql::ScalarType::to_value(&signature_string);
        let destination = async_graphql::InputType::to_value(&self.destination);

        fields.insert(async_graphql::Name::new("signature"), signature);
        fields.insert(async_graphql::Name::new("destination"), destination);

        async_graphql::Value::Object(fields)
    }
}

/// The [EIP-155] constant for the Ethereum mainnet.
///
/// [EIP-155]: https://eips.ethereum.org/EIPS/eip-155
pub const ETHEREUM_MAINNET_CHAIN_ID: alloy_primitives::aliases::ChainId = 1;
