use std::{
    collections::{BinaryHeap, HashMap},
    sync::Arc, fmt::format,
};

use dashmap::DashMap;
use log::{debug, error, info, trace, warn};
use ordered_float::NotNan;
use thiserror::Error;
use tokio::sync::watch::Receiver;
use tonic::Status;
use uuid::Uuid;

use crate::{
    bidask::{Ask, Bid},
    AccId, SecId,
};

#[derive(Debug, Clone)]
pub struct Market {
    securities: Arc<DashMap<SecId, Security>>,
    accounts: Arc<DashMap<AccId, HashMap<SecId, usize>>>,
    pub update_reciever: Receiver<()>,
}

impl Market {
    pub fn new(mut update_reciever: Receiver<()>) -> Self {
        let securities: Arc<DashMap<SecId, Security>> = Default::default();
        let accounts: Arc<DashMap<AccId, HashMap<SecId, usize>>> = Default::default();

        Self {
            securities,
            accounts,
            update_reciever,
        }
    }

    pub fn get_lowest_bid_price(&self, sec_id: SecId) -> Result<Option<f64>, MarketError> {
        if let Some(sec) = self.securities.get(&sec_id) {
            if let Some(bid) = sec.bids.peek() {
                debug!(
                    "Current lowest bid price for security {} is {}",
                    sec_id.0, bid.price.0
                );
                Ok(Some(*bid.price.0))
            } else {
                debug!("No bids placed for security {}", sec_id.0);
                Ok(None)

            }
        } else {
            error!(
                "Attempted to find lowest bid price for nonexistent security {}",
                sec_id.0
            );
            Err(MarketError::SecDoesNotExist(sec_id))
        }
    }

    pub fn get_highest_ask_price(&self, sec_id: SecId) -> Result<Option<f64>, MarketError> {
        if let Some(sec) = self.securities.get(&sec_id) {
            if let Some(ask) = sec.asks.peek() {
                debug!(
                    "Current highest ask price for security {} is {}",
                    sec_id.0, ask.price
                );
                Ok(Some(*ask.price))
            } else {
                debug!("No asks placed for security {}", sec_id.0);
                Ok(None)

            }
        } else {
            error!(
                "Attempted to list of bids of nonexistent security {}",
                sec_id.0
            );
            Err(MarketError::SecDoesNotExist(sec_id))
        }
    }

    pub fn current_value(&self, sec_id: SecId) -> Result<f64, MarketError> {
        if let Some(sec) = self.securities.get(&sec_id) {
            let price = sec.last_trade;
            debug!(
                "Security {} is currently worth {} by last trade value",
                sec_id.0, price
            );
            Ok(price)
        } else {
            error!(
                "Attempted to calculate value of nonexistent security {}",
                sec_id.0
            );
            Err(MarketError::SecDoesNotExist(sec_id))
        }
    }

    pub fn market_cap(&self, sec_id: SecId) -> Result<f64, MarketError> {
        if let Some(sec) = self.securities.get(&sec_id) {
            let price = sec.last_trade;
            let mcap = self
                .accounts
                .iter()
                .filter_map(|a| a.get(&sec_id).copied())
                .map(|a| a as f64 * price)
                .sum();
            debug!("Market cap of security {} is {}", sec_id.0, mcap);
            Ok(mcap)
        } else {
            error!(
                "Attempted to calculate market cap of nonexistent security {}",
                sec_id.0
            );
            Err(MarketError::SecDoesNotExist(sec_id))
        }
    }

