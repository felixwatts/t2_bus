use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle};

use crate::{stopper::{BasicStopper, MultiStopper}, transport::Transport, err::BusResult};

use super::{core::Core, ProtocolClient, ProtocolServer, Task};

pub (crate) trait Listener: 'static + Send {
    fn accept(&mut self) -> impl std::future::Future<Output = BusResult<impl Transport<ProtocolServer, ProtocolClient>>> + std::marker::Send;
}

pub(crate) fn listen_and_serve(listener: impl Listener) -> BusResult<MultiStopper> {
    let mut core = Core::new();
    let listen_stopper = listen(listener, core.get_task_sender())?;
    let core_stopper = core.spawn()?;
    let stopper = MultiStopper::new(vec![listen_stopper, core_stopper]);
    Ok(stopper)
}

pub(crate) fn listen(
    mut listener: impl Listener,
    register_channel: UnboundedSender<Task>,
) -> BusResult<BasicStopper> {
    let (stop_sender, mut stop_receiver) = tokio::sync::oneshot::channel();

    let join_handle: JoinHandle<BusResult<()>> = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = &mut stop_receiver => break,
                accepted = listener.accept() => {
                    let client_transport = accepted?;
                    register_channel.send(Task::Register(Box::new(client_transport)))?;
                }
            }
        }
        Ok(())
    });

    Ok(BasicStopper::new(stop_sender, join_handle))
}