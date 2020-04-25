use snarkos_dpc::{
    base_dpc::{instantiated::*, record::DPCRecord, record_payload::PaymentRecordPayload, BaseDPCComponents},
    test_data::*,
    DPCScheme,
};
use snarkos_dpc_consensus::{get_block_reward, test_data::*, ConsensusParameters};
use snarkos_dpc_storage::BlockStorage;
use snarkos_models::dpc::Record;
use snarkos_objects::{dpc::DPCTransactions, ledger::Ledger};
use snarkos_utilities::{bytes::ToBytes, to_bytes};

use rand::thread_rng;

#[test]
fn base_dpc_multiple_transactions() {
    let mut rng = thread_rng();

    let consensus = ConsensusParameters {
        max_block_size: 1_000_000_000usize,
        max_nonce: u32::max_value(),
        target_block_time: 10i64,
    };

    // Generate or load parameters for the ledger, commitment schemes, and CRH
    let (ledger_parameters, parameters) = setup_or_load_parameters(&mut rng);

    // Generate addresses
    let [genesis_address, miner_address, recipient] = generate_test_addresses(&parameters, &mut rng);

    // Setup the ledger
    let (ledger, genesis_pred_vk_bytes) = setup_ledger(&parameters, ledger_parameters, &genesis_address, &mut rng);

    let new_predicate = Predicate::new(genesis_pred_vk_bytes.clone());
    let new_birth_predicates = vec![new_predicate.clone(); NUM_OUTPUT_RECORDS];
    let new_death_predicates = vec![new_predicate.clone(); NUM_OUTPUT_RECORDS];

    let mut transactions = DPCTransactions::<Tx>::new();

    println!("Creating block with coinbase transaction");

    let (coinbase_records, block) = create_block_with_coinbase_transaction(
        &mut transactions,
        &parameters,
        &genesis_pred_vk_bytes,
        new_birth_predicates.clone(),
        new_death_predicates.clone(),
        genesis_address.clone(),
        miner_address.public_key.clone(),
        &consensus,
        &ledger,
        &mut rng,
    );

    assert!(InstantiatedDPC::verify(&parameters, &block.transactions[0], &ledger).unwrap());

    let block_reward = get_block_reward(ledger.len() as u32);

    assert_eq!(coinbase_records.len(), 2);
    assert!(!coinbase_records[0].is_dummy());
    assert!(coinbase_records[1].is_dummy());
    assert_eq!(coinbase_records[0].payload().balance, block_reward);
    assert_eq!(coinbase_records[1].payload().balance, 0);

    println!("Verifying the block");

    assert!(consensus.verify_block(&parameters, &block, &ledger).unwrap());

    assert!(InstantiatedDPC::verify(&parameters, &block.transactions[0], &ledger).unwrap());

    ledger.insert_block(block).unwrap();
    assert_eq!(ledger.len(), 2);

    // Add new block spending records from the previous block

    let spend_asks = vec![miner_address.secret_key.clone(); NUM_INPUT_RECORDS];
    let newer_apks = vec![recipient.public_key.clone(); NUM_OUTPUT_RECORDS];

    let new_dummy_flags = vec![false; NUM_OUTPUT_RECORDS];
    let new_payload = PaymentRecordPayload { balance: 10, lock: 0 };

    let new_payloads = vec![new_payload.clone(); NUM_OUTPUT_RECORDS];

    let auxiliary = [5u8; 32];
    let memo = [6u8; 32];

    let mut transactions = DPCTransactions::new();

    println!("Create a payment transaction transaction");

    let (spend_records, transaction) = ConsensusParameters::create_transaction(
        &parameters,
        coinbase_records,
        spend_asks,
        newer_apks,
        new_birth_predicates.clone(),
        new_death_predicates.clone(),
        new_dummy_flags,
        new_payloads,
        auxiliary,
        memo,
        &ledger,
        &mut rng,
    )
    .unwrap();

    assert_eq!(spend_records.len(), 2);
    assert!(!spend_records[0].is_dummy());
    assert!(!spend_records[1].is_dummy());
    assert_eq!(spend_records[0].payload().balance, 10);
    assert_eq!(spend_records[1].payload().balance, 10);
    assert_eq!(transaction.stuff.value_balance, (block_reward - 20) as i64);

    assert!(InstantiatedDPC::verify(&parameters, &transaction, &ledger).unwrap());

    transactions.push(transaction);

    println!("Create a new block with the payment transaction");

    let (new_coinbase_records, new_block) = create_block_with_coinbase_transaction(
        &mut transactions,
        &parameters,
        &genesis_pred_vk_bytes,
        new_birth_predicates,
        new_death_predicates,
        genesis_address,
        recipient.public_key,
        &consensus,
        &ledger,
        &mut rng,
    );

    let new_block_reward = get_block_reward(ledger.len() as u32);

    assert_eq!(new_coinbase_records.len(), 2);
    assert!(!new_coinbase_records[0].is_dummy());
    assert!(new_coinbase_records[1].is_dummy());
    assert_eq!(
        new_coinbase_records[0].payload().balance,
        new_block_reward + block_reward - 20
    );
    assert_eq!(new_coinbase_records[1].payload().balance, 0);

    println!("Verify the block with the new payment transaction");

    assert!(consensus.verify_block(&parameters, &new_block, &ledger).unwrap());

    ledger.insert_block(new_block).unwrap();
    assert_eq!(ledger.len(), 3);

    for record in &new_coinbase_records {
        ledger.store_record(record).unwrap();

        let reconstruct_record: Option<DPCRecord<Components>> = ledger
            .get_record(&to_bytes![record.commitment()].unwrap().to_vec())
            .unwrap();

        assert_eq!(
            to_bytes![reconstruct_record.unwrap()].unwrap(),
            to_bytes![record].unwrap()
        );
    }

    let path = ledger.storage.storage.path().to_owned();
    drop(ledger);
    BlockStorage::<Tx, <Components as BaseDPCComponents>::MerkleParameters>::destroy_storage(path).unwrap();
}
