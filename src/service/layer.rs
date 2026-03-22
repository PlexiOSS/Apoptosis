use khronos_runtime::rt::mlua_scheduler::LuaSchedulerAsyncUserData;
use khronos_runtime::rt::mluau::prelude::*;
use serde::{Serialize, de::DeserializeOwned};
use std::rc::Rc;
use tokio::task::spawn_local;
use tokio::{
    runtime::LocalOptions,
    select,
    sync::{
        mpsc::{UnboundedReceiver, UnboundedSender},
        oneshot::{Receiver as OneshotReceiver, Sender as OneshotSender, channel},
    },
};
use tokio_util::sync::CancellationToken;
use khronos_runtime::rt::{
    runtime::OnBrokenFunc, RuntimeCreateOpts, KhronosRuntime
};
use crate::service::kittycat::kittycat_base_tab;
use crate::service::optional_value::OptionalValue;
use crate::service::sharedlayer::{LuaSharedLayer, SharedLayer};

pub type DispatchLayerResult = Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone)]
/// A wrapper around layer data to be passed to VMs
pub struct LayerData<L: Layer> {
    data: Rc<L::LayerData>,
    value: LuaValue,
}

#[allow(dead_code)]
impl<L: Layer> LayerData<L> {
    pub fn new(layer_data: L::LayerData, lua: &Lua) -> LuaResult<Self> {
        let value = layer_data.clone().into_lua(lua)?;
        Ok(Self {
            data: Rc::new(layer_data),
            value,
        })
    }

    pub fn data(&self) -> &L::LayerData {
        &self.data
    }
}

/// A layer configuration wrapper for ergonomic handling of layer configs
/// 
/// Can be optionally used as a ergonomic wrapper around layer configs
#[derive(Clone)]
pub struct LayerConfig<L: Layer> {
    config: L::Config,
    config_cache: Rc<OptionalValue<LuaValue>>,
}

#[allow(dead_code)]
impl<L: Layer> LayerConfig<L> {
    /// Creates a new LayerConfig
    pub fn new(config: L::Config) -> Self {
        Self {
            config,
            config_cache: Rc::new(OptionalValue::new()),
        }
    }

    pub fn config(&self) -> &L::Config {
        &self.config
    }

    /// Converts the config into a LuaValue, caching the result
    pub fn to_lua_value(&self, lua: &Lua) -> LuaResult<LuaValue> {
        self.config_cache.get_failable(|| {
            match lua.to_value(&self.config)? {
                LuaValue::Table(t) => {
                    t.set_readonly(true);
                    Ok(LuaValue::Table(t))
                },
                other => Ok(other),
            }
        })
    }
}

/// Data passed to layer::new()
pub struct NewLayerOpts<L: Layer> {
    pub config: L::Config,
    pub pool: sqlx::PgPool,
    pub diesel: crate::Db,
}

