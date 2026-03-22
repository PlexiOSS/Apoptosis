use axum::Router;
use khronos_runtime::rt::mlua_scheduler::LuaSchedulerAsyncUserData;
use khronos_runtime::rt::mluau::prelude::*;
use tokio::net::TcpListener;
use std::thread::Builder;

// Userdata that is for spawning axum from within luau
#[derive(Clone)]
pub struct Axum {
    router: axum::routing::IntoMakeService<Router>,
}

impl Axum {
    pub fn new(router: axum::routing::IntoMakeService<Router>) -> Self {
        Self { router }
    }
}

impl LuaUserData for Axum {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_scheduler_async_method("spawn", async |_lua, this, addr: String| {
            let listener = TcpListener::bind(&addr).await.map_err(|e| LuaError::external(e.to_string()))?;
            let router = this.router.clone();

            Builder::new().spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    axum::serve(listener, router).await.unwrap();
                })
            })
            .map_err(|e| LuaError::external(e.to_string()))?;
            
            Ok(())
        });
    }
}