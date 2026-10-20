//! Hardware-service connection and feature negotiation, taken from the
//! built-in renderer (MIT, see `LICENSE-t1bridge`) where it is private.

use std::collections::BTreeMap;
use std::fmt;
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::thread;
use std::time::Duration;

use t1_platform::seqpacket::{self, SeqPacketClient, SeqPacketError};
use t1_touchbar_hw::digitizer::DisplayDimensions;
use t1_touchbar_hw::packet::MAX_PACKET_LENGTH;
use t1_touchbar_hw::wire::{
    self, AncillaryMetadata, ClientMessage, Envelope, ErrorCode, FeatureBits, Hello, HelloAck,
    ServiceMessage,
};

pub const MAX_PENDING_REQUESTS: usize = 64;
const SEND_ATTEMPTS: usize = 64;
const SEND_RETRY_DELAY: Duration = Duration::from_millis(2);
const RECEIVE_RETRY_DELAY: Duration = Duration::from_millis(5);

/// Static failure from one hardware-service connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RendererError {
    ConnectionUnavailable,
    Transport,
    Protocol,
    Negotiation,
    Frame,
    Gesture,
    ServiceRejected,
}

impl fmt::Display for RendererError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ConnectionUnavailable => "Touch Bar hardware service is unavailable",
            Self::Transport => "Touch Bar hardware connection was lost",
            Self::Protocol => "Touch Bar hardware protocol failed",
            Self::Negotiation => "Touch Bar display negotiation failed",
            Self::Frame => "Touch Bar frame setup failed",
            Self::Gesture => "Touch Bar input sequence failed",
            Self::ServiceRejected => "Touch Bar hardware request was rejected",
        })
    }
}

impl std::error::Error for RendererError {}

pub struct HardwareConnection {
    descriptor: OwnedFd,
    receive_buffer: Vec<u8>,
}

impl HardwareConnection {
    fn new(descriptor: OwnedFd) -> Self {
        Self {
            descriptor,
            receive_buffer: vec![0; MAX_PACKET_LENGTH],
        }
    }

    fn client(&self) -> SeqPacketClient<'_> {
        SeqPacketClient::new(self.descriptor.as_fd())
    }

    /// # Errors
    ///
    /// Fails when encoding fails or the socket stays full or closes.
    pub fn send_request(
        &self,
        envelope: &Envelope<ClientMessage>,
        dimensions: Option<DisplayDimensions>,
        descriptor: Option<BorrowedFd<'_>>,
    ) -> Result<(), RendererError> {
        let packet =
            wire::encode_client(envelope, dimensions).map_err(|_| RendererError::Protocol)?;
        for _ in 0..SEND_ATTEMPTS {
            match self.client().send_with_fd(&packet, descriptor) {
                Ok(()) => return Ok(()),
                Err(SeqPacketError::WouldBlock | SeqPacketError::Interrupted) => {
                    thread::sleep(SEND_RETRY_DELAY);
                }
                Err(_) => return Err(RendererError::Transport),
            }
        }
        Err(RendererError::Transport)
    }

    fn receive_service(
        &mut self,
        dimensions: Option<DisplayDimensions>,
    ) -> Result<Envelope<ServiceMessage>, RendererError> {
        loop {
            if let Some(envelope) = self.try_receive_service(dimensions)? {
                return Ok(envelope);
            }
            thread::sleep(RECEIVE_RETRY_DELAY);
        }
    }

    /// # Errors
    ///
    /// Fails on a closed socket or an undecodable packet.
    pub fn try_receive_service(
        &mut self,
        dimensions: Option<DisplayDimensions>,
    ) -> Result<Option<Envelope<ServiceMessage>>, RendererError> {
        let client = SeqPacketClient::new(self.descriptor.as_fd());
        match client.receive(&mut self.receive_buffer) {
            Ok(length) => wire::decode_service(
                &self.receive_buffer[..length],
                AncillaryMetadata::default(),
                dimensions,
            )
            .map(Some)
            .map_err(|_| RendererError::Protocol),
            Err(SeqPacketError::WouldBlock | SeqPacketError::Interrupted) => Ok(None),
            Err(_) => Err(RendererError::Transport),
        }
    }
}

pub struct NegotiatedConnection {
    pub connection: HardwareConnection,
    pub request_ids: RequestIds,
    pub ack: HelloAck,
    pub features: FeatureBits,
}

enum HelloOutcome {
    Accepted(NegotiatedConnection),
    Unsupported,
}

/// Connects with every optional feature the service supports.
///
/// # Errors
///
/// Fails when the service is unavailable or rejects the base features.
pub fn negotiate() -> Result<NegotiatedConnection, RendererError> {
    let base = FeatureBits::INITIAL_SERVICE | FeatureBits::TOUCH_ID_CANCELLATION;
    let optional = FeatureBits::DISPLAY_BRIGHTNESS | FeatureBits::KEYBOARD_BACKLIGHT;
    if let HelloOutcome::Accepted(connection) = connect_and_hello(base | optional)? {
        return Ok(connection);
    }
    let mut supported = base;
    for feature in [
        FeatureBits::DISPLAY_BRIGHTNESS,
        FeatureBits::KEYBOARD_BACKLIGHT,
    ] {
        if matches!(connect_and_hello(base | feature)?, HelloOutcome::Accepted(_)) {
            supported = supported | feature;
        }
    }
    match connect_and_hello(supported)? {
        HelloOutcome::Accepted(connection) => Ok(connection),
        HelloOutcome::Unsupported => Err(RendererError::Negotiation),
    }
}

fn connect_and_hello(features: FeatureBits) -> Result<HelloOutcome, RendererError> {
    let descriptor =
        seqpacket::connect_touchbar().map_err(|_| RendererError::ConnectionUnavailable)?;
    let mut connection = HardwareConnection::new(descriptor);
    let mut request_ids = RequestIds::new();
    let request_id = request_ids.next(&BTreeMap::<u32, ()>::new())?;
    connection.send_request(
        &Envelope {
            request_id,
            message: ClientMessage::Hello(Hello {
                minor: wire::PROTOCOL_MINOR,
                required_features: features,
            }),
        },
        None,
        None,
    )?;
    let response = connection.receive_service(None)?;
    if response.request_id != request_id {
        return Err(RendererError::Protocol);
    }
    match response.message {
        ServiceMessage::HelloAck(ack) => Ok(HelloOutcome::Accepted(NegotiatedConnection {
            connection,
            request_ids,
            ack,
            features,
        })),
        ServiceMessage::Error(ErrorCode::UnsupportedFeature) => Ok(HelloOutcome::Unsupported),
        ServiceMessage::Error(_) => Err(RendererError::ServiceRejected),
        _ => Err(RendererError::Protocol),
    }
}

/// Non-zero request identifiers that skip ones still pending.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestIds {
    next: u32,
}

impl RequestIds {
    const fn new() -> Self {
        Self { next: 1 }
    }

    /// # Errors
    ///
    /// Fails when every candidate in one sweep is still pending.
    pub fn next<V>(&mut self, pending: &BTreeMap<u32, V>) -> Result<u32, RendererError> {
        for _ in 0..=MAX_PENDING_REQUESTS {
            let candidate = self.next;
            self.next = self.next.wrapping_add(1);
            if self.next == 0 {
                self.next = 1;
            }
            if candidate != 0 && !pending.contains_key(&candidate) {
                return Ok(candidate);
            }
        }
        Err(RendererError::Protocol)
    }
}
