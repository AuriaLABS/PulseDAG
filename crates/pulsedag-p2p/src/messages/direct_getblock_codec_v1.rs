//! Strictly size-bounded raw-byte request-response codec for future native-v3
//! GetBlock substreams. This codec does not parse or authorize blocks.
//! The Swarm must authenticate PeerId and the upper layer must invoke
//! direct_getblock_v1's strict chain/peer/hash/ID validators before admission.
//! NOT yet registered in a live Swarm. No legacy Gossipsub fallback.

use std::io;

use libp2p::futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use libp2p::{request_response, StreamProtocol};

use super::direct_getblock_v1::{
    DIRECT_GETBLOCK_REQUEST_MAX_BYTES, DIRECT_GETBLOCK_RESPONSE_MAX_BYTES,
};

pub const DIRECT_GETBLOCK_STREAM_PROTOCOL_V1: &str = "/pulsedag/v3/getblock/1.0.0";

#[derive(Debug, Clone, Default)]
pub struct DirectGetBlockRawCodecV1;

async fn read_bounded<T>(io: &mut T, max: usize) -> io::Result<Vec<u8>>
where
    T: AsyncRead + Unpin + Send,
{
    // Limit *before allocating or parsing*. A malicious peer may not send a
    // giant JSON body and rely on later serde validation to reject it.
    let mut raw = Vec::new();
    io.take(max as u64 + 1).read_to_end(&mut raw).await?;
    if raw.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "empty GetBlock frame",
        ));
    }
    if raw.len() > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "oversized GetBlock frame",
        ));
    }
    Ok(raw)
}

async fn write_bounded<T>(io: &mut T, bytes: Vec<u8>, max: usize) -> io::Result<()>
where
    T: AsyncWrite + Unpin + Send,
{
    if bytes.is_empty() || bytes.len() > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid GetBlock frame length",
        ));
    }
    io.write_all(&bytes).await?;
    io.close().await
}

impl request_response::Codec for DirectGetBlockRawCodecV1 {
    type Protocol = StreamProtocol;
    type Request = Vec<u8>;
    type Response = Vec<u8>;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_bounded(io, DIRECT_GETBLOCK_REQUEST_MAX_BYTES).await
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        read_bounded(io, DIRECT_GETBLOCK_RESPONSE_MAX_BYTES).await
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        request: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_bounded(io, request, DIRECT_GETBLOCK_REQUEST_MAX_BYTES).await
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        response: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        write_bounded(io, response, DIRECT_GETBLOCK_RESPONSE_MAX_BYTES).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_codec_preserves_exact_bytes_without_skipping_unknown_json() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut codec = DirectGetBlockRawCodecV1;
            let proto = StreamProtocol::new(DIRECT_GETBLOCK_STREAM_PROTOCOL_V1);
            let request = br#"{"requested_peer_id":"peerB","foreign_nested":{"x":true}}"#.to_vec();
            let mut writer = libp2p::futures::io::Cursor::new(Vec::new());
            request_response::Codec::write_request(
                &mut codec,
                &proto,
                &mut writer,
                request.clone(),
            )
            .await
            .unwrap();
            let mut reader = libp2p::futures::io::Cursor::new(writer.into_inner());
            let decoded = request_response::Codec::read_request(&mut codec, &proto, &mut reader)
                .await
                .unwrap();
            assert_eq!(
                decoded, request,
                "codec must not silently drop unknown nested JSON"
            );
        });
    }

    #[test]
    fn rejects_empty_and_oversize_requests_and_responses() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut codec = DirectGetBlockRawCodecV1;
            let proto = StreamProtocol::new(DIRECT_GETBLOCK_STREAM_PROTOCOL_V1);
            for size in [0, DIRECT_GETBLOCK_REQUEST_MAX_BYTES + 1] {
                let mut writer = libp2p::futures::io::Cursor::new(Vec::new());
                let err = request_response::Codec::write_request(
                    &mut codec,
                    &proto,
                    &mut writer,
                    vec![42; size],
                )
                .await
                .unwrap_err();
                assert_eq!(err.kind(), io::ErrorKind::InvalidData);
                assert!(
                    writer.into_inner().is_empty(),
                    "reject before writing any bytes"
                );
            }
            for size in [0, DIRECT_GETBLOCK_RESPONSE_MAX_BYTES + 1] {
                let mut writer = libp2p::futures::io::Cursor::new(Vec::new());
                let err = request_response::Codec::write_response(
                    &mut codec,
                    &proto,
                    &mut writer,
                    vec![42; size],
                )
                .await
                .unwrap_err();
                assert_eq!(err.kind(), io::ErrorKind::InvalidData);
                assert!(writer.into_inner().is_empty());
            }
            let mut oversized_request =
                libp2p::futures::io::Cursor::new(vec![b'a'; DIRECT_GETBLOCK_REQUEST_MAX_BYTES + 1]);
            let err =
                request_response::Codec::read_request(&mut codec, &proto, &mut oversized_request)
                    .await
                    .unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidData);
            let mut oversized_response = libp2p::futures::io::Cursor::new(vec![
                b'a';
                DIRECT_GETBLOCK_RESPONSE_MAX_BYTES
                    + 1
            ]);
            let err =
                request_response::Codec::read_response(&mut codec, &proto, &mut oversized_response)
                    .await
                    .unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        });
    }
}
