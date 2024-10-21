// Copyright (c) Zefchain Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};

use airdrop_demo::{
    test_utils::{create_dummy_application_id, sign_claim},
    AirDropClaim,
};
use alloy_primitives::Address;
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

    let sql_query = format!(
        "SELECT COUNT(*) FROM (SELECT * FROM ETHEREUM.NATIVE_WALLETS \
        WHERE WALLET_ADDRESS = '0x{}' AND BALANCE > 0 LIMIT 1);",
        hex::encode(address.as_slice())
    );
    let expected_query = format!(r#"{{ "sqlText": "{sql_query}" }}"#);

    service
        .runtime
        .lock()
        .expect("Test should abort on panic, so mutex should never be poisoned")
        .add_expected_http_request(
            http::Request::post(SXT_GATEWAY_URL, expected_query.as_bytes())
                .with_header("Content-Type", b"application/json")
                .with_header("Authorization", format!("Bearer {api_token}").as_bytes()),
            http::Response::ok(b"[{ \"COUNT(1)\": 1 }]"),
        );

    let json_query = format!(
        "{{ \"query\":
            \"query {{ \
                checkEligibility(address: \\\"{address}\\\", apiToken: \\\"{api_token}\\\") \
            }}\"
        }}"
    );

    let query = serde_json::from_str(&json_query).expect("Failed to deserialize GraphQL query");

    let response = service.handle_query(query).blocking_wait();

    assert_eq!(response.errors.len(), 0);

    let async_graphql::Value::Object(data) = response.data else {
        panic!("Unexpected response data: {response:?}");
    };

    assert_eq!(
        data.len(),
        1,
        "Expected a single item in response data: {data:?}"
    );
    assert_eq!(
        data["checkEligibility"],
        async_graphql::Value::Boolean(true)
    );
}

/// Tests if a GraphQL query can deny an account's eligibility.
#[test]
fn query_returns_address_is_not_eligible() {
    let service = create_service();

    let address = Address::random();
    let api_token = "API token".to_owned();

    let sql_query = format!(
        "SELECT COUNT(*) FROM (SELECT * FROM ETHEREUM.NATIVE_WALLETS \
        WHERE WALLET_ADDRESS = '0x{}' AND BALANCE > 0 LIMIT 1);",
        hex::encode(address.as_slice())
    );
    let expected_query = format!(r#"{{ "sqlText": "{sql_query}" }}"#);

    service
        .runtime
        .lock()
        .expect("Test should abort on panic, so mutex should never be poisoned")
        .add_expected_http_request(
            http::Request::post(SXT_GATEWAY_URL, expected_query.as_bytes())
                .with_header("Content-Type", b"application/json")
                .with_header("Authorization", format!("Bearer {api_token}").as_bytes()),
            http::Response::ok(b"[{ \"COUNT(1)\": 0 }]"),
        );

    let json_query = format!(
        "{{ \"query\":
            \"query {{ \
                checkEligibility(address: \\\"{address}\\\", apiToken: \\\"{api_token}\\\") \
            }}\"
        }}"
    );

    let query = serde_json::from_str(&json_query).expect("Failed to deserialize GraphQL query");

    let response = service.handle_query(query).blocking_wait();

    assert_eq!(response.errors.len(), 0);

    let async_graphql::Value::Object(data) = response.data else {
        panic!("Unexpected response data: {response:?}");
    };
    assert_eq!(
        data.len(),
        1,
        "Expected a single item in response data: {data:?}"
    );
    assert_eq!(
        data["checkEligibility"],
        async_graphql::Value::Boolean(false)
    );
}

/// Tests if a GraphQL query reports query errors.
#[test]
fn query_returns_http_errors() {
    let service = create_service();

    let address = Address::random();
    let api_token = "API token".to_owned();

    let sql_query = format!(
        "SELECT COUNT(*) FROM (SELECT * FROM ETHEREUM.NATIVE_WALLETS \
        WHERE WALLET_ADDRESS = '0x{}' AND BALANCE > 0 LIMIT 1);",
        hex::encode(address.as_slice())
    );
    let expected_query = format!(r#"{{ "sqlText": "{sql_query}" }}"#);

    service
        .runtime
        .lock()
        .expect("Test should abort on panic, so mutex should never be poisoned")
        .add_expected_http_request(
            http::Request::post(SXT_GATEWAY_URL, expected_query.as_bytes())
                .with_header("Content-Type", b"application/json")
                .with_header("Authorization", format!("Bearer {api_token}").as_bytes()),
            http::Response::unauthorized(),
        );

    let json_query = format!(
        "{{ \"query\":
            \"query {{ \
                checkEligibility(address: \\\"{address}\\\", apiToken: \\\"{api_token}\\\") \
            }}\"
        }}"
    );

    let query = serde_json::from_str(&json_query).expect("Failed to deserialize GraphQL query");

    let response = service.handle_query(query).blocking_wait();

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
    let application_id = create_dummy_application_id("zk-airdrop", 1);
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
    let runtime = MockServiceRuntime::new();

    ApplicationService {
        runtime: Arc::new(Mutex::new(runtime)),
    }
}
