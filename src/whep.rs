use std::{
    error::Error,
    net::SocketAddr,
    time::{Duration, Instant},
};

use async_std::prelude::FutureExt;
use local_ip_address::list_afinet_netifas;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use str0m::{
    bwe::Bitrate,
    change::SdpAnswer,
    media::{Direction, MediaKind},
    net::{Protocol, Receive},
    Candidate, Event, IceConnectionState, Input, Output, Rtc,
};
use udp_sas_async::async_std::UdpSocketSas;

#[derive(Debug)]
pub struct Stats {
    pub send_kbps: u64,
    pub recv_kbps: u64,
    pub live_ms: u32,
    pub rtt_ms: u32,
    pub lost: f32,
}

#[derive(Debug)]
pub enum WhepEvent {
    Continue,
    Connected,
    Stats(Stats),
    Disconnected,
}

#[derive(Debug)]
pub enum WhepError {
    UrlError,
    ServerError(Box<dyn Error>),
    SdpError,
    WebrtcError,
    NetworkError(Box<dyn Error>),
}

pub struct WhepClient {
    rtc: Rtc,
    socket: UdpSocketSas,
    location: Option<String>,
    parse_url: url::Url,
    url: String,
    token: String,
    live_at: Option<Instant>,
    rtt: u32,
    buf: [u8; 1500],
    pre_ts: Instant,
    pre_send_bytes: u64,
    pre_recv_bytes: u64,
}

impl WhepClient {
    pub fn new(url: &str, token: &str) -> Result<Self, WhepError> {
        let socket =
            UdpSocketSas::bind("0.0.0.0:0".parse().unwrap()).expect("Should bind udp socket");
        let mut rtc = Rtc::builder()
            .set_rtp_mode(true)
            .set_stats_interval(Some(Duration::from_secs(2)))
            .enable_bwe(Some(Bitrate::kbps(1000)))
            .build();

        if let Ok(network_interfaces) = list_afinet_netifas() {
            for (_name, ip) in network_interfaces {
                if ip.is_ipv4() {
                    rtc.add_local_candidate(
                        Candidate::host(
                            SocketAddr::new(ip, socket.local_addr().port()),
                            str0m::net::Protocol::Udp,
                        )
                        .expect(""),
                    );
                }
            }
        }

        Ok(Self {
            socket,
            rtc,
            location: None,
            live_at: None,
            parse_url: url::Url::parse(url).map_err(|_| WhepError::UrlError)?,
            url: url.to_string(),
            token: token.to_string(),
            rtt: 0,
            buf: [0; 1500],
            pre_ts: Instant::now(),
            pre_send_bytes: 0,
            pre_recv_bytes: 0,
        })
    }

    pub async fn prepare(&mut self) -> Result<(), WhepError> {
        let mut change = self.rtc.sdp_api();
        change.add_media(
            MediaKind::Audio,
            Direction::RecvOnly,
            Some("audio_0".to_string()),
            Some("audio_0".to_string()),
        );
        change.add_media(
            MediaKind::Video,
            Direction::RecvOnly,
            Some("video_0".to_string()),
            Some("video_0".to_string()),
        );

        let (offer, pending) = change.apply().expect("");

        let offer_str = offer.to_sdp_string();
        log::info!("offer: {}", offer_str);

        let res = reqwest::Client::new()
            .post(&self.url)
            .header(CONTENT_TYPE, "application/sdp")
            .header(USER_AGENT, "Whep Benchmark in Rust")
            .header(ACCEPT, "application/sdp")
            //set token with Bear header
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .body(offer_str)
            .send()
            .await
            .map_err(|e| WhepError::ServerError(e.into()))?;

        // get answer sdp from body
        let location = res.headers().get("location").cloned();
        let http_code = res.status();
        let answer = res
            .text()
            .await
            .map_err(|e| WhepError::ServerError(e.into()))?;
        log::info!("answer: {} {}", http_code, answer);
        let answer = SdpAnswer::from_sdp_string(&answer).map_err(|_| WhepError::SdpError)?;

        // get location form header location
        let location = location
            .ok_or(WhepError::ServerError("Location Header Not Found".into()))?
            .to_str()
            .map_err(|e| WhepError::ServerError(e.into()))?
            .to_string();

        // if location started with / then concat with based url, else use location as url
        let url = if location.starts_with("/") {
            format!(
                "{}{}",
                self.parse_url.origin().ascii_serialization(),
                location
            )
        } else {
            location.to_string()
        };
        self.location = Some(url);

        // apply answer sdp
        self.rtc
            .sdp_api()
            .accept_answer(pending, answer)
            .map_err(|_| WhepError::SdpError)?;

        Ok(())
    }

