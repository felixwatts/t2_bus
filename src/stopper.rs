

use futures::Future;
use tokio::task::JoinHandle;
use crate::prelude::{BusError, BusResult};

/// Represents a way to stop a concurrent process and to wait for it to stop. Returned by some functions that spawn concurrent processes.
pub trait Stopper{
    /// Send a stop signal to the process and then wait for it to terminate
    fn stop(self) -> impl Future<Output=BusResult<()>> + Send;
    /// Wait for the process to terminate without sending a stop signal
    fn join(self) -> impl Future<Output=BusResult<()>> + Send;
}

pub struct BasicStopper{
    stop_sender: tokio::sync::oneshot::Sender<()>, 
    join_handle: JoinHandle::<BusResult::<()>>, 
}

impl BasicStopper{
    pub(crate) fn new(stop_sender: tokio::sync::oneshot::Sender<()>, join_handle: JoinHandle::<BusResult::<()>>) -> Self{
        Self { stop_sender, join_handle }
    }
}

impl Stopper for BasicStopper{
    async fn stop(self) -> BusResult<()> {
        self.stop_sender.send(()).map_err(|_| BusError::ChannelClosed)?;
        self.join_handle.await??;
        Ok(())
    }

    async fn join(self) -> BusResult<()> {
        self.join_handle.await??;
        Ok(())
    }
}

pub struct MultiStopper{
    stoppers: Vec<BasicStopper>
}

impl MultiStopper{
    pub(crate) fn new(stoppers: Vec<BasicStopper>) -> Self{
        Self { stoppers }
    }
}

impl Stopper for MultiStopper{
    async fn stop(self) -> BusResult<()> {
        let stop_futures = self.stoppers.into_iter().map(|s| s.stop()).collect::<Vec<_>>();
        let stop_results = futures::future::join_all(stop_futures.into_iter()).await;
        let stop_result: Result<Vec<()>, BusError> = stop_results.into_iter().collect(); 
        stop_result?;
        Ok(())
    }

    async fn join(self) -> BusResult<()> {
        let join_futures = self.stoppers.into_iter().map(|s| s.join()).collect::<Vec<_>>();
        let join_results = futures::future::join_all(join_futures.into_iter()).await;
        let join_result: Result<Vec<()>, BusError> = join_results.into_iter().collect(); 
        join_result?;
        Ok(())
    }
}