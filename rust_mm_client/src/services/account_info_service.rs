use{
    std::{
        sync::Arc,
        time::Duration,
    },
    solana_client::{
        nonblocking::rpc_client::RpcClient,
        client_error::ClientError,
    },
    solana_sdk::{
        pubkey::Pubkey,
        commitment_config::CommitmentConfig
    },
    tokio::{
        task,
        time::sleep
    },
    log::{info, warn},
    crate::accounts_cache::{AccountsCache, AccountState}
};

pub struct AccountInfoService {
    cache: Arc<AccountsCache>,
    client: Arc<RpcClient>,
    keys: Vec<Pubkey>,
}

impl AccountInfoService {
    pub fn default() -> Self {
        Self {
            cache: Arc::new(AccountsCache::default()), 
            client: Arc::new(RpcClient::new("http://localhost:8899".to_string())),
            keys: Vec::new()
        }
    }

    pub fn new(
        cache: Arc<AccountsCache>,
        client: Arc<RpcClient>,
        keys: &[Pubkey],
    ) -> AccountInfoService {
        AccountInfoService {
            cache,
            client,
            keys: Vec::from(keys),
        }
    }

    pub async fn start_service(
        self: &Arc<Self>
    ) -> Result<(), task::JoinError> {
        let rpc_cloned_self = self.clone();

        for i in (0..self.keys.len()).step_by(100) {
            rpc_cloned_self.update_infos(
                i, self.keys.len().min(i+100)
            ).await.unwrap();
        }

        let t = task::spawn(rpc_cloned_self.update_infos_replay()).await;

        if let Err(e) = t {
            warn!("[AIS] Error attempting to join with task: {}", e.to_string());
            return Err(e);
        };

        Ok(())
    }

    #[inline(always)]
    async fn update_infos(
        self: &Arc<Self>,
        from: usize,
        to: usize
    ) -> Result<(), ClientError> {
            let account_keys = &self.keys[from..to];
            let rpc_result = self.client.get_multiple_accounts_with_commitment(
                account_keys,
                CommitmentConfig::confirmed()
            ).await;
            
            let res = match rpc_result {
                Ok(r) => r,
                Err(e) => {
                    warn!("[AIS] Could not fetch account infos: {}", e.to_string());
                    return Err(e);
                }
            };

            let mut infos = res.value;
            info!("[AIS] Fetched {} account infos.", infos.len()); 

            while !infos.is_empty() {
                let next = infos.pop().unwrap();
                let i = infos.len();
                let key = account_keys[i];
                //info!("[AIS] [{}/{}] Fetched account {}", i, infos.len(), key.to_string()); 

                let info = match next {
                    Some(ai) => ai,
                    None => {
                        warn!("[AIS] [{}/{}] An account info was missing!!", i, infos.len());
                        continue;
                    }
                };
                //info!("[AIS] [{}/{}] Account {} has data: {}", i, infos.len(), key.to_string(), base64::encode(&info.data));

                let res = self.cache.insert(key, AccountState{
                    account: info,
                    slot: res.context.slot
                });

                match res {
                    Ok(_) => (),
                    Err(_) => {
                        warn!("[AIS] There was an error while inserting account info in the cache.");
                    }
                };
            }

            Ok(())  
    }

    #[inline(always)]
    async fn update_infos_replay(
        self: Arc<Self>,
    ) {
        loop {
            let aself = self.clone();

            for i in (0..self.keys.len()).step_by(100) {
                let res = aself.update_infos(
                    i, self.keys.len().min(i+100)
                ).await;

                if res.is_err() {
                    warn!("[AIS] Failed to update account infos: {}", res.err().unwrap().to_string());
                }
            }
            
            sleep(Duration::from_millis(1000)).await;
        }
    }
}