    pub async fn disconnect(&mut self) -> Result<(), WhepError> {
        if let Some(location) = self.location.take() {
            reqwest::Client::new()
                .delete(location)
                .send()
                .await
                .map_err(|e| WhepError::ServerError(e.into()))?;
        }
        Ok(())
    }

    pub async fn recv<'a>(&mut self) -> Result<WhepEvent, WhepError> {
        let timeout = match self.rtc.poll_output().map_err(|_| WhepError::WebrtcError)? {
            Output::Event(event) => match event {
                Event::Connected => {
                    self.live_at = Some(Instant::now());
                    return Ok(WhepEvent::Connected);
                }
                Event::IceConnectionStateChange(state) => {
                    log::info!("[WhepClient] ice connection state change: {:?}", state);
                    match state {
                        IceConnectionState::Disconnected => return Ok(WhepEvent::Disconnected),
                        _ => return Ok(WhepEvent::Continue),
                    }
                }
                Event::MediaIngressStats(stats) => {
                    self.rtt = stats.rtt.unwrap_or(0.0) as u32;
                    return Ok(WhepEvent::Continue);
                }
                Event::PeerStats(stats) => {
                    let duration = self.pre_ts.elapsed().as_millis() as u64;
                    self.pre_ts = Instant::now();

                    let send_kbps = ((stats.peer_bytes_tx - self.pre_send_bytes) * 8) / duration;
                    let recv_kbps = ((stats.peer_bytes_rx - self.pre_recv_bytes) * 8) / duration;
                    self.pre_send_bytes = stats.peer_bytes_tx;
                    self.pre_recv_bytes = stats.peer_bytes_rx;

                    return Ok(WhepEvent::Stats(Stats {
                        send_kbps,
                        recv_kbps,
                        lost: stats.ingress_loss_fraction.unwrap_or(0.0),
                        live_ms: self
                            .live_at
                            .map(|t| t.elapsed().as_millis() as u32)
                            .unwrap_or(0),
                        rtt_ms: self.rtt,
                    }));
                }
                Event::RtpPacket(pkt) => {
                    log::trace!("rtp packet: {:?}", pkt);
                    return Ok(WhepEvent::Continue);
                }
                _ => {
                    return Ok(WhepEvent::Continue);
                }
            },
            Output::Timeout(timeout) => timeout,
            Output::Transmit(send) => {
                if let Err(e) = self
                    .socket
                    .send_sas(&send.contents, send.source.ip(), send.destination)
                    .await
                {
                    log::debug!(
                        "sending to {} => {}, len {} error {:?}",
                        send.source,
                        send.destination,
                        send.contents.len(),
                        e
                    );
                };
                return Ok(WhepEvent::Continue);
            }
        };

        let duration = timeout - Instant::now();
        if duration.is_zero() {
            // Drive time forwards in rtc straight away.
            return match self.rtc.handle_input(Input::Timeout(Instant::now())) {
                Ok(_) => Ok(WhepEvent::Continue),
                Err(e) => {
                    log::error!("[WhepClient] error handle input rtc: {:?}", e);
                    Ok(WhepEvent::Continue)
                }
            };
        }

        let input = match self.socket.recv_sas(&mut self.buf).timeout(duration).await {
            Ok(Ok((n, source, destination))) => {
                // UDP data received.
                log::trace!("received from {} => {}, len {}", source, destination, n);
                Input::Receive(
                    Instant::now(),
                    Receive {
                        proto: Protocol::Udp,
                        source,
                        destination: SocketAddr::new(destination, self.socket.local_addr().port()),
                        contents: (&self.buf[..n]).try_into().expect("should webrtc"),
                    },
                )
            }
            Ok(Err(e)) => {
                log::error!("[TransportWebrtc] network error {:?}", e);
                return Err(WhepError::NetworkError(e.into()));
            }
            Err(_e) => {
                // Expected error for set_read_timeout().
                // One for windows, one for the rest.
                Input::Timeout(Instant::now())
            }
        };

        // Input is either a Timeout or Receive of data. Both drive the state forward.
        self.rtc
            .handle_input(input)
            .map_err(|_| WhepError::WebrtcError)?;
        return Ok(WhepEvent::Continue);
    }
}
