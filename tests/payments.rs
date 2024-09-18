#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;

use airdrop_demo::{AirDropClaim, AirDropId, ApplicationAbi, Parameters};
use async_graphql::InputType;
use linera_sdk::{
    abis::fungible::{self, FungibleTokenAbi},
    base::{AccountOwner, Amount, ApplicationId},
    test::{ActiveChain, TestValidator},
};

/// Tests if a valid [`AirDropClaim`] is properly paid.
#[tokio::test]
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

    claimer_chain.register_application(application_id).await;

    let claim_certificate = claimer_chain
        .add_block(|block| {
            block.with_operation(
                application_id,
                AirDropClaim {
                    id: AirDropId::from(b"airdrop"),
                    destination: claimer_account,
                },
            );
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
            Parameters { token_id },
            (),
            vec![token_id.forget_abi()],
        )
        .await;

    let airdrop_account = fungible::Account {
        chain_id: airdrop_chain.id(),
        owner: AccountOwner::Application(application_id.forget_abi()),
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
