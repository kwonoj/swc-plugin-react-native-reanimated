use std::path::PathBuf;

use serde_json::Value;
use swc_plugin::{ast::*, plugin_transform, source_map::FileName, TransformPluginProgramMetadata};
use swc_reanimated_worklets_visitor::{create_worklets_visitor, WorkletsOptions};

#[plugin_transform]
pub fn process(program: Program, metadata: TransformPluginProgramMetadata) -> Program {
    let context: Value = serde_json::from_str(&metadata.transform_context)
        .expect("Should able to deserialize context");
    let filename = if let Some(filename) = (&context["filename"]).as_str() {
        FileName::Real(PathBuf::from(filename))
    } else {
        FileName::Anon
    };

    let visitor = create_worklets_visitor(
        WorkletsOptions::new(None, filename),
        std::sync::Arc::new(metadata.source_map),
    );

    program.fold_with(&mut as_folder(visitor))
}
