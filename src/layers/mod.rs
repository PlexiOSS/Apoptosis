pub mod sample;
pub mod api;

#[derive(Clone)]
pub struct DummyData {}
impl khronos_runtime::rt::mluau::UserData for DummyData {}

#[macro_export]
/// Macro to initialize a layer
macro_rules! layer_init {
    ($opts:ident) => {
        {
            use crate::layers::DummyData;
            let shared = SharedLayer::new($opts.pool, $opts.diesel);
            let vm = Self::setup_vm(RuntimeCreateOpts::default(), get_luau_vfs(), None).await?;

            let layer_data = Self::create_layer_data(SharedLayerData::new($opts.config, DummyData {}, shared), &vm)
            .map_err(|e| format!("Failed to create layer data: {e}"))?;

            Ok(Self {
                layer_data,
                vm: Rc::new(vm),
            })
        }
    };
}

/// Macro to create a layer
#[macro_export]
macro_rules! layer {
    // Entry point using default dispatch implementation
    ($(#[$attr:meta])* $name:ident = ( $mod:ident, $id:literal, $msg_type:ty, $config_type:ty, $entrypoint:literal ) ) => {
        $crate::layer! {
            @impl
            $(#[$attr])*
            $name = ( $mod, $id, $msg_type, $config_type, async |self_ref, msg| {
                Self::dispatch_to_vm_serde(&self_ref.vm, self_ref.layer_data.clone(), msg, $entrypoint).await
            })
        }
    };

    // Entry point with custom dispatch code
    ($(#[$attr:meta])* $name:ident = ( $mod:ident, $id:literal, $msg_type:ty, $config_type:ty, $entrypoint:literal, async |$self:ident, $msg:ident| $code:block ) ) => {
        $crate::layer! {
            @impl
            $(#[$attr])*
            $name = ( $mod, $id, $msg_type, $config_type, async |$self, $msg| $code )
        }
    };

    (@impl 
        $(#[$attr:meta])* $name:ident = ( $mod:ident, $id:literal, $msg_type:ty, $config_type:ty, async |$self:ident, $msg:ident| $code:expr ) 
    ) => {
        pub mod $mod {
            use super::{$msg_type, $config_type};
            use std::rc::Rc;
            use crate::service::{layer::{DispatchLayerResult, Layer, LayerData, SharedLayerData, NewLayerOpts}, sharedlayer::SharedLayer, vfs::get_luau_vfs};
            use khronos_runtime::rt::{runtime::KhronosRuntime, RuntimeCreateOpts};
            use crate::layers::DummyData;

            #[derive(Clone)]
            $(#[$attr])*
            pub struct $name {
                vm: Rc<KhronosRuntime>,
                layer_data: LayerData<Self>,
            }

            impl Layer for $name {
                type Message = $msg_type;
                type LayerData = SharedLayerData<Self, DummyData>;
                type Config = $config_type;

                fn name() -> &'static str {
                    $id
                }

                async fn new(opts: NewLayerOpts<Self>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
                    $crate::layer_init!(opts)
                }

                async fn dispatch(&self, msg: Self::Message) -> DispatchLayerResult {
                    let action = async move |$self: &$name, $msg: $msg_type| -> DispatchLayerResult {
                        $code
                    };
                    
                    action(self, msg).await
                }
            }
        }
    };
}