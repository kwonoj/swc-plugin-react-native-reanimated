use swc_plugin::{ast::*, plugin_transform, TransformPluginProgramMetadata};
use swc_reanimated_worklets_visitor::{create_worklets_visitor};

#[plugin_transform]
pub fn process(program: Program, _metadata: TransformPluginProgramMetadata) -> Program {
    let mut visitor = create_worklets_visitor();

    program.fold_with(&mut visitor)
}
