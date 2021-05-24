use super::Task;
use crate::{
    protocol::{Msg, ProtocolClient, ProtocolServer},
    transport::Transport,
    err::*,
};
use futures::SinkExt;
use futures::StreamExt;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub(crate) struct ClientStub {
    id: u32,
    transport: Box<dyn Transport<ProtocolServer, ProtocolClient>>,
    task_sender: UnboundedSender<Task>,
    protocol_server_receiver: UnboundedReceiver<Msg<ProtocolServer>>,
}

impl ClientStub {
    pub fn new(
        id: u32,
        transport: Box<dyn Transport<ProtocolServer, ProtocolClient>>,
        task_sender: UnboundedSender<Task>,
    ) -> BusResult<(ClientStub, UnboundedSender<Msg<ProtocolServer>>)> {
        let (protocol_server_sender, protocol_server_receiver) = unbounded_channel();

        Ok((
            ClientStub {
                id,
                transport,
                task_sender,
                protocol_server_receiver,
            },
            protocol_server_sender,
        ))
    }

    pub async fn serve(mut self) -> BusResult<()> {
        let result = self._serve().await;
        self.task_sender.send(Task::Deregister(self.id))?;
        result
    }

    async fn _serve(&mut self) -> BusResult<()> {
        loop {
            tokio::select! {
                // message from  client
                msg = self.transport.next() => {
                    let msg = msg.ok_or(BusError::ChannelClosed)??;
                    let task = Task::Message(self.id, msg);
                    self.task_sender.send(task)?
                },

                // message from core
                msg_option = self.protocol_server_receiver.recv() => {
                    match msg_option {
                        Some(msg) => {
                            self.transport.send(msg).await?;
                        },
                        None => return Ok(())
                    }
                }
            }
        }
    }
}
