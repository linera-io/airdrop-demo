#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;

use airdrop_demo::{test_utils::sign_claim, AirDropClaim, ApplicationAbi, Parameters};
use alloy_primitives::U256;
use async_graphql::InputType;
use k256::ecdsa::SigningKey;
use linera_sdk::{
    abis::fungible::{self, FungibleTokenAbi},
    linera_base_types::{AccountOwner, Amount, ApplicationId},
    test::{ActiveChain, TestValidator},
};
use rand::{rngs::StdRng, SeedableRng};

/// Tests if a valid [`AirDropClaim`] is properly paid.
#[tokio::test]
#[ignore = "Requires real network access"]
async fn pays_valid_claim() {
    let initial_tokens = Amount::from_tokens(100);
    let (validator, airdrop_chain, airdrop_account, token_id, application_id) =
        setup(initial_tokens).await;

    let claimer_chain = validator.new_chain().await;
    let claim_amount = Amount::ONE;
    let claimer_account = fungible::Account {
        chain_id: claimer_chain.id(),
        owner: AccountOwner::from(claimer_chain.public_key()),
    };

    let claim = prepare_airdrop_claim(application_id, 0, claimer_account);

    let claim_certificate = claimer_chain
        .add_block(|block| {
            block.with_operation(application_id, claim);
        })
        .await;

    assert_eq!(claim_certificate.outgoing_message_count(), 2);

    let payment_certificate = airdrop_chain
        .add_block(|block| {
            block.with_messages_from(&claim_certificate);
        })
        .await;

    assert_eq!(payment_certificate.outgoing_message_count(), 2);

    let receipt_certificate = claimer_chain
        .add_block(|block| {
            block.with_messages_from(&payment_certificate);
        })
        .await;

    assert_eq!(receipt_certificate.outgoing_message_count(), 0);

    let airdrop_balance = query_balance(token_id, &airdrop_chain, airdrop_account.owner).await;
    let claimer_balance = query_balance(token_id, &claimer_chain, claimer_account.owner).await;

    assert_eq!(claimer_balance, Some(claim_amount));
    assert_eq!(airdrop_balance, Some(initial_tokens - claim_amount));
}

/// Tests if multiple valid [`AirDropClaim`]s are properly paid.
#[tokio::test]
#[ignore = "Requires real network access"]
async fn pays_multiple_claims() {
    let initial_tokens = Amount::from_tokens(10);
    let (validator, airdrop_chain, airdrop_account, token_id, application_id) =
        setup(initial_tokens).await;

    for claim_index in 1..=10 {
        let claimer_chain = validator.new_chain().await;
        let claim_amount = Amount::ONE;
        let claimer_account = fungible::Account {
            chain_id: claimer_chain.id(),
            owner: AccountOwner::from(claimer_chain.public_key()),
        };

        let claim = prepare_airdrop_claim(application_id, claim_index, claimer_account);

        let claim_certificate = claimer_chain
            .add_block(|block| {
                block.with_operation(application_id, claim);
            })
            .await;

        assert_eq!(claim_certificate.outgoing_message_count(), 2);

        let payment_certificate = airdrop_chain
            .add_block(|block| {
                block.with_messages_from(&claim_certificate);
            })
            .await;

        assert_eq!(payment_certificate.outgoing_message_count(), 2);

        let receipt_certificate = claimer_chain
            .add_block(|block| {
                block.with_messages_from(&payment_certificate);
            })
            .await;

        assert_eq!(receipt_certificate.outgoing_message_count(), 0);

        let airdrop_balance = query_balance(token_id, &airdrop_chain, airdrop_account.owner).await;
        let claimer_balance = query_balance(token_id, &claimer_chain, claimer_account.owner).await;

        assert_eq!(claimer_balance, Some(claim_amount));
        assert_eq!(
            airdrop_balance.unwrap_or(Amount::ZERO),
            initial_tokens - claim_amount * claim_index.into()
        );
    }
}

/// Tests if an attempt to replay a claim in the same block is rejected.
#[tokio::test]
#[ignore = "Requires real network access"]
#[should_panic]
async fn rejects_replay_attacks_in_the_same_block() {
    let initial_tokens = Amount::from_tokens(100);
    let (validator, airdrop_chain, _airdrop_account, _token_id, application_id) =
        setup(initial_tokens).await;

    let claimer_chain = validator.new_chain().await;
    let claimer_account = fungible::Account {
        chain_id: claimer_chain.id(),
        owner: AccountOwner::from(claimer_chain.public_key()),
    };

    let claim = prepare_airdrop_claim(application_id, 0, claimer_account);

    claimer_chain
        .add_block(|block| {
            block
                .with_operation(application_id, claim.clone())
                .with_operation(application_id, claim);
        })
        .await;
    airdrop_chain.handle_received_messages().await;
}

/// Tests if an attempt to replay a claim in the same chain is rejected.
#[tokio::test]
#[ignore = "Requires real network access"]
#[should_panic]
async fn rejects_replay_attacks_in_the_same_chain() {
    let initial_tokens = Amount::from_tokens(100);
    let (validator, airdrop_chain, _airdrop_account, _token_id, application_id) =
        setup(initial_tokens).await;

    let claimer_chain = validator.new_chain().await;
    let claimer_account = fungible::Account {
        chain_id: claimer_chain.id(),
        owner: AccountOwner::from(claimer_chain.public_key()),
    };

    let claim = prepare_airdrop_claim(application_id, 0, claimer_account);

    claimer_chain
        .add_block(|block| {
            block.with_operation(application_id, claim.clone());
        })
        .await;
    airdrop_chain.handle_received_messages().await;

    claimer_chain
        .add_block(|block| {
            block.with_operation(application_id, claim);
        })
        .await;
    airdrop_chain.handle_received_messages().await;
}

