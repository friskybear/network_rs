use anyhow::Result;
use colorful::Colorful;
use futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{identify, request_response, Multiaddr, PeerId, StreamProtocol, SwarmBuilder};
use libp2p_request_response::ProtocolSupport;
use pyo3::prelude::*;
use std::time::Duration;

use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tokio::select;
/// agent version
const AGENT_VERSION: &str = "peer/0.0.1";
const PROTOCOL: &str = "/connction/1";

#[derive(Debug, Serialize, Deserialize, Clone)]
struct RequestResponse {
    message: String,
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    identify: identify::Behaviour,
    request_response: request_response::cbor::Behaviour<RequestResponse, RequestResponse>,
}
#[pyfunction]
fn get_key() -> Vec<u8> {
    Keypair::generate_ed25519().to_protobuf_encoding().unwrap()
}
async fn receiver(key: Vec<u8>, port: u16, mut n: u32, func: PyObject) {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()
                .unwrap(),
        )
        .init();
    let key_pair = Keypair::from_protobuf_encoding(&key).unwrap();
    let local_peer_id = PeerId::from_public_key(&key_pair.public());

    let mut swarm = SwarmBuilder::with_existing_identity(key_pair)
        .with_tokio()
        .with_quic()
        .with_behaviour(|key| Behaviour {
            identify: {
                let cfg = identify::Config::new(PROTOCOL.to_string(), key.public())
                    .with_push_listen_addr_updates(true)
                    .with_agent_version(AGENT_VERSION.to_string());
                identify::Behaviour::new(cfg)
            },
            request_response: {
                request_response::cbor::Behaviour::<RequestResponse, RequestResponse>::new(
                    [(StreamProtocol::new(PROTOCOL), ProtocolSupport::Full)],
                    request_response::Config::default().with_max_concurrent_streams(1),
                )
            },
        })
        .unwrap()
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    swarm
        .listen_on(
            format!("/ip4/0.0.0.0/udp/{port}/quic-v1")
                .parse()
                .unwrap(),
        )
        .unwrap();

    let mut my_addr = Box::pin(Multiaddr::empty());
    loop {
        select! {
            _ = async{},if n==0 =>{return},
            event = swarm.select_next_some() => match event {
                libp2p::swarm::SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(request_response::Event::Message { peer, message })) => match message {
                    libp2p_request_response::Message::Request { request_id, request, channel } => {
                        let req: RequestResponse = request;
                        tracing::info!("Request from:{}  req_id = {}  message = {}", peer,request_id, req.message);
                        let response: RequestResponse = RequestResponse {
                            message: {
                                Python::with_gil(|py| -> PyResult<String> {
                                    let args = (req.message,);
                                    let result = func.call1(py, args).unwrap();
                                    Ok(result.to_string())
                                }).unwrap()
                            }
                        };
                        tracing::info!("Responding with:{:?}",response);
                        println!("{}",channel.is_open().to_string().red());
                        swarm.behaviour_mut().request_response.send_response(channel, response).unwrap();
                        n-=1;
                    },
                    libp2p_request_response::Message::Response {..} => {}
                },
                
                libp2p::swarm::SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received { peer_id, info })) =>{
                    if local_peer_id != peer_id && info.protocol_version != *PROTOCOL {
                        tracing::info!("Disconnection from {} Wrong Protocol",peer_id);
                        swarm.disconnect_peer_id(peer_id).unwrap_or_else(|_| panic!("failed to disconnect peer {peer_id}"));

                    }
                },
                libp2p::swarm::SwarmEvent::NewListenAddr {address ,..}=>{
                    let listener_address = address.with_p2p(*swarm.local_peer_id()).unwrap();
                    tracing::info!(%listener_address);
                    *my_addr = listener_address;
                },
                libp2p::swarm::SwarmEvent::ConnectionEstablished{peer_id,endpoint,..} =>{
                    match endpoint {
                        libp2p::core::ConnectedPoint::Dialer { address,.. } => {
                            tracing::info!("Successfully dialed to {peer_id}: {address}");
                        }
                        libp2p::core::ConnectedPoint::Listener {send_back_addr,.. } => {
                            tracing::info!("Successfully received dial from {peer_id}: {send_back_addr}");
                            tracing::info!("Dialing back...");
                            swarm.dial(send_back_addr).unwrap();

                        },
                    }
                },
                libp2p::swarm::SwarmEvent::ConnectionClosed { peer_id, connection_id,cause,.. } => {
                    tracing::info!("Connection to {peer_id}:{connection_id} closed reason:{cause:?}");
                }
                libp2p::swarm::SwarmEvent::ExternalAddrConfirmed { address } => {
                    tracing::info!("External address confirmed as {address}");
                    *my_addr = address;
                }
                ,libp2p::swarm::SwarmEvent::Dialing { peer_id, .. } => {
                    if let Some(peer_id) = peer_id {
                        tracing::info!("Dialing {peer_id}");
                    }
                }
                _ => {
                    tracing::info!("Unhandled event: {:?}", event);
                }
            }
        }
    }
}

