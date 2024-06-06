use anyhow::Result;

use futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::multiaddr::Protocol;
use libp2p::request_response::ProtocolSupport;
use libp2p::swarm::NetworkBehaviour;
use libp2p::{identify, request_response, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder};

use port_selector::is_free;
use pyo3::prelude::*;
use serde_json::{json, Value};
use std::num::NonZeroU8;
use std::time::Duration;

use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use tokio::select;
/// agent version
const AGENT_VERSION: &str = "/ipfs/0.1.0";
const PROTOCOL: &str = "/connection/1";

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
#[pyfunction]
fn get_free_port() -> Option<u16> {
    (10000..=u16::MAX).find(|&port| is_free(port))
}
#[pyfunction]
fn log_info(message: String, value: String) {
    let sub = tracing_subscriber::FmtSubscriber::builder()
        .compact()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()
                .unwrap(),
        )
        .with_ansi(false)
        .finish();
    let _ = tracing::subscriber::set_global_default(sub);
    tracing::info!("{}:{}", message, value);
}
#[pyfunction]
fn get_peer_id(key: Vec<u8>) -> String {
    let key_pair = Keypair::from_protobuf_encoding(&key).unwrap();
    let local_peer_id = PeerId::from_public_key(&key_pair.public());
    local_peer_id.to_string()
}

#[pyfunction]
fn get_peer_id_from_address(address: String) -> String {
    if let Some(Protocol::P2p(peer_id)) = address.parse::<Multiaddr>().unwrap().iter().last() {
        return peer_id.to_string();
    }
    "".to_owned()
}