/// Tests if an attempt to replain a claim in a different chain is rejected.
#[tokio::test]
#[ignore = "Requires real network access"]
#[should_panic]
async fn rejects_replay_attacks_in_different_chains() {
    let initial_tokens = Amount::from_tokens(100);
    let (validator, airdrop_chain, _airdrop_account, _token_id, application_id) =
        setup(initial_tokens).await;

    let claimer_chain = validator.new_chain().await;
    let claimer_account = fungible::Account {
        chain_id: claimer_chain.id(),
        owner: AccountOwner::from(claimer_chain.public_key()),
    };

    let claim = prepare_airdrop_claim(application_id, 0, claimer_account);

    claimer_chain
        .add_block(|block| {
            block.with_operation(application_id, claim.clone());
        })
        .await;
    airdrop_chain.handle_received_messages().await;

    let attacker_chain = validator.new_chain().await;

    attacker_chain
        .add_block(|block| {
            block.with_operation(application_id, claim);
        })
        .await;
    airdrop_chain.handle_received_messages().await;
}

/// Tests if airdrop claims are rejected when the airdrop account is empty.
#[tokio::test]
#[ignore = "Requires real network access"]
#[should_panic]
async fn payment_fails_if_airdrop_account_is_empty() {
    let (validator, airdrop_chain, _airdrop_account, _token_id, application_id) =
        setup(Amount::ONE).await;

    let claimer_chain = validator.new_chain().await;
    let claimer_account = fungible::Account {
        chain_id: claimer_chain.id(),
        owner: AccountOwner::from(claimer_chain.public_key()),
    };

    let first_claim = prepare_airdrop_claim(application_id, 1, claimer_account);

    claimer_chain
        .add_block(|block| {
            block.with_operation(application_id, first_claim);
        })
        .await;
    airdrop_chain.handle_received_messages().await;

    let late_claimer_chain = validator.new_chain().await;
    let late_claimer_account = fungible::Account {
        chain_id: late_claimer_chain.id(),
        owner: AccountOwner::from(late_claimer_chain.public_key()),
    };

    let late_claim = prepare_airdrop_claim(application_id, 2, late_claimer_account);

    late_claimer_chain
        .add_block(|block| {
            block.with_operation(application_id, late_claim);
        })
        .await;
    airdrop_chain.handle_received_messages().await;
}

/// Configures the test environment, deploying the airdrop application with some newly minted
/// tokens.
async fn setup(
    initial_tokens: Amount,
) -> (
    TestValidator,
    ActiveChain,
    fungible::Account,
    ApplicationId<FungibleTokenAbi>,
    ApplicationId<ApplicationAbi>,
) {
    let (validator, bytecode_id) =
        TestValidator::with_current_bytecode::<ApplicationAbi, Parameters, ()>().await;

    let mut airdrop_chain = validator.new_chain().await;
    let initial_token_owner = AccountOwner::from(airdrop_chain.public_key());

    let fungible_bytecode_id = airdrop_chain
        .publish_bytecodes_in("vendor/linera-protocol/examples/fungible")
        .await;
    let token_id = airdrop_chain
        .create_application(
            fungible_bytecode_id,
            fungible::Parameters {
                ticker_symbol: "TOK".to_owned(),
            },
            fungible::InitialState {
                accounts: BTreeMap::from([(initial_token_owner, initial_tokens)]),
            },
            vec![],
        )
        .await;

    let application_id = airdrop_chain
        .create_application(
            bytecode_id,
            Parameters {
                token_id,
                snapshot_block: 250,
                minimum_balance: U256::from(25),
            },
            (),
            vec![token_id.forget_abi()],
        )
        .await;

    let airdrop_account = fungible::Account {
        chain_id: airdrop_chain.id(),
        owner: AccountOwner::from(application_id),
    };

    airdrop_chain
        .add_block(|block| {
            block.with_operation(
                token_id,
                fungible::Operation::Transfer {
                    owner: initial_token_owner,
                    amount: initial_tokens,
                    target_account: airdrop_account,
                },
            );
        })
        .await;

    (
        validator,
        airdrop_chain,
        airdrop_account,
        token_id,
        application_id,
    )
}

/// Creates an [`AirDropClaim`] for the test.
fn prepare_airdrop_claim(
    application_id: ApplicationId<ApplicationAbi>,
    seed_data: u64,
    destination: fungible::Account,
) -> AirDropClaim {
    let signing_key = SigningKey::random(&mut StdRng::seed_from_u64(seed_data));
    let signature = sign_claim(&signing_key, application_id, destination);

    AirDropClaim {
        signature,
        destination,
        api_token: "API token".to_owned(),
    }
}

/// Queries the token balance of an `owner` on a `chain`.
async fn query_balance(
    token_id: ApplicationId<FungibleTokenAbi>,
    chain: &ActiveChain,
    owner: AccountOwner,
) -> Option<Amount> {
    let owner = owner.to_value();
    let query = format!("query {{ accounts {{ entry(key: {owner}) {{ value }} }} }}");

    let response = chain.graphql_query(token_id, query).await;

    let balance = response.pointer("/accounts/entry/value")?.as_str()?;

    Some(
        balance
            .parse()
            .expect("Failed to parse account balance amount"),
    )
}
