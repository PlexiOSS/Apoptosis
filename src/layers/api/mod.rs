pub mod server;
pub mod extractors;
pub mod public_api;

use std::rc::Rc;
use khronos_runtime::rt::{RuntimeCreateOpts, KhronosRuntime};
use crate::service::layer::{DispatchLayerResult, Layer, LayerData, NewLayerOpts, SharedLayerData};
use crate::service::axum::Axum;
use crate::service::sharedlayer::SharedLayer;
use crate::service::vfs::get_luau_vfs;


#[derive(Clone)]
pub struct ApiLayer {
    vm: Rc<KhronosRuntime>,
    layer_data: LayerData<Self>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct ApiLayerConfig {
    addr: String,
}

impl Layer for ApiLayer {
    type Message = ();
    type LayerData = SharedLayerData<ApiLayer, Axum>;
    type Config = ApiLayerConfig;

    fn name() -> &'static str {
        "apilayer"
    }

    async fn new(opts: NewLayerOpts<Self>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let shared = SharedLayer::new(opts.pool, opts.diesel);
        let vm = Self::setup_vm(RuntimeCreateOpts::default(), get_luau_vfs(), None).await?;

        let sl = SharedLayerData::new(
            opts.config, 
            Axum::new(server::create(shared.clone())),
            shared
        );

        let layer_data = Self::create_layer_data(sl, &vm)
        .map_err(|e| format!("Failed to create layer data: {e}"))?;

        // TODO: Actually spin up API sercer

        Ok(Self {
            layer_data,
            vm: Rc::new(vm),
        })
    }

    async fn dispatch(&self, msg: Self::Message) -> DispatchLayerResult {
        Self::dispatch_to_vm_serde(&self.vm, self.layer_data.clone(), msg, "./api").await
    }
}