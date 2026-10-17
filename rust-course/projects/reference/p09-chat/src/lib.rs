//! A length-prefixed, bounded, local teaching chat service. No authentication/TLS.
use std::{io, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpListener,
    sync::{Semaphore, broadcast, watch},
    task::JoinSet,
    time::timeout,
};
pub const MAX_FRAME: usize = 1024;
/// A four-byte big-endian length followed by UTF-8 bytes. Empty frames are invalid.
pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<String> {
    let len = reader.read_u32().await? as usize;
    if len == 0 || len > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame length outside 1..=1024",
        ));
    }
    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes).await?;
    let text = String::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid UTF-8"))?;
    if text.chars().any(|c| c.is_control()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "control character",
        ));
    }
    Ok(text)
}
pub async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, text: &str) -> io::Result<()> {
    if text.is_empty() || text.len() > MAX_FRAME || text.chars().any(|c| c.is_control()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid frame length",
        ));
    }
    writer.write_u32(text.len() as u32).await?;
    writer.write_all(text.as_bytes()).await?;
    writer.flush().await
}
/// At most 32 clients and 32 retained messages. A lagging client is disconnected.
/// Shutdown aborts connection tasks: the protocol has no durable delivery promise.
/// A true watch value or closure of all senders requests shutdown; false is not a stop.
pub async fn serve(listener: TcpListener, mut stop: watch::Receiver<bool>) -> io::Result<()> {
    let permits = Arc::new(Semaphore::new(32));
    let (bus, _) = broadcast::channel::<String>(32);
    let mut clients = JoinSet::new();
    let result = loop {
        // Observe the state before waiting: a receiver may already hold true.
        if *stop.borrow_and_update() {
            break Ok(());
        }
        tokio::select! {
            changed=stop.changed()=>{if changed.is_err(){break Ok(());}},
            Some(_)=clients.join_next(), if !clients.is_empty()=>{},
            incoming=listener.accept()=> {
                let (stream,_) = match incoming {
                    Ok(connection) => connection,
                    Err(error) => break Err(error),
                };
                let Ok(permit)=permits.clone().try_acquire_owned() else { drop(stream); continue; };
                let sender=bus.clone(); let mut messages=bus.subscribe();
                clients.spawn(async move {
                    let _permit=permit;
                    let (mut read,mut write)=stream.into_split();
                    // Dedicated reader owns partially consumed frames across writes.
                    let mut reader=Box::pin(async move {
                        loop { let text=timeout(Duration::from_secs(30),read_frame(&mut read)).await.map_err(|_|io::Error::new(io::ErrorKind::TimedOut,"read deadline"))??;
                            let _=sender.send(text);
                        }
                        #[allow(unreachable_code)] Ok::<(),io::Error>(())
                    });
                    loop { tokio::select! {
                        _=&mut reader=>break,
                        message=messages.recv()=> {
                            let Ok(text)=message else { break; };
                            if !matches!(timeout(Duration::from_secs(2),write_frame(&mut write,&text)).await,Ok(Ok(()))) {break;}
                        }
                    }}
                });
            }
        }
    };
    clients.abort_all();
    while clients.join_next().await.is_some() {}
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn malformed_text_and_empty_frames_are_rejected() {
        for fixture in [vec![0, 0, 0, 0], vec![0, 0, 0, 1, 255], vec![0, 0, 0, 1, 0]] {
            assert_eq!(
                read_frame(&mut fixture.as_slice())
                    .await
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData
            );
        }
        let mut output = Vec::new();
        assert!(write_frame(&mut output, "line\nbreak").await.is_err());
        assert!(output.is_empty());
    }
    #[tokio::test]
    async fn fragmented_frame_and_utf8() {
        let (mut tx, mut rx) = tokio::io::duplex(64);
        let task = tokio::spawn(async move {
            for b in [0, 0, 0, 2, 0xc3, 0xa9] {
                tx.write_all(&[b]).await.unwrap();
                tokio::task::yield_now().await;
            }
        });
        assert_eq!(read_frame(&mut rx).await.unwrap(), "é");
        task.await.unwrap();
    }
    #[tokio::test]
    async fn coalesced_frames_obey_the_exact_byte_boundary() {
        let maximum = "é".repeat(MAX_FRAME / 2);
        assert_eq!(maximum.len(), MAX_FRAME);
        let mut wire = Vec::new();
        write_frame(&mut wire, &maximum).await.unwrap();
        write_frame(&mut wire, "after").await.unwrap();
        let mut input = wire.as_slice();
        assert_eq!(read_frame(&mut input).await.unwrap(), maximum);
        assert_eq!(read_frame(&mut input).await.unwrap(), "after");
        assert!(input.is_empty());

        let mut output = vec![99];
        assert_eq!(
            write_frame(&mut output, &format!("{maximum}x"))
                .await
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(output, [99]); // Validation precedes even the prefix write.
        let header = (MAX_FRAME as u32 + 1).to_be_bytes();
        assert_eq!(
            read_frame(&mut header.as_slice()).await.unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    #[tokio::test]
    async fn retaining_the_same_future_preserves_partial_prefix_progress() {
        use std::{future::Future, task::Poll};
        let (mut tx, mut rx) = tokio::io::duplex(8);
        tx.write_all(&[0, 0]).await.unwrap();
        let mut frame = Box::pin(read_frame(&mut rx));
        std::future::poll_fn(|context| {
            assert!(frame.as_mut().poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        // Other work can finish while the pinned decoder retains its first two
        // bytes. This checks the ownership pattern, not a timed socket schedule.
        tx.write_all(&[0, 2, 0xc3, 0xa9]).await.unwrap();
        assert_eq!(frame.await.unwrap(), "é");
    }
    #[tokio::test]
    async fn oversized_rejected_before_body() {
        let mut bytes = &[0, 0, 8, 0][..];
        assert_eq!(
            read_frame(&mut bytes).await.unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    #[tokio::test]
    async fn truncated_body_fails() {
        let mut bytes = &[0, 0, 0, 3, b'a'][..];
        assert_eq!(
            read_frame(&mut bytes).await.unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
    #[tokio::test]
    async fn lag_is_visible() {
        let (tx, mut rx) = broadcast::channel(2);
        for n in 0..3 {
            tx.send(n).unwrap();
        }
        assert!(matches!(
            rx.recv().await,
            Err(broadcast::error::RecvError::Lagged(1))
        ));
    }
    #[tokio::test]
    async fn loopback_and_shutdown() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = watch::channel(false);
        let server = tokio::spawn(serve(listener, rx));
        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        write_frame(&mut client, "hello").await.unwrap();
        assert_eq!(
            timeout(Duration::from_secs(2), read_frame(&mut client))
                .await
                .unwrap()
                .unwrap(),
            "hello"
        );
        tx.send(true).unwrap();
        server.await.unwrap().unwrap();
    }
    #[tokio::test]
    async fn shutdown_already_requested_does_not_wait_for_a_second_change() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (_sender, receiver) = watch::channel(true);
        timeout(Duration::from_secs(2), serve(listener, receiver))
            .await
            .expect("an already-requested stop must complete")
            .unwrap();
    }
    #[tokio::test]
    async fn false_notifications_do_not_stop_and_sender_drop_does() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, receiver) = watch::channel(false);
        let server = tokio::spawn(serve(listener, receiver));
        let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
        for text in ["before", "after"] {
            sender.send(false).unwrap();
            write_frame(&mut client, text).await.unwrap();
            assert_eq!(
                timeout(Duration::from_secs(2), read_frame(&mut client))
                    .await
                    .unwrap()
                    .unwrap(),
                text
            );
        }
        drop(sender);
        timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}