    pub fn account_value(&self, acc_id: AccId, sec_id: SecId) -> Result<f64, MarketError> {
        if let Some(account) = self.accounts.get(&acc_id) {
            if let Some(security) = self.securities.get(&sec_id) {
                if let Some(amount) = account.get(&sec_id) {
                    let value = *amount as f64 * security.last_trade;
                    if value == 0.0 {
                        debug!(
                            "Account {} has no holdings in security {}",
                            acc_id.0, sec_id.0
                        );
                    } else {
                        debug!(
                            "Account {} has holdings in security {} worth {}",
                            acc_id.0, sec_id.0, value
                        );
                    }

                    Ok(value)
                } else {
                    debug!(
                        "Account {} has no holdings in security {}",
                        acc_id.0, sec_id.0
                    );
                    Ok(0.0)
                }
            } else {
                error!(
                    "Attempted to look up holdings of account {} in nonexistent security {}",
                    acc_id.0, sec_id.0
                );
                Err(MarketError::SecDoesNotExist(sec_id))
            }
        } else {
            error!(
                "Attempted to look up holdings of nonexistent account {} in security {}",
                acc_id.0, sec_id.0
            );
            Err(MarketError::AccDoesNotExist(acc_id))
        }
    }

    pub fn account_num_shares(&self, acc_id: AccId, sec_id: SecId) -> Result<usize, MarketError> {
        if let Some(account) = self.accounts.get(&acc_id) {
            if let Some(_security) = self.securities.get(&sec_id) {
                let amount = account.get(&sec_id).unwrap_or(&0);
                debug!(
                    "Account {} has {} shares in security {}",
                    acc_id.0, amount, sec_id.0
                );
                Ok(*amount)
            } else {
                error!(
                    "Attempted to look up holdings of account {} in nonexistent security {}",
                    acc_id.0, sec_id.0
                );
                Err(MarketError::SecDoesNotExist(sec_id))
            }
        } else {
            error!(
                "Attempted to look up holdings of nonexistent account {} in security {}",
                acc_id.0, sec_id.0
            );
            Err(MarketError::AccDoesNotExist(acc_id))
        }
    }

    // fn close(self) {
    //     self.thread.join().unwrap();
    // }

    pub fn create_account(&self) -> AccId {
        let id = AccId(Uuid::new_v4());
        let accs = Arc::clone(&self.accounts);
        accs.insert(id, Default::default());
        info!("Account {} created", id.0);
        id
    }

    pub fn place_bid(&self, acc: AccId, sec: SecId, price: f64) -> Result<(), MarketError> {
        let sec_id = sec;
        if !self.accounts.contains_key(&acc) {
            error!("Nonexistent account {} attempted to place bid for share of security {} at max price of {}", acc.0, sec_id.0, price);
            return Err(MarketError::AccDoesNotExist(acc));
        }
        if let Some(mut sec) = self.securities.get_mut(&sec) {
            sec.bids.push(Bid::new(acc, price));
            info!(
                "Account {} placed bid for share of {} at max price of {}",
                acc.0, sec_id.0, price
            );
            Ok(())
        } else {
            error!("Account {} attempted to place bid for share of nonexistent security {} at max price of {}", acc.0, sec_id.0, price);
            Err(MarketError::SecDoesNotExist(sec))
        }
    }

    pub fn place_ask(&self, acc: AccId, sec: SecId, price: f64) -> Result<(), MarketError> {
        let sec_id = sec;
        if !self.accounts.contains_key(&acc) {
            error!("Nonexistent account {} attempted to place ask for share of security {} at min price of {}", acc.0, sec_id.0, price);
            return Err(MarketError::AccDoesNotExist(acc));
        }
        if let Some(mut sec) = self.securities.get_mut(&sec) {
            sec.asks.push(Ask::new(acc, price));
            info!(
                "Account {} placed ask for share of {} at min price of {}",
                acc.0, sec_id.0, price
            );
            Ok(())
        } else {
            error!("Account {} attempted to place ask for share of nonexistent security {} at min price of {}", acc.0, sec_id.0, price);
            Err(MarketError::SecDoesNotExist(sec))
        }
    }

    pub fn list_securities(&self) -> Vec<SecId> {
        let map = Arc::as_ref(&self.securities);
        map.iter().map(|s|s.pair().0.clone()).collect::<Vec<_>>()
    }

