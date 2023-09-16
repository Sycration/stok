#![allow(unused_imports)]
use bidask::{Ask, Bid};
use log::{debug, error, info, trace, warn};
use ordered_float::NotNan;
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    env,
    error::Error,
    pin::Pin,
    sync::Arc,
    thread::{self, JoinHandle, Thread},
    time::Duration,
};
use stok::*;
use thiserror::Error;
use tokio::{
    join,
    runtime::Builder,
    sync::watch::{self, Receiver},
    time::{interval, Interval},
};
use tokio_stream::{
    wrappers::{ReceiverStream, WatchStream},
    Stream, StreamExt,
};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use uuid::Uuid;
mod bidask;
mod market;
use crate::market::Market;
use tonic::{transport::Server, Request, Response, Status};

#[derive(Debug, Default, PartialEq, Eq, Hash, Clone, Copy)]
pub struct SecId(Uuid);
#[derive(Debug, Default, PartialEq, Eq, Hash, Clone, Copy)]
pub struct AccId(Uuid);

pub mod stok {
    tonic::include_proto!("stok"); // The string specified here must match the proto package name
}

type ResponseStream = Pin<Box<dyn Stream<Item = Result<SecValue, Status>> + Send>>;

#[derive(Debug)]
pub struct MyGreeter {
    market: crate::market::Market,
}

#[tonic::async_trait]
impl market_server::Market for MyGreeter {
    type RegisterSecValueStream = ResponseStream;

    async fn list_securities(
        &self,
        _request: tonic::Request<ListSecsReq>,
    ) -> std::result::Result<tonic::Response<SecList>, tonic::Status> {
        let secs = self.market.list_securities().iter().map(|i| stok::SecId {
            id: Some(stok::Uuid {
                value: i.0.to_string(),
            }),
        }).collect::<Vec<_>>();

        Ok(Response::new(SecList { list: secs }))
    }

    async fn create_account(
        &self,
        _request: tonic::Request<CreateAccReq>,
    ) -> std::result::Result<tonic::Response<stok::AccId>, tonic::Status> {
        let acc = self.market.create_account();

        return Ok(Response::new(stok::AccId {
            id: Some(stok::Uuid {
                value: acc.0.to_string(),
            }),
        }));
    }

    async fn create_security(
        &self,
        request: tonic::Request<CreateSecReq>,
    ) -> std::result::Result<tonic::Response<stok::CreateSecResponse>, tonic::Status> {
        let request = request.into_inner();
        let (founding_shares, founding_price) = (request.founding_shares, request.founding_price);
        let (sec, acc) = self
            .market
            .create_security(founding_shares as usize, founding_price);

        return Ok(Response::new(stok::CreateSecResponse {
            owner_acct: Some(stok::AccId {
                id: Some(stok::Uuid {
                    value: acc.0.to_string(),
                }),
            }),
            security: Some(stok::SecId {
                id: Some(stok::Uuid {
                    value: sec.0.to_string(),
                }),
            }),
        }));
    }

    async fn register_sec_value(
        &self,
        request: tonic::Request<SecValueReq>,
    ) -> Result<tonic::Response<Self::RegisterSecValueStream>, tonic::Status> {
        let (tx, rx) = tokio::sync::mpsc::channel(128);
        let mut update_ping = WatchStream::new(self.market.update_reciever.clone());

        let sec = if let Some(id) = request.into_inner().sec.map(|s| s.id).flatten() {
            if let Ok(id) = Uuid::parse_str(&id.value) {
                id
            } else {
                return Err(Status::data_loss(format!(
                    "Invalid security ID sent: {}",
                    id.value
                )));
            }
        } else {
            return Err(Status::data_loss("No security ID sent".to_string()));
        };
        while let Some(_) = update_ping.next().await {
            let value = self.market.current_value(SecId(sec));
            let value = if let Ok(value) = value {
                value
            } else {
                return Err(Status::failed_precondition(format!(
                    "Security {} does not exist",
                    sec
                )));
            };

            match tx
                .send(Ok(SecValue {
                    sec: Some(stok::SecId {
                        id: Some(stok::Uuid {
                            value: sec.to_string(),
                        }),
                    }),
                    value,
                }))
                .await
            {
                Ok(_) => {}
                Err(_) => {
                    break;
                }
            }
        }

        let output_stream = ReceiverStream::new(rx);
        Ok(Response::new(
            Box::pin(output_stream) as Self::RegisterSecValueStream
        ))
    }

