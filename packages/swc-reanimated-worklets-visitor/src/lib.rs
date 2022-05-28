use swc_ecma_transforms_compat::{
    es2015::{arrow, shorthand, template_literal},
    es2020::{nullish_coalescing, optional_chaining},
};
use swc_ecmascript::visit::{VisitMut, Fold, as_folder};
use swc_visit::chain;

struct ReanimatedWorkletsVisitor;

impl VisitMut for ReanimatedWorkletsVisitor {}

pub fn create_worklets_visitor() -> impl Fold {
    // TODO: this is mimic existing plugin behavior runs specific transform pass
    // before running actual visitor.
    // 1. This may not required
    // 2. If required, need to way to pass config to visitors instead of Default::default()
    // https://github.com/software-mansion/react-native-reanimated/blob/b4ee4ea9a1f246c461dd1819c6f3d48440a25756/plugin.js#L367-L371=
    chain!(
        shorthand(),
        arrow(),
        optional_chaining(Default::default()),
        nullish_coalescing(Default::default()),
        template_literal(Default::default()),
        as_folder(ReanimatedWorkletsVisitor)
    )
}
