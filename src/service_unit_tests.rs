// Copyright (c) Zefchain Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};

use airdrop_demo::{
    test_utils::{create_dummy_application_id, create_dummy_token_id, sign_claim},
    AirDropClaim, Parameters,
};
use alloy_primitives::{Address, U256};
use k256::ecdsa::SigningKey;
use linera_sdk::{
    abis::fungible,
    base::{AccountOwner, ChainId, CryptoHash, Owner},
    bcs, http, serde_json,
    service::MockServiceRuntime,
    util::BlockingWait,
    Service,
};
use rand::rngs::OsRng;

use super::{ApplicationService, SXT_GATEWAY_URL};

/// Tests if a GraphQL query can successfully check if an account is eligible.
#[test]
fn query_returns_address_is_eligible() {
    let service = create_service();

    let address = Address::random();
    let api_token = "API token".to_owned();

    let eligibility_query = prepare_eligibility_query(
        &service,
        &address,
        &api_token,
        http::Response::ok(format!("[{{ \"BALANCE\": \"{MINIMUM_BALANCE}\" }}]").as_bytes()),
    );

    let response = service.handle_query(eligibility_query).blocking_wait();

    assert!(extract_eligibility_from(response));
}

/// Tests if a GraphQL query can deny an account's eligibility if it's balance was below the
/// [`MINIMUM_BALANCE`].
#[test]
fn query_returns_address_with_insufficient_balance_is_not_eligible() {
    let service = create_service();

    let address = Address::random();
    let api_token = "API token".to_owned();

    let insufficient_balance = MINIMUM_BALANCE - 1;

    let eligibility_query = prepare_eligibility_query(
        &service,
        &address,
        &api_token,
        http::Response::ok(format!("[{{ \"BALANCE\": \"{insufficient_balance}\" }}]").as_bytes()),
    );

    let response = service.handle_query(eligibility_query).blocking_wait();

    assert!(!extract_eligibility_from(response));
}

/// Tests if a GraphQL query can deny an account's eligibility if it didn't exist at the snapshot
/// block height.
#[test]
fn query_returns_address_thats_unknown_at_snapshot_is_not_eligible() {
    let service = create_service();

    let address = Address::random();
    let api_token = "API token".to_owned();

    let eligibility_query =
        prepare_eligibility_query(&service, &address, &api_token, http::Response::ok(b"[]"));

    let response = service.handle_query(eligibility_query).blocking_wait();

    assert!(!extract_eligibility_from(response));
}

/// Tests if a GraphQL query reports query errors.
#[test]
fn query_returns_http_errors() {
    let service = create_service();

    let address = Address::random();
    let api_token = "API token".to_owned();

    let eligibility_query = prepare_eligibility_query(
        &service,
        &address,
        &api_token,
        http::Response::unauthorized(),
    );

    let response = service.handle_query(eligibility_query).blocking_wait();

    assert!(matches!(response.data, async_graphql::Value::Null));
    assert_eq!(response.errors.len(), 1);
}

