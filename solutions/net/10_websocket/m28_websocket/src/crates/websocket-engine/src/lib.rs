mod assembler;
mod base64;
mod client;
mod close;
mod frame;
mod handshake;
mod stream;

pub use assembler::{Assembler, Event, Message};
pub use base64::{Base64Error, decode as base64_decode, encode as base64_encode};
pub use client::{client_request, connect, new_key, verify_response};
pub use close::{Closing, Phase, encode_close, parse_close, valid_close_code};
pub use frame::{Decoder, Frame, MaskGen, Opcode, ProtoError, Role, close_code, encode_frame};
pub use handshake::{WsConfig, accept_key, check_upgrade, upgrade};
pub use stream::{WsError, WsStream};
