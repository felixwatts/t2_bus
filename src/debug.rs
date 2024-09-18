use crate::protocol::{Msg, ProtocolClient, ProtocolServer};

pub fn server_msg_to_string(msg: &Msg<ProtocolServer>) -> String {
    let content = match &msg.content {
        ProtocolServer::Pub(params) => format!("PUB   {} ({})", &params.topic, &params.payload),
        ProtocolServer::Req(params) => format!("REQ   {} ({})", &params.topic, &params.payload),
        ProtocolServer::Rsp(params) => format!("RSP   {} ({})", &params.req_id, &params.payload),
        ProtocolServer::Ack(params) => format!(
            "ACK   {} {}",
            &params.msg_id,
            params.err.as_ref().map(|e| e.to_string()).unwrap_or("".to_string())
        ),
    };
    format!("{:>6} {}", &msg.id, &content)
}

pub fn client_msg_to_string(msg: &Msg<ProtocolClient>) -> String {
    let content = match &msg.content {
        ProtocolClient::Sub(params) => format!("SUB   {}", &params.topic),
        ProtocolClient::Unsub(params) => format!("UNSUB {}", &params.topic),
        ProtocolClient::Pub(params) => format!("PUB   {} ({})", &params.topic, &params.payload),
        ProtocolClient::Srv(params) => format!("SRV   {}", &params.topic),
        ProtocolClient::Unsrv(params) => format!("UNSRV {}", &params.topic),
        ProtocolClient::Req(params) => format!("REQ   {} ({})", &params.topic, &params.payload),
        ProtocolClient::Rsp(params) => format!("RSP   #{} ({})", &params.req_id, &params.payload),
        ProtocolClient::Stop => "STOP".to_string(),
        ProtocolClient::KeepAlive => "KEEP_ALIVE".to_string(),

    };
    format!("{:>6} {}", &msg.id, &content)
}