fn get_swarm(key_pair: Keypair) -> Swarm<Behaviour> {
    SwarmBuilder::with_existing_identity(key_pair)
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
                    request_response::Config::default()
                        .with_max_concurrent_streams(5)
                        .with_request_timeout(Duration::from_secs(120)),
                )
            },
        })
        .unwrap()
        .with_swarm_config(|c| {
            c.with_idle_connection_timeout(Duration::from_secs(60))
                .with_dial_concurrency_factor(NonZeroU8::new(5).unwrap())
        })
        .build()
}
async fn receiver(
    key: Vec<u8>,
    port: u16,
    mut n: u32,
    func: PyObject,
    inputs: Vec<String>,
    exclude: Vec<String>,
) -> Result<String> {
    let key_pair = Keypair::from_protobuf_encoding(&key).unwrap();
    let local_peer_id = PeerId::from_public_key(&key_pair.public());
    // file logging
    // let mut path = std::env::current_dir().unwrap();
    // path.push("logs");
    // if !std::path::Path::new(&path).exists() {
    //     std::fs::create_dir(&path).unwrap();
    // }
    // path.push(format!("{}.txt", local_peer_id));
    // let file = OpenOptions::new()
    //     .create(true)
    //     .append(true)
    //     .open(format!("./logs/{}.txt", local_peer_id))
    //     .expect("Unable to open log file");

    let sub = tracing_subscriber::FmtSubscriber::builder()
        .compact()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()
                .unwrap(),
        )
        .pretty()
        .with_ansi(true)
        .compact()
        .finish();
    let _ = tracing::subscriber::set_global_default(sub);

    let mut swarm = get_swarm(key_pair);

    swarm
        .listen_on(format!("/ip4/0.0.0.0/udp/{port}/quic-v1").parse().unwrap())
        .unwrap();

    let mut my_addr = Box::pin(Multiaddr::empty());
    let mut result = BTreeMap::new();

    loop {
        let event = swarm.select_next_some().await;
        match event {
            libp2p::swarm::SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(
                request_response::Event::Message { peer, message },
            )) => match message {
                libp2p::request_response::Message::Request {
                    request_id,
                    request,
                    channel,
                } => {
                    let req: RequestResponse = request;

                    tracing::info!(
                        "Request from {} with req_id {} :: {}",
                        peer,
                        request_id,
                        req.message
                    );
                    let mut calculated_message: HashMap<String, Value> = {
                        Python::with_gil(|py| -> PyResult<HashMap<String, Value>> {
                            let args = (req.message.clone(), inputs.clone());
                            let result = func.call1(py, args).unwrap();
                            Ok(serde_json::from_str(&result.to_string()).unwrap())
                        })
                        .unwrap()
                    };

                    result.insert(peer, calculated_message.clone());
                    let response: RequestResponse = RequestResponse {
                        message: {
                            exclude.iter().for_each(|f| {
                                calculated_message.remove(f);
                            });
                            serde_json::to_string(&calculated_message).unwrap()
                        },
                    };
                    tracing::info!("Responding with:{:?}", response);
                    swarm
                        .behaviour_mut()
                        .request_response
                        .send_response(channel, response)
                        .unwrap();
                    n -= 1;
                }
                libp2p::request_response::Message::Response { .. } => {}
            },
            libp2p::swarm::SwarmEvent::Behaviour(BehaviourEvent::Identify(
                identify::Event::Received { peer_id, info },
            )) => {
                if local_peer_id != peer_id && info.protocol_version != *PROTOCOL {
                    tracing::info!("Disconnection from {} Wrong Protocol", peer_id);
                    swarm
                        .disconnect_peer_id(peer_id)
                        .unwrap_or_else(|_| panic!("failed to disconnect peer {peer_id}"));
                }
            }
            libp2p::swarm::SwarmEvent::NewListenAddr { address, .. } => {
                let listener_address = address.with_p2p(*swarm.local_peer_id()).unwrap();
                tracing::info!(%listener_address);
                *my_addr = listener_address;
            }
            libp2p::swarm::SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => match endpoint {
                libp2p::core::ConnectedPoint::Dialer { address, .. } => {
                    tracing::info!("Successfully dialed to {peer_id}: {address}");
                }
                libp2p::core::ConnectedPoint::Listener { send_back_addr, .. } => {
                    tracing::info!("Successfully received dial from {peer_id}: {send_back_addr}");
                    tracing::info!("Dialing back...");
                    swarm.dial(send_back_addr).unwrap();
                }
            },
            libp2p::swarm::SwarmEvent::ConnectionClosed {
                peer_id,
                connection_id,
                ..
            } => {
                tracing::info!("Connection to {peer_id}:{connection_id} closed");
                if n == 0 && swarm.connected_peers().collect::<Vec<&PeerId>>().is_empty() {
                    let result = serde_json::to_string(&result).unwrap();
                    tracing::info!(%result);
                    return Ok(result);
                }
            }
            libp2p::swarm::SwarmEvent::ExternalAddrConfirmed { address } => {
                tracing::info!("External address confirmed as {address}");
                *my_addr = address;
            }
            libp2p::swarm::SwarmEvent::Dialing { peer_id, .. } => {
                let peer_id = peer_id.unwrap_or(PeerId::random());
                tracing::info!("Dialing {peer_id}");
            }
            _ => {}
        }
    }
}

