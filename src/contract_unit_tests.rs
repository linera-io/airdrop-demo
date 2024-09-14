// Copyright (c) Zefchain Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use airdrop_demo::{AirDropClaim, AirDropId, Parameters};
use linera_sdk::{
    abis::fungible::Account,
    base::{
        AccountOwner, Amount, ApplicationId, BlockHeight, BytecodeId, ChainId, CryptoHash,
        Destination, MessageId, Owner,
    },
    util::BlockingWait,
    views::View,
    Contract, ContractRuntime, Resources, SendMessageRequest,
};

use super::{state::Application, ApplicationContract, ApprovedAirDrop};

/// Tests if a valid airdrop claim is accepted and results in a message to execute the payment.
#[test]
fn accepts_new_claim() {
    let mut contract = create_and_instantiate_contract();
    let airdrop_id = AirDropId::from(b"airdrop");
    let destination_account = create_dummy_destination();

    let claim = AirDropClaim {
        id: airdrop_id.clone(),
        destination: destination_account,
    };

    let () = contract.execute_operation(claim).blocking_wait();

    let application_creator_chain_id = contract.runtime.application_creator_chain_id();
    let scheduled_messages = contract.runtime.created_send_message_requests();

    let expected_message = SendMessageRequest {
        destination: Destination::Recipient(application_creator_chain_id),
        authenticated: true,
        is_tracked: false,
        grant: Resources::default(),
        message: ApprovedAirDrop {
            id: airdrop_id,
            amount: Amount::ONE,
            destination: destination_account,
        },
    };

    assert_eq!(*scheduled_messages, vec![expected_message]);
}

/// Creates an [`ApplicationContract`] instance and calls `instantiate` on it.
fn create_and_instantiate_contract() -> ApplicationContract {
    let runtime = ContractRuntime::new()
        .with_application_parameters(Parameters {
            token_id: create_dummy_token_id(),
        })
        .with_application_id(create_dummy_application_id("zk-airdrop", 1))
        .with_application_creator_chain_id(ChainId(CryptoHash::test_hash("creator chain")));

    let mut contract = ApplicationContract {
        state: Application::load(runtime.root_view_storage_context())
            .blocking_wait()
            .expect("Failed to read from mock key value store"),
        runtime,
    };

    contract.instantiate(()).blocking_wait();

    contract
}

/// Creates a dummy [`ApplicationId`] to use as the Fungible Token for testing.
fn create_dummy_token_id<Abi>() -> ApplicationId<Abi> {
    create_dummy_application_id("fungible token", 0)
}

/// Creates a dummy [`ApplicationId`] to use for testing.
fn create_dummy_application_id<Abi>(name: &str, index: u32) -> ApplicationId<Abi> {
    let bytecode_id = BytecodeId::new(
        CryptoHash::test_hash(format!("{name} contract")),
        CryptoHash::test_hash(format!("{name} service")),
    );

    let creation = MessageId {
        chain_id: ChainId(CryptoHash::test_hash("chain")),
        height: BlockHeight::ZERO,
        index,
    };

    ApplicationId {
        bytecode_id,
        creation,
    }
    .with_abi()
}

/// Creates a dummy [`Account`] to use as a test destination for the airdropped tokens.
fn create_dummy_destination() -> Account {
    Account {
        chain_id: ChainId(CryptoHash::test_hash("destination chain")),
        owner: AccountOwner::User(Owner(CryptoHash::test_hash("destination owner"))),
    }
}