/// A layer provides a specific service within Omniplex/IBL
#[allow(dead_code)]
pub trait Layer: Clone + Sized + 'static {
    type Message: Serialize + DeserializeOwned + Send + Default + 'static;

    /// The data type passed to the layer's VMs
    type LayerData: Clone + IntoLua + 'static;

    /// The configuration type for the layer
    type Config: Serialize + DeserializeOwned + Send + 'static;

    /// Returns the layer name
    fn name() -> &'static str;

    /// Creates a new layer
    async fn new(cfg: NewLayerOpts<Self>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>>;

    /// Dispatches a message to the layer
    async fn dispatch(&self, msg: Self::Message) -> DispatchLayerResult;

    /// Cleans up the layer
    async fn cleanup(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }

    // Pre-provided helpers

    /// Create a LayerData object for this layer
    fn create_layer_data(
        layer_data: Self::LayerData,
        vm: &KhronosRuntime
    ) -> LuaResult<LayerData<Self>> {
        vm.with_lua(|lua| LayerData::new(layer_data, lua))
    }

    /// Set up the VM for this layer
    ///
    /// Can be called within new() etc
    async fn setup_vm<FS>(
        opts: RuntimeCreateOpts,
        vfs: FS,
        on_broken: Option<OnBrokenFunc>,
    ) -> Result<KhronosRuntime, Box<dyn std::error::Error + Send + Sync>> 
    where
        FS: khronos_runtime::mluau_require::vfs::FileSystem + 'static,
    {
        let vm = KhronosRuntime::new(opts, None::<(fn(&Lua, LuaThread) -> Result<(), LuaError>, fn(LuaLightUserData) -> ())>, vfs, "omniplex-rust")
            .map_err(|e| format!("Failed to create VM for layer {}: {}", Self::name(), e))?;

        if let Some(on_broken) = on_broken {
            vm.set_on_broken(on_broken);
        } else {
            vm.set_on_broken(Box::new(|| {
                log::error!("VM for layer {} has broken", Self::name());
            }));
        }

        vm.with_lua(|lua| {
            lua.register_module("@omniplex-rust/kittycat", kittycat_base_tab(lua)?)
        })
        .map_err(|e| format!("Failed to create VM for layer {}: {}", Self::name(), e))?;

        Ok(vm)
    }

    /// Dispatches a message to a VM at the given path
    async fn dispatch_to_vm<A>(
        vm: &KhronosRuntime,
        path: &str,
        layer_data: LayerData<Self>,
        msg: Self::Message,
    ) -> LuaResult<A> 
    where
        A: FromLua
    {
        let ctx: Context<Self> = Context::new(layer_data, msg);
        let func = vm.eval_script::<LuaFunction>(path)?;
        let res: A = vm.call_in_scheduler(func, ctx).await?;
        Ok(res)
    }

    /// Same as dispatch_to_vm but with return as a serde type
    async fn dispatch_to_vm_serde<T>(
        vm: &KhronosRuntime,
        layer_data: LayerData<Self>,
        msg: Self::Message,
        entrypoint: &str,
    ) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
    where
        T: serde::de::DeserializeOwned,
    {
        let res = Self::dispatch_to_vm::<LuaValue>(vm, entrypoint, layer_data, msg)
            .await
            .map_err(|e| format!("{e}"))?;

        let value = vm.from_value(res)
            .map_err(|e| format!("Failed to deserialize response from layer VM: {e}"))?;

        Ok(value)
    }

    /// Load layer in its own thread
    fn load(opts: NewLayerOpts<Self>) -> LayerThread<Self> {
        LayerThread::new(opts)
    }
}

/// A LayerThread provides a dedicated thread for a specific IBL apoptosis layer
#[allow(dead_code)]
#[derive(Clone)]
pub struct LayerThread<L: Layer> {
    tx: UnboundedSender<(L::Message, OneshotSender<DispatchLayerResult>)>,
    cancellation_token: CancellationToken,
}

#[allow(dead_code)]
impl<L: Layer> LayerThread<L> {
    /// Creates a new VmThread
    pub fn new(opts: NewLayerOpts<L>) -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let cancellation_token = CancellationToken::new();
        let ct_clone = cancellation_token.clone();

        std::thread::Builder::new()
            .name(format!("LayerThread-{}", std::any::type_name::<L>()))
            .spawn(move || {
                Self::thread(opts, ct_clone, rx);
            })
            .expect("Failed to spawn VM thread");

