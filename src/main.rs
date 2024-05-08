pub mod bindings;

use std::{env, time::Duration};

use dotenv::dotenv;
use ecdsa::SigningKey;
use k256::SecretKey as K256SecretKey;
use std::sync::Arc;

use crate::bindings::lock::Lock;
use ethers::{
    middleware::{NonceManagerMiddleware, SignerMiddleware},
    providers::Middleware,
    providers::{Http, Provider},
    signers::{LocalWallet, Signer, Wallet},
    types::Address,
    types::{BlockId, BlockNumber},
};

use ethers::core::k256::ecdsa::SigningKey as ImpSigningKey;

type Provider0 = Provider<Http>;
type Provider1 = SignerMiddleware<Provider0, Wallet<ImpSigningKey>>;
type Provider2 = NonceManagerMiddleware<Provider1>;
pub type SignerProvider = Provider2;
pub type SignerWithoutNonceProvider = Provider1;

const COUNTER_CLIENT: &str = "counter_client_context";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_log();
    dotenv().ok();
    let private_key = env::var("PRIVATE_KEY".to_string())?;
    let l2_rpc_url = env::var("L2_RPC_URL".to_string())?;
    let contract_addr = env::var("CONCTRACT_ADDR".to_string())?;

    let signing_key = SigningKey::from_slice(&hex::decode(remove_0x_prefix(&private_key))?)?;
    let evm_operator_private_key = K256SecretKey::from(signing_key);

    let evm_provider = connect_evm_rpc(&l2_rpc_url);
    let l2_chain_id = evm_provider.get_chainid().await.unwrap().as_u64();
    let signer_provider = Arc::new(get_signer_provider(
        evm_provider.clone(),
        l2_chain_id,
        evm_operator_private_key.clone(),
    ));

    let block = evm_provider
        .get_block(BlockId::Number(BlockNumber::Latest))
        .await?;
    let base_fee = block.unwrap().base_fee_per_gas.unwrap().as_u128();
    let max_fee = base_fee * 2;

    let contract_addr: Address = contract_addr.parse()?;

    let contract = Lock::new(contract_addr, signer_provider);
    let origin_counter = contract.counter().call().await?;
    tracing::info!(target: COUNTER_CLIENT, "counter start value: {}", origin_counter);

    for i in 0..10 {
        match contract
            .inc()
            .gas(3_000_000)
            .gas_price(max_fee)
            .send()
            .await
        {
            Ok(tx) => {
                tracing::info!(target: COUNTER_CLIENT, "idx:{} tx info: {:?}", i, tx);
            }
            Err(err) => {
                tracing::warn!(target: COUNTER_CLIENT, "idx:{} tx failed with err: {}", i, err);
            }
        }
    }

    tokio::time::sleep(Duration::from_secs(10)).await;
    let origin_counter = contract.counter().call().await?;
    tracing::info!(target: COUNTER_CLIENT, "counter finish value: {}", origin_counter);

    Ok(())
}

pub fn remove_0x_prefix(input: &str) -> String {
    if input.starts_with("0x") || input.starts_with("0X") {
        input[2..].to_string()
    } else {
        input.to_string()
    }
}

pub fn get_signer_provider(
    http_provider: Provider<Http>,
    chain_id: u64,
    private_key: K256SecretKey,
) -> SignerProvider {
    let wallet: LocalWallet = private_key.into();
    let wallet = wallet.with_chain_id(chain_id);
    let signer_provider = SignerMiddleware::new(http_provider, wallet.clone());
    NonceManagerMiddleware::new(signer_provider, wallet.address())
}

pub fn connect_evm_rpc(rpc: &str) -> Provider<Http> {
    Provider::<Http>::try_from(rpc).expect(format!("Failed to connect to {}", rpc).as_str())
}

pub(crate) fn init_log() {
    tracing_subscriber::fmt::Subscriber::builder()
        .with_writer(std::io::stderr)
        // .with_max_level(tracing_subscriber::filter::LevelFilter::DEBUG)
        .with_thread_ids(true)
        .with_thread_names(true)
        .init();
}
