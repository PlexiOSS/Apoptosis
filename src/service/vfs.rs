use khronos_runtime::mluau_require::rust_embed;
use khronos_runtime::mluau_require::Embed;

#[derive(Embed, Debug)]
#[folder = "$CARGO_MANIFEST_DIR/src/luau"]
#[prefix = ""]
pub struct LuauBase;

pub fn get_luau_vfs() -> khronos_runtime::mluau_require::vfs::EmbeddedFS<LuauBase> {
    khronos_runtime::mluau_require::vfs::EmbeddedFS::<LuauBase>::new()
}