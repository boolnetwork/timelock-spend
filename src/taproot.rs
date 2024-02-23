use std::str::FromStr;
use bitcoin::secp256k1::XOnlyPublicKey;

use bitcoin::hashes::Hash;
use bitcoin::key::{Keypair, TapTweak, TweakedKeypair, UntweakedPublicKey};
use bitcoin::locktime::absolute;
use bitcoin::secp256k1::{rand, Message, Secp256k1, SecretKey, Signing, Verification};
use bitcoin::sighash::{Prevouts, SighashCache, TapSighashType};
use bitcoin::{transaction, Address, Amount, Network, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid, Witness, script, TapNodeHash, PublicKey};
use bitcoin::consensus::encode;
use bitcoin::opcodes::all::{OP_CHECKSIG, OP_CLTV, OP_DROP};
use bitcoin::taproot::{ControlBlock, LeafVersion, TaprootBuilder, TaprootSpendInfo};
use bitcoin::TapLeafHash;
use crate::utxo::Utxo;

pub fn create_taproot_info(time: u32, secret: &[u8], publickey_of_comitee: &str) -> (TaprootSpendInfo, ScriptBuf){
    let secp = Secp256k1::new();

    let keypair = Keypair::from_seckey_slice(&secp, secret).unwrap();

    // 这个 secret 用来解锁 taproot中的叶子，同时他也是收益人。
    let (internal_key, _parity) = keypair.x_only_public_key();

    let reveal_script  = script::Builder::new()
        .push_int(time as i64)
        .push_opcode(OP_CLTV)
        .push_opcode(OP_DROP)
        .push_x_only_key(&internal_key)
        .push_opcode(OP_CHECKSIG)
        .into_script();

    let internal_key_of_commitee = XOnlyPublicKey::from_str(publickey_of_comitee).unwrap();

    let taproot_spend_info = TaprootBuilder::new()
        .add_leaf(0, reveal_script.clone())
        .expect("adding leaf should work")
        .finalize(&secp, internal_key_of_commitee)
        .expect("finalizing taproot builder should work");

    (taproot_spend_info,reveal_script)
}

fn receivers_script_pubkey(receiver: &str, amount: f64, network: Network) -> TxOut {
    let address = Address::from_str(receiver)
        .expect("a valid address")
        .require_network(network)
        .expect("valid address for mainnet");
    address.script_pubkey();
    let spend = TxOut { value: Amount::from_btc(amount).unwrap(), script_pubkey: address.script_pubkey() };
    spend
}

pub fn convert_all_inputs_to_sighashs(time: u32, utxos: Vec<Utxo>,
                                      receiver: &str,  secret: &[u8], publickey_of_comitee: &str,
                                      network: Network){
    let secp = Secp256k1::new();
    let keypair = Keypair::from_seckey_slice(&secp, secret).unwrap();

    let (taproot_spend_info,reveal_script)
        = create_taproot_info(time, secret, publickey_of_comitee);

    let script_pubkey = ScriptBuf::new_p2tr(
        &secp,
        taproot_spend_info.internal_key(), // 这个和上面的internal_key是一样的
        taproot_spend_info.merkle_root(),
    );
    let mut inputs = Vec::new();
    let mut prevouts = Vec::new();
    let mut amount_btc = 0f64;

    for utxo in &utxos {
        amount_btc += utxo.amount;
        let out_point = OutPoint {
            txid: Txid::from_str(&utxo.txid).unwrap(), // Obviously invalid.
            vout: utxo.vout as u32,
        };
        let utxo = TxOut { value: Amount::from_btc(utxo.amount).unwrap(),
            script_pubkey: script_pubkey.clone() };

        // The input for the transaction we are constructing.
        let input = TxIn {
            previous_output: out_point, // The dummy output we are spending.
            script_sig: ScriptBuf::default(), // For a p2tr script_sig is empty.
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: Witness::from(vec![vec![88u8;14];10]), // to calculate fee
        };
        inputs.push(input);
        prevouts.push(utxo);
    }

    let mut spend = receivers_script_pubkey(receiver , amount_btc, network);
    let mut unsigned_tx = Transaction {
        version: transaction::Version::TWO,  // Post BIP-68.
        lock_time: absolute::LockTime::from_height(time + 1).unwrap(), // Ignore the locktime.
        input: inputs.clone(),                  // Input goes into index 0.
        output: vec![spend.clone()],         // Outputs, order does not matter.
    };
    let fee = calculate_fee(unsigned_tx.vsize(), 0.00001, 1.0);
    spend.value = Amount::from_btc(amount_btc - fee as f64/100_000_000f64).unwrap();
    println!("{} {}",amount_btc,fee);


    let mut unsigned_tx = Transaction {
        version: transaction::Version::TWO,  // Post BIP-68.
        lock_time: absolute::LockTime::from_height(time + 1).unwrap(), // Ignore the locktime.
        input: inputs,                  // Input goes into index 0.
        output: vec![spend.clone()],         // Outputs, order does not matter.
    };

    let mut sighasher = SighashCache::new(&mut unsigned_tx);


    let control_block = taproot_spend_info
        .control_block(&(reveal_script.clone(), LeafVersion::TapScript))
        .expect("should compute control block");

    for i in 0..utxos.len(){
        let sighash = sighasher
            .taproot_script_spend_signature_hash(
                i,
                &Prevouts::All(&prevouts),
                TapLeafHash::from_script(&reveal_script, LeafVersion::TapScript),
                TapSighashType::Default,
            )
            .expect("failed to construct sighash");

        let signature = secp.sign_schnorr(&Message::from(sighash), &keypair);
        println!("signateure {}",signature);

        let witness = sighasher
            .witness_mut(i)
            .expect("getting mutable witness reference should work");
        witness.clear();
        witness.push(
            bitcoin::taproot::Signature { sig: signature, hash_ty: TapSighashType::Default, }.to_vec(),
        );
        witness.push(reveal_script.clone());
        witness.push(&control_block.serialize());
    }
    let tx = sighasher.into_transaction();
    println!("tx {:?}",tx);
}

pub fn calculate_fee(virtual_size: usize, rate: f64, multiplier: f64) -> u64 {
    let kilo_bytes = virtual_size as f64 / 1000_f64;
    let rate = bitcoin::Amount::from_btc(rate).unwrap().to_sat() as f64;
    ((kilo_bytes * rate) * multiplier).round() as u64
}