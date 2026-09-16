pub mod mock_vm_service;
pub mod vm_service_client;

pub use mock_vm_service::MockVmServiceAdapter;
pub use vm_service_client::WebSocketVmServiceAdapter;
