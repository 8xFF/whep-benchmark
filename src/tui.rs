use std::{collections::BTreeMap, sync::Arc, time::Duration};

use async_std::{channel::Receiver, stream::StreamExt};
use dioxus::prelude::*;
use futures_util::{select, FutureExt};
use parking_lot::RwLock;

use crate::{bench::BenchEvent, whep::Stats};

#[derive(Default)]
pub struct Client {
    id: usize,
    connected: bool,
    stats: Option<Stats>,
}

#[derive(Default)]
struct AppState {
    clients: BTreeMap<usize, Client>,
}

impl AppState {
    pub fn add_client(&mut self, id: usize) {
        self.clients.insert(
            id,
            Client {
                id,
                ..Default::default()
            },
        );
    }

    pub fn set_client_connected(&mut self, id: usize) {
        if let Some(client) = self.clients.get_mut(&id) {
            client.connected = true;
        }
    }

    pub fn set_client_stats(&mut self, id: usize, stats: Stats) {
        if let Some(client) = self.clients.get_mut(&id) {
            client.stats = Some(stats);
        }
    }

    pub fn remove_client(&mut self, id: usize) {
        self.clients.remove(&id);
    }

    pub fn get_clients(&self) -> &BTreeMap<usize, Client> {
        &self.clients
    }

    pub fn clients_sum(&self) -> usize {
        self.clients.len()
    }

    pub fn clients_connected(&self) -> usize {
        self.clients.values().filter(|v| v.connected).count()
    }

    pub fn sum_send_kbps(&self) -> u64 {
        self.clients
            .values()
            .filter_map(|v| v.stats.as_ref())
            .map(|v| v.send_kbps)
            .sum::<u64>()
    }

    pub fn sum_recv_kbps(&self) -> u64 {
        self.clients
            .values()
            .filter_map(|v| v.stats.as_ref())
            .map(|v| v.recv_kbps)
            .sum::<u64>()
    }
}

pub struct AppProps {
    pub rx: Arc<Receiver<BenchEvent>>,
}

pub fn dioxus_app(cx: Scope<AppProps>) -> Element {
    let ver = use_state(cx, || 0);
    let state = use_state(cx, || RwLock::new(AppState::default()));

    let _ = use_coroutine(cx, |_: UnboundedReceiver<()>| {
        let rx = cx.props.rx.clone();
        let state = state.to_owned();
        let ver = ver.to_owned();
        let mut tick = async_std::stream::interval(Duration::from_millis(300));
        async move {
            let mut has_update = false;
            loop {
                select! {
                    _ = tick.next().fuse() => {
                        if has_update {
                            ver.set(*ver + 1);
                        }
                    }
                    event = rx.recv().fuse() => {
                        let mut state = state.write();
                        match event {
                            Ok(BenchEvent::Connecting(id)) => {
                                state.add_client(id);
                            }
                            Ok(BenchEvent::Connected(id)) => {
                                state.set_client_connected(id);
                            }
                            Ok(BenchEvent::Stats(id, stats)) => {
                                state.set_client_stats(id, stats);
                            }
                            Ok(BenchEvent::Disconnected(id)) => {
                                state.remove_client(id);
                            }
                            Err(_) => {
                                break;
                            }
                        };

                        has_update = true;
                    }
                };
            }
        }
    });

    let state = state.get().read();
    let clients_sum = state.clients_sum();
    let clients_connected = state.clients_connected();
    let sum_send_kbps = state.sum_send_kbps();
    let sum_recv_kbps = state.sum_recv_kbps();
    let clients = state.get_clients();

    cx.render(rsx! {
        div{
            width: "100%",
            flex_direction: "column",
            header {
                width: "100%",

                ul {
                    overflow: "hidden",
                    background_color: "#ffffff",
                    color: "#000000",
                    width: "100%",

                    div {
                        flex_direction: "row",
                        width: "100%",

                        li {
                            width: "40%",

                            "Clients {clients_sum}"
                        }
                        li {
                            width: "30%",

                            "Connected {clients_connected}"
                        }
                        li {
                            width: "30%",

                            "Send: {sum_send_kbps} kbps, Recv: {sum_recv_kbps} kbps"
                        }
                    }
                }
            }
            div {
                width: "100%",

                ul {
                    width: "100%",
                    flex_direction: "column",

                    clients.values().into_iter().map(|v| {
                        rsx!(div {
                            flex_direction: "row",
                            width: "100%",

                            li {
                                width: "40%",

                                format!("Sender {}", v.id)
                            }
                            li {
                                width: "30%",

                                if v.connected { "Running" } else { "Connecting" }
                            }
                            li {
                                width: "30%",

                                if let Some(stats) = &v.stats { format!("{} kbps/ {} kbps", stats.send_kbps, stats.recv_kbps) } else { format!("...") }
                            }
                        })
                    })
                }
            }
        }
    })
}
