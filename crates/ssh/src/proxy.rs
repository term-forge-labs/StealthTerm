// SSH ProxyJump support placeholder
use crate::config::ProxyJump;

pub struct ProxyConnection;

impl ProxyConnection {
    pub async fn connect_through(_proxy: &ProxyJump) -> Result<(), String> {
        // TODO: implement multi-hop SSH tunneling
        Err("ProxyJump not yet implemented".to_string())
    }
}
