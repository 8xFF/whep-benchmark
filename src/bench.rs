use std::time::Duration;

use async_std::channel::Sender;

use crate::whep::{Stats, WhepClient, WhepEvent};

pub enum BenchEvent {
    Connecting(usize),
    Connected(usize),
    Stats(usize, Stats),
    Disconnected(usize),
}

pub struct BenchPlan {
    pub count: usize,
    pub interval: Duration,
    pub live: Duration,
}

pub struct BenchRunner {
    plan: BenchPlan,
    url: String,
    token: String,
    count: usize,
    event_tx: Sender<BenchEvent>,
}

impl BenchRunner {
    pub fn new(url: &str, token: &str, plan: BenchPlan, event_tx: Sender<BenchEvent>) -> Self {
        BenchRunner {
            plan,
            url: url.to_string(),
            token: token.to_string(),
            count: 0,
            event_tx,
        }
    }

    pub async fn bootstrap(&mut self) {
        while self.count < self.plan.count {
            self.count += 1;
            let client_id = self.count;
            let event_tx = self.event_tx.clone();
            event_tx
                .send(BenchEvent::Connecting(client_id))
                .await
                .expect("should send connecting event");
            let url = self.url.clone();
            let token = self.token.clone();
            let live_time = self.plan.live;
            async_std::task::spawn(async move {
                let mut client = WhepClient::new(&url, &token).expect("should create whep client");
                client.prepare().await.expect("should connect");
                let started = std::time::Instant::now();
                loop {
                    if started.elapsed() > live_time {
                        log::info!("[WhepClient] disconnecting after life time expired");
                        client.disconnect().await.expect("should disconnect");
                        break;
                    }

                    match client.recv().await {
                        Ok(event) => match event {
                            WhepEvent::Connected => {
                                event_tx
                                    .send(BenchEvent::Connected(client_id))
                                    .await
                                    .expect("should send connected event");
                                log::info!("[WhepClient] connected");
                            }
                            WhepEvent::Disconnected => {
                                log::info!("[WhepClient] disconnected");
                                break;
                            }
                            WhepEvent::Stats(stats) => {
                                log::info!("[WhepClient] stats: {:?}", stats);
                                event_tx
                                    .send(BenchEvent::Stats(client_id, stats))
                                    .await
                                    .expect("should send stats event");
                            }
                            WhepEvent::Continue => {}
                        },
                        Err(err) => {
                            log::error!("[WhepClient] error: {:?}", err);
                            break;
                        }
                    }
                }
                event_tx
                    .send(BenchEvent::Disconnected(client_id))
                    .await
                    .expect("should send disconnected event");
            });
            async_std::task::sleep(self.plan.interval).await;
        }

        log::info!("[BenchRunner] done");
    }
}