async fn sender(
    key: Vec<u8>,
    port: u16,
    mut message: String,
    clients: Vec<String>,
) -> Result<String> {
    let key_pair = Keypair::from_protobuf_encoding(&key).unwrap();
    let local_peer_id = PeerId::from_public_key(&key_pair.public());
    // let mut path = std::env::current_dir().unwrap();
    // path.push("logs");
    // if !std::path::Path::new(&path).exists() {
    //     std::fs::create_dir(&path).unwrap();
    // }
    // path.push(format!("{}.txt", local_peer_id));
    // let file = OpenOptions::new()
    //     .create(true)
    //     .append(true)
    //     .open(format!("./logs/{}.txt", local_peer_id))
    //     .expect("Unable to open log file");

    let sub = tracing_subscriber::FmtSubscriber::builder()
        .compact()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()
                .unwrap(),
        )
        .pretty()
        .with_ansi(true)
        .compact()
        .finish();
    let _ = tracing::subscriber::set_global_default(sub);

    let len = clients.len();
    let mut swarm = get_swarm(key_pair);

    swarm
        .listen_on(format!("/ip4/0.0.0.0/udp/{port}/quic-v1").parse().unwrap())
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
            _ = async{},if !swarm.connected_peers().cloned().collect::<Vec<PeerId>>().is_empty() && !peers.is_empty()=>{

                    if let Some(client) = peers.pop_last(){



                        let json_message = serde_json::from_str::<Value>(&message).unwrap();
                        if json_message["protocol"] == "coordinate" && json_message.get("address").is_none() {
                            let mut value = Value::default();
                            value["protocol"] = json!("coordinate");
                            value["address"] = json!(my_addr.to_string());
                            message = serde_json::to_string(&value).unwrap();
                        }

                    if let Some(Protocol::P2p(peer_id)) = client.1.iter().last() {
                        if swarm.is_connected(&peer_id){
                            let out_id = swarm.behaviour_mut().request_response.send_request(&peer_id, RequestResponse{message:message.clone()});
                            out_id_to_addr.insert(out_id,client.1);
                        }else{
                            peers.insert(peer_id,client.1);
                        }
                    }


            };
            }

            event = swarm.select_next_some() => match event {
                libp2p::swarm::SwarmEvent::Behaviour(BehaviourEvent::RequestResponse(request_response::Event::Message{message,peer}))=>{
                    match message {libp2p::request_response::Message::Response{response,request_id}=>{
                        let response:RequestResponse = response;
                        let key = out_id_to_addr.get(&request_id).unwrap().to_owned();
                        result.insert(key,response);
                        swarm.disconnect_peer_id(peer).unwrap();
                        }
                        libp2p::request_response::Message::Request { .. } => {} }
                }

                libp2p::swarm::SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received { peer_id, info })) =>{
                    if local_peer_id != peer_id && info.protocol_version != *PROTOCOL {
                        tracing::error!("Disconnecting {} :: Wrong Protocol",peer_id);
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
                        },
                    }
                },
                libp2p::swarm::SwarmEvent::ConnectionClosed { peer_id, connection_id,.. } => {
                    tracing::info!("Connection to {peer_id}:{connection_id} closed");
                    if peers.is_empty() && result.len() == len{
                        let result = serde_json::to_string(&result).unwrap();
                        tracing::info!(%result);
                        return Ok(result);
                    }
                }
                libp2p::swarm::SwarmEvent::ExternalAddrConfirmed { address } => {
                    tracing::info!("External address confirmed as {address}");
                    *my_addr = address;
                }
                ,
                libp2p::swarm::SwarmEvent::Dialing { peer_id, .. } => {
                    let peer_id = peer_id.unwrap_or(PeerId::random());
                    tracing::info!("Dialing {peer_id}");

                },
                _ => {}
            }
        }
    }
}
/// Formats the sum of two numbers as string.
#[pyo3_async::pyfunction(allow_threads)]
async fn receive(
    key: Vec<u8>,
    port: u16,
    n: u32,
    func: PyObject,
    inputs: Vec<String>,
    exclude: Vec<String>,
) -> PyResult<String> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    match rt
        .spawn(receiver(key, port, n, func, inputs, exclude))
        .await
        .unwrap()
    {
        Ok(s) => Ok(s),
        Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
    }
}

#[pyo3_async::pyfunction(allow_threads)]
async fn send(key: Vec<u8>, port: u16, message: String, clients: Vec<String>) -> PyResult<String> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    match rt.spawn(sender(key, port, message, clients)).await.unwrap() {
        Ok(s) => Ok(s),
        Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
    }
}

/// A Python module implemented in Rust.
#[pymodule]
fn network_rs(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(async_receive, m)?)?;
    m.add_function(wrap_pyfunction!(get_key, m)?)?;
    m.add_function(wrap_pyfunction!(async_send, m)?)?;
    m.add_function(wrap_pyfunction!(get_free_port, m)?)?;
    m.add_function(wrap_pyfunction!(log_info, m)?)?;
    m.add_function(wrap_pyfunction!(get_peer_id, m)?)?;
    m.add_function(wrap_pyfunction!(get_peer_id_from_address, m)?)?;
    Ok(())
}