        Self {
            tx,
            cancellation_token,
        }
    }

    /// thread function
    fn thread(
        opts: NewLayerOpts<L>,
        cancellation_token: CancellationToken,
        mut rx: UnboundedReceiver<(L::Message, OneshotSender<DispatchLayerResult>)>,
    ) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build_local(LocalOptions::default())
            .unwrap();

        rt.block_on(async move {
            let layer = Rc::new(L::new(opts).await.expect("Failed to create layer"));

            loop {
                select! {
                    Some(msg) = rx.recv() => {
                        let layer_ref = layer.clone();
                        spawn_local(async move {
                            let (msg, tx) = msg;
                            let result = layer_ref.dispatch(msg).await;
                            let _ = tx.send(result);
                        });
                    }
                    _ = cancellation_token.cancelled() => {
                        match layer.cleanup().await {
                            Ok(_) => {},
                            Err(e) => log::error!("Error during layer cleanup: {e}"),
                        }
                        return;
                    }
                }
            }
        });
    }

    pub async fn dispatch(&self, msg: L::Message) -> DispatchLayerResult {
        let (tx, rx): (
            OneshotSender<DispatchLayerResult>,
            OneshotReceiver<DispatchLayerResult>,
        ) = channel();

        self.tx
            .send((msg, tx))
            .map_err(|e| format!("Failed to send message to layer thread: {e}"))?;

        match rx.await {
            Ok(result) => result,
            Err(e) => Err(format!("Failed to receive response from layer thread: {e}").into()),
        }
    }

    fn cancel(&self) {
        self.cancellation_token.cancel();
    }
}

impl<L: Layer> LuaUserData for LayerThread<L> {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_scheduler_async_method("Dispatch", |lua, this, msg: LuaValue| async move {
            let msg: L::Message = lua
                .from_value(msg)
                .map_err(|e| LuaError::external(format!("Failed to deserialize message: {e}")))?;

            let result = this
                .dispatch(msg)
                .await
                .map_err(|e| LuaError::external(format!("Layer dispatch error: {e}")))?;

            let lua_result = lua
                .to_value(&result)
                .map_err(|e| LuaError::external(format!("Failed to serialize result: {e}")))?;

            Ok(lua_result)
        });
    }
}

/// A context for an event
pub struct Context<L: Layer> {
    layer_data: LayerData<L>,
    event: OptionalValue<L::Message>,
    event_cache: OptionalValue<LuaValue>,
}

impl<L: Layer> Context<L> {
    /// Creates a new context
    pub fn new(layer_data: LayerData<L>, event: L::Message) -> Self {
        Self {
            layer_data,
            event: OptionalValue::with(event),
            event_cache: OptionalValue::new(),
        }
    }
}

impl<L: Layer> LuaUserData for Context<L> {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("layer", |_lua, this| Ok(this.layer_data.value.clone()));

        fields.add_field_method_get("event", |lua, this| {
            this.event_cache.get_failable(|| {
                let event = this
                    .event
                    .take()
                    .ok_or("Event should be set")
                    .map_err(LuaError::external)?;
                let value = lua.to_value(&event)?;
                Ok(value)
            })
        });
    }
}

/// Helper struct to hold shared layer data
#[derive(Clone)]
pub struct SharedLayerData<T, U> 
where T: Layer,
    T::Config: Clone,
    U: LuaUserData + Clone + 'static
{
    pub cfg: LayerConfig<T>,
    pub layer_data: U,
    pub layer_data_ud: Rc<OptionalValue<LuaAnyUserData>>,
    pub shared: SharedLayer,
    shared_layer_ud: Rc<OptionalValue<LuaAnyUserData>>,
}

impl<T, U> SharedLayerData<T, U> 
where T: Layer,
    T::Config: Clone,
    U: LuaUserData + Clone + 'static
{
    pub fn new(config: T::Config, data: U, shared: SharedLayer) -> Self {
        Self {
            cfg: LayerConfig::new(config),
            layer_data: data,
            layer_data_ud: Rc::new(OptionalValue::new()),
            shared,
            shared_layer_ud: Rc::new(OptionalValue::new()),
        }
    }
}

impl<T, U> LuaUserData for SharedLayerData<T, U> 
where T: Layer, T::Config: Clone, U: LuaUserData + Clone + 'static 
{
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("Shared", |lua, this| {
            this.shared_layer_ud
                .get_failable(|| lua.create_userdata(LuaSharedLayer::new(this.shared.clone())))
        });

        fields.add_field_method_get("Config", |lua, this| {
            this.cfg.to_lua_value(lua)
        });

        fields.add_field_method_get("Data", |lua, this| {
            this.layer_data_ud
                .get_failable(|| lua.create_userdata(this.layer_data.clone()))
        });
    }
}
