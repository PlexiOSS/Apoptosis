use kittycat::perms as kittycat_perms;
use khronos_runtime::rt::mluau::prelude::*;

use super::optional_value::OptionalValue;

pub struct Permission {
    perm: kittycat_perms::Permission,

    // Cache any computed fields here
    namespace_cache: OptionalValue<LuaString>,
    perm_cache: OptionalValue<LuaString>,
    perm_repr_cache: OptionalValue<LuaString>,
}

impl From<kittycat_perms::Permission> for Permission {
    fn from(perm: kittycat_perms::Permission) -> Self {
        Self {
            perm,
            namespace_cache: OptionalValue::new(),
            perm_cache: OptionalValue::new(),
            perm_repr_cache: OptionalValue::new(),
        }
    }
}

impl LuaUserData for Permission {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("namespace", |lua, this| {
            this.namespace_cache
                .get_failable(|| lua.create_string(&this.perm.namespace))
        });

        fields.add_field_method_get("perm", |lua, this| {
            this.perm_cache
                .get_failable(|| lua.create_string(&this.perm.perm))
        });

        fields.add_field_method_get("negator", |_lua, this| Ok(this.perm.negator));
    }

    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(LuaMetaMethod::ToString, |lua, this, ()| {
            this.perm_repr_cache
                .get_failable(|| lua.create_string(this.perm.to_string()))
        });
    }
}

pub struct PartialStaffPosition {
    position: kittycat_perms::PartialStaffPosition,

    // Cache any computed fields here
    id_cache: OptionalValue<LuaString>,
    perms_cache: OptionalValue<LuaTable>,
}

impl From<kittycat_perms::PartialStaffPosition> for PartialStaffPosition {
    fn from(position: kittycat_perms::PartialStaffPosition) -> Self {
        Self {
            position,
            id_cache: OptionalValue::new(),
            perms_cache: OptionalValue::new(),
        }
    }
}

impl LuaUserData for PartialStaffPosition {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("id", |lua, this| {
            this.id_cache
                .get_failable(|| lua.create_string(&this.position.id))
        });
        fields.add_field_method_get("index", |_lua, this| Ok(this.position.index));

        fields.add_field_method_get("perms", |lua, this| {
            this.perms_cache.get_failable(|| {
                let table = lua.create_table()?;
                for perm in &this.position.perms {
                    let perm_userdata = Permission::from(perm.clone());
                    table.raw_push(perm_userdata)?;
                }
                table.set_readonly(true);
                Ok(table)
            })
        });
    }
}

pub struct StaffPermissions {
    perms: kittycat_perms::StaffPermissions,

    // Cache any computed fields here
    user_positions_cache: OptionalValue<LuaTable>,
    perm_overrides_cache: OptionalValue<LuaTable>,
    resolved_perms_cache: OptionalValue<LuaAnyUserData>,
}

impl From<kittycat_perms::StaffPermissions> for StaffPermissions {
    fn from(perms: kittycat_perms::StaffPermissions) -> Self {
        Self {
            perms,
            user_positions_cache: OptionalValue::new(),
            perm_overrides_cache: OptionalValue::new(),
            resolved_perms_cache: OptionalValue::new(),
        }
    }
}

impl LuaUserData for StaffPermissions {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("user_positions", |lua, this| {
            this.user_positions_cache.get_failable(|| {
                let table = lua.create_table()?;
                for position in &this.perms.user_positions {
                    let position_userdata = PartialStaffPosition::from(position.clone());
                    table.raw_push(position_userdata)?;
                }
                table.set_readonly(true);
                Ok(table)
            })
        });

