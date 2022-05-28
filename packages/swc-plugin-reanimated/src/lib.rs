use swc_plugin::{ast::*, plugin_transform, TransformPluginProgramMetadata};
use swc_reanimated_worklets_visitor::ReanimatedWorkletsVisitor;

#[plugin_transform]
pub fn process(program: Program, _metadata: TransformPluginProgramMetadata) -> Program {
    let visitor = ReanimatedWorkletsVisitor;

    program.fold_with(&mut as_folder(visitor))
}