    async fn get_lowest_bid(
        &self,
        request: tonic::Request<LowestBidReq>,
    ) -> Result<tonic::Response<LowestBid>, tonic::Status> {
        let sec = if let Some(id) = request.into_inner().sec.map(|s| s.id).flatten() {
            if let Ok(id) = Uuid::parse_str(&id.value) {
                id
            } else {
                return Err(Status::data_loss(format!(
                    "Invalid security ID sent: {}",
                    id.value
                )));
            }
        } else {
            return Err(Status::data_loss("No security ID sent".to_string()));
        };

        let bid = self.market.get_lowest_bid_price(SecId(sec))?;

        Ok(Response::new(LowestBid { price: bid }))
    }
    async fn get_highest_ask(
        &self,
        request: tonic::Request<HighestAskReq>,
    ) -> Result<tonic::Response<HighestAsk>, tonic::Status> {
        let sec = if let Some(id) = request.into_inner().sec.map(|s| s.id).flatten() {
            if let Ok(id) = Uuid::parse_str(&id.value) {
                id
            } else {
                return Err(Status::data_loss(format!(
                    "Invalid security ID sent: {}",
                    id.value
                )));
            }
        } else {
            return Err(Status::data_loss("No security ID sent".to_string()));
        };

        let ask = self.market.get_highest_ask_price(SecId(sec))?;

        Ok(Response::new(HighestAsk { price: ask }))
    }
    async fn get_market_cap(
        &self,
        request: tonic::Request<MarketCapReq>,
    ) -> Result<tonic::Response<MarketCap>, tonic::Status> {
        let sec = if let Some(id) = request.into_inner().sec.map(|s| s.id).flatten() {
            if let Ok(id) = Uuid::parse_str(&id.value) {
                id
            } else {
                return Err(Status::data_loss(format!(
                    "Invalid security ID sent: {}",
                    id.value
                )));
            }
        } else {
            return Err(Status::data_loss("No security ID sent".to_string()));
        };

        let marketcap = self.market.market_cap(SecId(sec))?;

        Ok(Response::new(MarketCap { marketcap }))
    }
    async fn place_ask(
        &self,
        request: tonic::Request<stok::Ask>,
    ) -> Result<tonic::Response<AskPlaced>, tonic::Status> {
        let req = request.into_inner();

        let sec = if let Some(id) = req.sec.map(|s| s.id).flatten() {
            if let Ok(id) = Uuid::parse_str(&id.value) {
                id
            } else {
                return Err(Status::data_loss(format!(
                    "Invalid security ID sent: {}",
                    id.value
                )));
            }
        } else {
            return Err(Status::data_loss("No security ID sent".to_string()));
        };

        let acc = if let Some(id) = req.acc.map(|s| s.id).flatten() {
            if let Ok(id) = Uuid::parse_str(&id.value) {
                id
            } else {
                return Err(Status::data_loss(format!(
                    "Invalid account ID sent: {}",
                    id.value
                )));
            }
        } else {
            return Err(Status::data_loss("No account ID sent".to_string()));
        };

        self.market.place_ask(AccId(acc), SecId(sec), req.price)?;

        Ok(Response::new(AskPlaced { price: req.price }))
    }
    async fn place_bid(
        &self,
        request: tonic::Request<stok::Bid>,
    ) -> Result<tonic::Response<BidPlaced>, tonic::Status> {
        let req = request.into_inner();

        let sec = if let Some(id) = req.sec.map(|s| s.id).flatten() {
            if let Ok(id) = Uuid::parse_str(&id.value) {
                id
            } else {
                return Err(Status::data_loss(format!(
                    "Invalid security ID sent: {}",
                    id.value
                )));
            }
        } else {
            return Err(Status::data_loss("No security ID sent".to_string()));
        };

        let acc = if let Some(id) = req.acc.map(|s| s.id).flatten() {
            if let Ok(id) = Uuid::parse_str(&id.value) {
                id
            } else {
                return Err(Status::data_loss(format!(
                    "Invalid account ID sent: {}",
                    id.value
                )));
            }
        } else {
            return Err(Status::data_loss("No account ID sent".to_string()));
        };

        self.market.place_bid(AccId(acc), SecId(sec), req.price)?;

        Ok(Response::new(BidPlaced { price: req.price }))
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    env::set_var("RUST_LOG", "trace");
    env_logger::init();
    let subscriber = FmtSubscriber::builder().pretty().finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let rt = Builder::new_multi_thread().enable_all().build().unwrap();

    rt.block_on(app());

    Ok(())
}

async fn app() {
    let (tx, mut rx) = watch::channel(());

    let mut market = crate::Market::new(rx);

    let updater_market = market.clone();
    let update_task = tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(1));

        loop {
            interval.tick().await;
            trace!("Market update tick");
            let market = updater_market.clone();
            Market::run_market_loop(market);
            tx.send(());
        }
    });

    let addr = "0.0.0.0:50051".parse().unwrap();
    let greeter = MyGreeter { market };

    let server = Server::builder()
        .add_service(crate::market_server::MarketServer::new(greeter))
        .serve(addr);

    join!(update_task, server);
}