async fn sender(
    key: Vec<u8>,
    port: u16,
    message: String,
    clients: Vec<String>,
) -> Result<String> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()
                .unwrap(),
        )
        .init();
    let len = clients.len();
    let key_pair = Keypair::from_protobuf_encoding(&key).unwrap();
    let local_peer_id = PeerId::from_public_key(&key_pair.public());
    let mut swarm = SwarmBuilder::with_existing_identity(key_pair)
        .with_tokio()
        .with_quic()
        .with_behaviour(|key| Behaviour {
            identify: {
                let cfg = identify::Config::new(PROTOCOL.to_string(), key.public())
                    .with_push_listen_addr_updates(true)
                    .with_agent_version(AGENT_VERSION.to_string());
                identify::Behaviour::new(cfg)
            },
            request_response: {
                request_response::cbor::Behaviour::<RequestResponse, RequestResponse>::new(
                    [(StreamProtocol::new(PROTOCOL), ProtocolSupport::Full)],
                    request_response::Config::default().with_max_concurrent_streams(1),
                )
            },
        })
        .unwrap()
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    swarm
        .listen_on(
            format!("/ip4/0.0.0.0/udp/{port}/quic-v1")
                .parse()
                .unwrap(),
        )
        .unwrap();
    let mut peers = Box::pin(BTreeMap::<PeerId, Multiaddr>::new());
    let mut out_id_to_addr = BTreeMap::new();
    let mut result = BTreeMap::new();
    let mut my_addr = Box::pin(Multiaddr::empty());
    for client in clients.into_iter() {
        let addr = client.parse::<Multiaddr>().unwrap();
        if let Some(Protocol::P2p(peer_id)) = addr.iter().last() {
            peers.insert(peer_id, addr.clone());
            swarm.dial(addr).unwrap();
        };
    }


    loop {
        select! {
            _ = async{},if(!peers.is_empty()) => {
                let connected: Vec<PeerId> = swarm.connected_peers().cloned().collect();
                if !connected.is_empty() {
                    for peer_id in &connected {
                        match peers.get(peer_id).cloned() {
                            Some(address) => {
                                let out_id = swarm.behaviour_mut()
                                    .request_response
                                    .send_request(peer_id, RequestResponse {message:{
                                        if message.is_empty(){
                                            my_addr.to_string()
                                        }
                                        else {
                                            message.clone()
                                        }
                                    }});
                                out_id_to_addr.insert(out_id,address);
                                peers.remove(peer_id);
                            }
                            None => {
                                println!("Peer {peer_id} not in list of peers");
                            }
                        }
                    }
                }
            }
            _ = async{}, if (peers.is_empty() && result.len() == len) =>{
                let result = serde_json::to_string(&result).unwrap();
                tracing::info!(%result);
                return Ok(result);
            }
            event = swarm.select_next_some() => match event {
                libp2p::swarm::SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(request_response::Event::Message{message,..}))=>{
                    match message {libp2p_request_response::Message::Response{response,request_id}=>{
                        let response:RequestResponse = response;
                        let key = out_id_to_addr.get(&request_id).unwrap().to_owned();
                        result.insert(key,response);
                        }
                        libp2p_request_response::Message::Request { .. } => {} }
                }

                libp2p::swarm::SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received { peer_id, info })) =>{
                    if local_peer_id != peer_id && info.protocol_version != *PROTOCOL {
                        tracing::info!("Disconnecting {} :: Wrong Protocol",peer_id);
                        swarm.disconnect_peer_id(peer_id).unwrap_or_else(|_| panic!("failed to disconnect peer {peer_id}"));

                    }
                },
                libp2p::swarm::SwarmEvent::NewListenAddr {address ,..}=>{
                    let listener_address = address.with_p2p(*swarm.local_peer_id()).unwrap();
                    tracing::info!(%listener_address);
                    *my_addr = listener_address;
                },
                libp2p::swarm::SwarmEvent::ConnectionEstablished{peer_id,endpoint,..} =>{
                    match endpoint {
                        libp2p::core::ConnectedPoint::Dialer { address,.. } => {
                            tracing::info!("Successfully dialed to {peer_id}: {address}");

                        }
                        libp2p::core::ConnectedPoint::Listener {send_back_addr,.. } => {
                            tracing::info!("Successfully received dial from {peer_id}: {send_back_addr}");
                            tracing::info!("Dialing back...");

                        },
                    }
                },
                libp2p::swarm::SwarmEvent::ConnectionClosed { peer_id, connection_id, cause,.. } => {
                    
                    tracing::info!("Connection to {peer_id}:{connection_id} closed reason:{cause:?}");
                }
                libp2p::swarm::SwarmEvent::ExternalAddrConfirmed { address } => {
                    tracing::info!("External address confirmed as {address}");
                    *my_addr = address;
                },
                libp2p::swarm::SwarmEvent::Dialing { peer_id, .. } => {
                    if let Some(peer_id) = peer_id {
                        tracing::info!("Dialing {peer_id}");
                    }
                },
                _ => {
                    tracing::info!("Unhandled event: {:?}", event);
                }
            }
        }
    }
}
/// Formats the sum of two numbers as string.
#[pyfunction]
fn receive(key: Vec<u8>, port: u16, n: u32, func: PyObject, py: Python) -> PyResult<&PyAny> {
    pyo3_asyncio::tokio::future_into_py(py, async move {
        receiver(key, port, n, func).await;
        Ok(())
    })
}

#[pyfunction]
fn send(
    key: Vec<u8>,
    port: u16,
    message: String,
    clients: Vec<String>,
    py: Python,
) -> PyResult<&PyAny> {
    pyo3_asyncio::tokio::future_into_py(py, async move {
        match sender(key, port, message, clients).await {
            Ok(s) => Ok(s),
            Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
        }
    })
}

/// A Python module implemented in Rust.
#[pymodule]
fn network(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(receive, m)?)?;
    m.add_function(wrap_pyfunction!(get_key, m)?)?;
    m.add_function(wrap_pyfunction!(send, m)?)?;
    Ok(())
}