    pub fn create_security(
        &self,
        founding_shares: usize,
        founding_price: f64,
    ) -> (SecId, AccId) {
        let sec_id = SecId(Uuid::new_v4());
        self.securities.insert(sec_id, Security::default());
        let acc_id = self.create_account();
        let mut account = self.accounts.get_mut(&acc_id).unwrap();
        account.insert(sec_id, founding_shares);
        let mut security = self.securities.get_mut(&sec_id).unwrap();
        security.asks.reserve(founding_shares);
        for _ in 0..founding_shares {
            security.asks.push(Ask {
                price: NotNan::new(founding_price).unwrap(),
                account: acc_id,
            });
            security.last_trade = founding_price;
        }
        info!("Security {} created", sec_id.0);

        (sec_id, acc_id)
    }

    pub fn run_market_loop(market: Market) {
        let securities = market.securities;
        let accounts = market.accounts;
        for mut sec in securities.iter_mut() {
            let (sec_id, sec) = sec.pair_mut();
            trace!("Processing security {}", sec_id.0);
            'find: while let (Some(bid), Some(ask)) = (sec.bids.peek(), sec.asks.peek()) {
                trace!(
                    "Cheching a bid by account {} of {} against an ask by account {} of {}",
                    bid.account.0,
                    bid.price.0,
                    ask.account.0,
                    ask.price
                );

                if bid.price.0 >= ask.price {
                    trace!("Processing possible transaction");
                    let bid = sec.bids.pop().unwrap();
                    let ask = sec.asks.pop().unwrap();

                    let price = bid.price;
                    trace!("Agreed price: {}", price.0);

                    let mut seller = accounts.get_mut(&ask.account).unwrap();
                    let (seller_id, seller) = seller.pair_mut();

                    let mut buyer = accounts.get_mut(&bid.account).unwrap();
                    let (buyer_id, buyer) = buyer.pair_mut();

                    if !seller.contains_key(sec_id) {
                        warn!("Seller account {} has no holdings in security {}; Transaction unavailable", seller_id.0, sec_id.0);
                        continue;
                    }

                    if *seller.get(sec_id).unwrap() < 1 {
                        warn!("Seller account {} has no holdings in security {}; Transaction unavailable", seller_id.0, sec_id.0);
                        continue;
                    }

                    trace!("Seller account {} has at least one share of security {}, transaction will go ahead", seller_id.0, sec_id.0);

                    *seller.get_mut(sec_id).unwrap() -= 1;
                    trace!(
                        "Removed one share of security {} from seller account {}",
                        sec_id.0,
                        seller_id.0
                    );
                    buyer
                        .entry(*sec_id)
                        .and_modify(|a| *a += 1)
                        .or_insert_with(|| 1);
                    trace!(
                        "Added one share of security {} to buyer account {}",
                        sec_id.0,
                        buyer_id.0
                    );

                    info!(
                        "Transaction occured between buyer {} and seller {}:",
                        buyer_id.0, seller_id.0
                    );
                    sec.last_trade = *price.0;

                    info!("Security {} sold for {}", sec_id.0, price.0);
                } else {
                    debug!("No available transactions");
                    break 'find;
                }
            }
        }
    }
}

#[derive(Error, Debug)]
pub enum MarketError {
    #[error("Security {} does not exist", 0.0)]
    SecDoesNotExist(SecId),
    #[error("Account {} does not exist", 0.0)]
    AccDoesNotExist(AccId),
    #[error("No bids are placed for security {}", 0.0)]
    NoBids(SecId),
    #[error("No asks are placed for security {}", 0.0)]
    NoAsks(SecId),
}

impl From<MarketError> for Status {
    fn from(value: MarketError) -> Self {
        match value {
            MarketError::SecDoesNotExist(sec) => {
                Status::failed_precondition(format!("Security {} does not exist", sec.0))
            }
            MarketError::AccDoesNotExist(acc) => {
                Status::failed_precondition(format!("Account {} does not exist", acc.0))
            }
            MarketError::NoBids(sec) => Status::ok(format!("No bids are placed for security {}", sec.0)),
            MarketError::NoAsks(sec) => Status::ok(format!("No asks are placed for security {}", sec.0)),
        }
    }
}

#[derive(Debug, Default)]
pub struct Security {
    last_trade: f64,
    bids: BinaryHeap<Bid>,
    asks: BinaryHeap<Ask>,
}
