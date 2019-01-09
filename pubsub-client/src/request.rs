use serde_json::{json, Value};

pub enum PubSubRequest {
    Account,
    Program,
    Signature,
}

impl PubSubRequest {
    pub fn build_request_json(&self, id: u64, params: Option<Value>) -> Value {
        let jsonrpc = "2.0";
        let method = match self {
            PubSubRequest::Account => "accountSubscribe",
            PubSubRequest::Program => "programSubscribe",
            PubSubRequest::Signature => "signatureSubscribe",
        };
        let mut request = json!({
           "jsonrpc": jsonrpc,
           "id": id,
           "method": method,
        });
        if let Some(param_string) = params {
            request["params"] = param_string;
        }
        request
    }
}
