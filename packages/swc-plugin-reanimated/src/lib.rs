use swc_plugin::{ast::*, plugin_transform, TransformPluginProgramMetadata};
use swc_reanimated_worklets_visitor::create_worklets_visitor;

#[plugin_transform]
pub fn process(program: Program, _metadata: TransformPluginProgramMetadata) -> Program {
    let visitor = create_worklets_visitor(None);

    program.fold_with(&mut as_folder(visitor))
}