        fields.add_field_method_get("perm_overrides", |lua, this| {
            this.perm_overrides_cache.get_failable(|| {
                let table = lua.create_table()?;
                for perm in &this.perms.perm_overrides {
                    let perm_userdata = Permission::from(perm.clone());
                    table.raw_push(perm_userdata)?;
                }
                table.set_readonly(true);
                Ok(table)
            })
        });
    }

    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("resolve", |lua, this, _: ()| {
            this.resolved_perms_cache.get_failable(|| {
                let resolved = this.perms.resolve();
                let ud = lua.create_userdata(ResolvedPermissions::from(resolved))?;
                Ok(ud)
            })
        });
    }
}

pub struct ResolvedPermissions {
    perms: Vec<kittycat_perms::Permission>,

    // Cache any computed fields here
    perms_list_cache: OptionalValue<LuaTable>,
}

impl From<Vec<kittycat_perms::Permission>> for ResolvedPermissions {
    fn from(perms: Vec<kittycat_perms::Permission>) -> Self {
        Self {
            perms,
            perms_list_cache: OptionalValue::new(),
        }
    }
}

impl LuaUserData for ResolvedPermissions {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("hasperm", |_lua, this, perm: LuaUserDataRef<Permission>| {
            Ok(kittycat_perms::has_perm(&this.perms, &perm.perm))
        });
    }

    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("perms", |lua, this| {
            this.perms_list_cache.get_failable(|| {
                let table = lua.create_table()?;
                for perm in &this.perms {
                    let perm_userdata = Permission::from(perm.clone());
                    table.raw_push(perm_userdata)?;
                }
                table.set_readonly(true);
                Ok(table)
            })
        });
    }
}

struct CheckPatchChangesError(kittycat_perms::CheckPatchChangesError);

impl IntoLua for CheckPatchChangesError {
    fn into_lua(self, lua: &Lua) -> LuaResult<LuaValue> {
        let tab = lua.create_table()?;

        match self.0 {
            kittycat_perms::CheckPatchChangesError::NoPermission { permission } => {
                tab.set("type", "NoPermission")?;
                let perm_userdata = Permission::from(permission);
                tab.set("permission", perm_userdata)?;
            }
            kittycat_perms::CheckPatchChangesError::LacksNegatorForWildcard {
                wildcard,
                negator,
            } => {
                tab.set("type", "LacksNegatorForWildcard")?;
                let wildcard_userdata = Permission::from(wildcard);
                let negator_userdata = Permission::from(negator);
                tab.set("wildcard", wildcard_userdata)?;
                tab.set("negator", negator_userdata)?;
            }
        }

        tab.set_readonly(true);

        Ok(LuaValue::Table(tab))
    }
}

#[allow(dead_code)]
/// Creates the base kittycat table for Luau side
pub fn kittycat_base_tab(lua: &Lua) -> LuaResult<LuaTable> {
    let tab = lua.create_table()?;

    tab.set(
        "permissionfromstring",
        lua.create_function(|_lua, perm_str: String| {
            let perm = kittycat_perms::Permission::from_string(&perm_str);
            Ok(Permission::from(perm))
        })?,
    )?;

    tab.set(
        "haspermstr",
        lua.create_function(|_lua, (perms, perm): (Vec<String>, String)| {
            Ok(kittycat_perms::has_perm_str(&perms, &perm))
        })?,
    )?;

    tab.set(
        "checkpatchchanges",
        lua.create_function(
            |lua,
             (manager_perms, current_perms, new_perms): (
                LuaUserDataRef<ResolvedPermissions>,
                LuaUserDataRef<ResolvedPermissions>,
                LuaUserDataRef<ResolvedPermissions>,
            )| {
                match kittycat_perms::check_patch_changes(
                    &manager_perms.perms,
                    &current_perms.perms,
                    &new_perms.perms,
                ) {
                    Ok(_) => Ok((true, LuaValue::Nil)),
                    Err(err) => {
                        let err = CheckPatchChangesError(err).into_lua(lua)?;
                        Ok((false, err))
                    }
                }
            },
        )?,
    )?;

    tab.set_readonly(true);

    Ok(tab)
}
