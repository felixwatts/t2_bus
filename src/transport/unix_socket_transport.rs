use crate::{
    protocol::{Msg, ProtocolClient, ProtocolServer},
    server::Task,
    err::BusResult,
};
use std::path::PathBuf;

use tokio::{
    net::{UnixListener, UnixStream},
    sync::mpsc::UnboundedSender,
    task::JoinHandle,
};
use tokio_util::codec::Framed;

use super::{cbor_codec::CborCodec, Transport};

pub(crate) async fn connect(
    addr: &PathBuf,
) -> BusResult<Framed<UnixStream, CborCodec<Msg<ProtocolClient>, Msg<ProtocolServer>>>> {
    let socket = UnixStream::connect(addr).await?;
    let transport = tokio_util::codec::Framed::new(socket, CborCodec::new());
    Ok(transport)
}

pub struct UnixBusStopper{
    core_stop_sender: tokio::sync::oneshot::Sender<()>, 
    core_join_handle: JoinHandle::<BusResult::<()>>, 
    listener_stop_sender: tokio::sync::oneshot::Sender<()>, 
    listener_join_handle: JoinHandle::<BusResult::<()>>
}

impl UnixBusStopper{
    pub async fn stop(self) -> BusResult<(BusResult<()>, BusResult<()>)> {
        let _ = self.core_stop_sender.send(());
        let _ = self.listener_stop_sender.send(());
        let (core_result, listener_result) = tokio::join![self.core_join_handle, self.listener_join_handle];
        Ok((core_result?, listener_result?))
    }

    pub async fn join(self)  -> BusResult<(BusResult<()>, BusResult<()>)> {
        let (core_result, listener_result) = tokio::join![self.core_join_handle, self.listener_join_handle];
        Ok((core_result?, listener_result?))
    }
}

pub fn listen_and_serve(addr: &PathBuf) -> BusResult<UnixBusStopper> {
    let mut core = crate::server::Core::new();
    let (listener_stop_sender, listener_join_handle) =
        listen(&addr, core.get_task_sender())?;
    let (core_stop_sender, core_join_handle) = core.spawn()?;
    let stopper = UnixBusStopper{
        core_stop_sender,
        core_join_handle,
        listener_stop_sender,
        listener_join_handle
    };
    Ok(stopper)
}

pub(crate) fn listen(
    addr: &PathBuf,
    register_channel: UnboundedSender<Task>,
) -> BusResult<(tokio::sync::oneshot::Sender<()>, JoinHandle<BusResult<()>>)> {
    let _ = std::fs::remove_file(addr);
    let listener = UnixListener::bind(addr)?;
    let (stop_sender, mut stop_receiver) = tokio::sync::oneshot::channel();

    let join_handle: JoinHandle<BusResult<()>> = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut stop_receiver => break,
                accepted = listener.accept() => {
                    let (socket, _) = accepted?;
                    let client_transport = tokio_util::codec::Framed::new(socket, CborCodec::new());
                    register_channel.send(Task::Register(Box::new(client_transport)))?;
                }
            }
        }
        Ok(())
    });

    Ok((stop_sender, join_handle))
}

impl<TSend, TRecv> Transport<TSend, TRecv>
    for tokio_util::codec::Framed<UnixStream, CborCodec<Msg<TSend>, Msg<TRecv>>>
where
    TSend: 'static + serde::Serialize + Send,
    TRecv: 'static + serde::de::DeserializeOwned + Send,
{
}