/// Tests if a GraphQL mutation can be used to create an [`AirDropClaim`] operation.
#[test]
fn mutation_generates_air_drop_claim() {
    let service = create_service();

    let chain_id = ChainId(CryptoHash::test_hash("chain ID"));
    let claimer = AccountOwner::User(Owner(CryptoHash::test_hash("claimer")));
    let destination = fungible::Account {
        chain_id,
        owner: claimer,
    };

    let api_token = "API token".to_owned();
    let application_id = create_dummy_application_id("zk-airdrop");
    let signing_key = SigningKey::random(&mut OsRng);
    let signature = sign_claim(&signing_key, application_id, destination);
    let signature_string = hex::encode(signature.as_bytes());

    let json_query = format!(
        "{{ \"query\":
            \"mutation {{ \
                airDropClaim( \
                    signature: \\\"{signature_string}\\\", \
                    destination: {{ \
                        chainId: \\\"{chain_id}\\\", \
                        owner: \\\"{claimer}\\\" \
                    }}, \
                    apiToken: \\\"{api_token}\\\" \
                ) \
            }}\"
        }}"
    );

    let query = serde_json::from_str(&json_query).expect("Failed to deserialize GraphQL claim");

    let response = service.handle_query(query).blocking_wait();

    let async_graphql::Value::Object(response_object) = response.data else {
        panic!("Unexpected response data from query: {response:?}");
    };
    let async_graphql::Value::List(ref claim_bytes) = response_object["airDropClaim"] else {
        panic!("Missing serialized `airDropClaim` in response object");
    };

    let serialized_operation = claim_bytes
        .iter()
        .map(|wrapped_byte| {
            let async_graphql::Value::Number(byte_value) = wrapped_byte else {
                panic!("Serialized `airDropClaim` is not a list of numbers");
            };
            let byte_integer = byte_value
                .as_u64()
                .expect("Serialized `airDropClaim` is not a list of integers");

            u8::try_from(byte_integer).expect("Serialized `airDropClaim` is not a list of bytes")
        })
        .collect::<Vec<u8>>();

    let mut operation = bcs::from_bytes::<AirDropClaim>(&serialized_operation)
        .expect("Failed to deserialize returned operation");

    operation.signature = operation.signature.with_parity_bool();

    let expected_operation = AirDropClaim {
        signature,
        destination: fungible::Account {
            chain_id,
            owner: claimer,
        },
        api_token,
    };

    assert_eq!(operation, expected_operation);
}

/// Creates an [`ApplicationService`] instance.
fn create_service() -> ApplicationService {
    let runtime = MockServiceRuntime::new().with_application_parameters(Parameters {
        token_id: create_dummy_token_id(),
        snapshot_block: 100,
        minimum_balance: U256::from(MINIMUM_BALANCE),
    });

    ApplicationService {
        runtime: Arc::new(Mutex::new(runtime)),
    }
}

/// The minimum balance to be eligible for an airdrop in the tests.
const MINIMUM_BALANCE: usize = 10;

/// Prepares an [`async_graphql::Request`] to the service to `checkEligibility` of an [`Address`].
///
/// Configures the `service`'s mock runtime to return the expected `query_response` when the HTTP
/// query is made.
fn prepare_eligibility_query(
    service: &ApplicationService,
    address: &Address,
    api_token: &str,
    query_response: http::Response,
) -> async_graphql::Request {
    let mut runtime = service
        .runtime
        .lock()
        .expect("Test should abort on panic, so mutex should never be poisoned");

    let snapshot_block = runtime.application_parameters().snapshot_block;
    let sql_query = format!(
        "SELECT BALANCE FROM ETHEREUM.NATIVE_WALLETS \
        WHERE WALLET_ADDRESS = '0x{}' AND BLOCK_NUMBER <= {snapshot_block} \
        ORDER BY BLOCK_NUMBER DESC \
        LIMIT 1;",
        hex::encode(address.as_slice())
    );
    let expected_query = format!(r#"{{ "sqlText": "{sql_query}" }}"#);

    runtime.add_expected_http_request(
        http::Request::post(SXT_GATEWAY_URL, expected_query.as_bytes())
            .with_header("Content-Type", b"application/json")
            .with_header("Authorization", format!("Bearer {api_token}").as_bytes()),
        query_response,
    );

    let json_query = format!(
        "{{ \"query\":
            \"query {{ \
                checkEligibility(address: \\\"{address}\\\", apiToken: \\\"{api_token}\\\") \
            }}\"
        }}"
    );

    serde_json::from_str(&json_query).expect("Failed to deserialize GraphQL query")
}

/// Parses the [`async_graphql::Response`] of `checkEligibility` to extract the `true` or `false`
/// value that indicates the eligibility.
fn extract_eligibility_from(response: async_graphql::Response) -> bool {
    assert_eq!(
        response.errors.len(),
        0,
        "Errors reported from service: {:?}",
        response.errors
    );

    let async_graphql::Value::Object(data) = response.data else {
        panic!("Unexpected response data: {response:?}");
    };

    assert_eq!(
        data.len(),
        1,
        "Expected a single item in response data: {data:?}"
    );

    let async_graphql::Value::Boolean(is_eligible) = data["checkEligibility"] else {
        panic!("Unexpected `checkEligibility` result: {data:?}");
    };

    is_eligible
}
