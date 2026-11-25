use bevy::prelude::*;
use bevy_mod_scripting::bindings::{
    InteropError, WorldExtensions,
    function::{namespace::NamespaceBuilder, script_function::FunctionCallContext},
    script_value::ScriptValue,
};

use crate::dom::node::NodeSpec;
use crate::dom::{DomNodeMeta, DomOp, DomRuntime, spec_tree};
use crate::state::MenuCmd;

fn push_menu_cmd(ctx: &FunctionCallContext, cmd: MenuCmd) -> Result<(), InteropError> {
    ctx.world()?
        .with_resource_mut(|mut writer: Mut<Messages<MenuCmd>>| {
            writer.write(cmd);
        })
}

fn push_dom_op(ctx: &FunctionCallContext, op: DomOp) -> Result<(), InteropError> {
    ctx.world()?
        .with_resource_mut(|mut rt: Mut<DomRuntime>| rt.ops.push(op))
}

fn queue_script_node(
    ctx: &FunctionCallContext,
    parent_id: i64,
    build: impl FnOnce(&mut DomRuntime, Entity) -> ScriptValue,
) -> Result<ScriptValue, InteropError> {
    let world = ctx.world()?;
    let Some(menu_root) = world.with_resource(|rt: &DomRuntime| rt.index.owner(parent_id))? else {
        return Ok(ScriptValue::Unit);
    };
    world.with_resource_mut(|mut rt: Mut<DomRuntime>| build(&mut *rt, menu_root))
}

pub(crate) fn register_menu_namespaces(app: &mut App) {
    NamespaceBuilder::in_namespace(app.world_mut(), "menus")
        .register("open", |ctx: FunctionCallContext, name: String| {
            push_menu_cmd(&ctx, MenuCmd::Open(name))
        })
        .register("close", |ctx: FunctionCallContext, name: String| {
            push_menu_cmd(&ctx, MenuCmd::Close(Some(name)))
        })
        .register("close_top", |ctx: FunctionCallContext| {
            push_menu_cmd(&ctx, MenuCmd::Close(None))
        });

    NamespaceBuilder::in_namespace(app.world_mut(), "dom")
        .register(
            "get_by_id",
            |ctx: FunctionCallContext, scope_id: i64, id: String| -> Result<ScriptValue, InteropError> {
                ctx.world()?.with_resource(|rt: &DomRuntime| {
                    rt.find_by_html_id(scope_id, &id)
                        .map_or(ScriptValue::Unit, ScriptValue::Integer)
                })
            },
        )
        .register(
            "create",
            |ctx: FunctionCallContext,
             parent_id: i64,
             tag: String,
             opts: ScriptValue|
             -> Result<ScriptValue, InteropError> {
                let spec = NodeSpec::from_script_opts(tag, opts);
                queue_script_node(&ctx, parent_id, move |rt, menu_root| {
                    ScriptValue::Integer(rt.queue_node(menu_root, parent_id, spec))
                })
            },
        )
        .register(
            "html",
            |ctx: FunctionCallContext,
             parent_id: i64,
             html: String|
             -> Result<ScriptValue, InteropError> {
                let doc = roxmltree::Document::parse(&html)
                    .map_err(|e| InteropError::external_boxed(Box::new(e)))?;
                let tree = spec_tree(doc.root_element());
                queue_script_node(&ctx, parent_id, move |rt, menu_root| {
                    ScriptValue::Integer(rt.queue_spec_tree(menu_root, parent_id, &tree))
                })
            },
        )
        .register("clear", |ctx: FunctionCallContext, id: i64| {
            push_dom_op(&ctx, DomOp::Clear(id))
        })
        .register("remove", |ctx: FunctionCallContext, id: i64| {
            push_dom_op(&ctx, DomOp::Remove(id))
        })
        .register("set_text", |ctx: FunctionCallContext, id: i64, text: String| {
            push_dom_op(&ctx, DomOp::SetText { id, text })
        })
        .register("set_value", |ctx: FunctionCallContext, id: i64, text: String| {
            push_dom_op(&ctx, DomOp::SetValue { id, text })
        })
        .register("add_class", |ctx: FunctionCallContext, id: i64, cls: String| {
            push_dom_op(&ctx, DomOp::SetClass { id, cls, add: true })
        })
        .register("remove_class", |ctx: FunctionCallContext, id: i64, cls: String| {
            push_dom_op(&ctx, DomOp::SetClass { id, cls, add: false })
        })
        .register(
            "has_class",
            |ctx: FunctionCallContext, id: i64, cls: String| -> Result<bool, InteropError> {
                let world = ctx.world()?;
                let Some(entity) = world.with_resource(|rt: &DomRuntime| rt.index.entity(id))?
                else {
                    return Ok(false);
                };
                world.with_component::<DomNodeMeta, _, _>(entity, |meta| {
                    meta.is_some_and(|meta| meta.classes.iter().any(|c| c == &cls))
                })
            },
        )
        .register(
            "on",
            |ctx: FunctionCallContext, id: i64, event: String, handler: String| {
                push_dom_op(&ctx, DomOp::BindEvent { id, event, handler })
            },
        )
        .register("focus", |ctx: FunctionCallContext, id: i64| {
            push_dom_op(&ctx, DomOp::Focus(id))
        })
        .register("blur", |ctx: FunctionCallContext| {
            push_dom_op(&ctx, DomOp::Blur)
        });

    #[cfg(feature = "clipboard")]
    register_clipboard_namespace(app);
}

#[cfg(feature = "clipboard")]
fn register_clipboard_namespace(app: &mut App) {
    NamespaceBuilder::in_namespace(app.world_mut(), "clipboard")
        .register("get", |_ctx: FunctionCallContext| -> ScriptValue {
            match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
                Ok(text) => ScriptValue::String(text.into()),
                Err(_) => ScriptValue::String("".into()),
            }
        })
        .register("set", |_ctx: FunctionCallContext, text: String| -> bool {
            arboard::Clipboard::new()
                .and_then(|mut c| c.set_text(text))
                .is_ok()
        });
}
