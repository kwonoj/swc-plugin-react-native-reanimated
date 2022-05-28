use swc_ecma_transforms_compat::{
    es2015::{arrow, shorthand, template_literal},
    es2020::{nullish_coalescing, optional_chaining},
};
use swc_ecmascript::{
    ast::*,
    visit::{as_folder, VisitMut},
};
use swc_visit::chain;

struct ReanimatedWorkletsVisitor;

// TODO: this mimics existing plugin behavior runs specific transform pass
// before running actual visitor.
// 1. This may not required
// 2. If required, need to way to pass config to visitors instead of Default::default()
// https://github.com/software-mansion/react-native-reanimated/blob/b4ee4ea9a1f246c461dd1819c6f3d48440a25756/plugin.js#L367-L371=
impl VisitMut for ReanimatedWorkletsVisitor {
    fn visit_mut_call_expr(&mut self, call_expr: &mut CallExpr) {}

    fn visit_mut_fn_decl(&mut self, fn_decl: &mut FnDecl) {}

    fn visit_mut_fn_expr(&mut self, fn_expr: &mut FnExpr) {}

    fn visit_mut_arrow_expr(&mut self, arrow_expr: &mut ArrowExpr) {}
}

pub fn create_worklets_visitor() -> impl VisitMut {
    // allows adding custom globals such as host-functions
    /*
    if (this.opts != null && Array.isArray(this.opts.globals)) {
        this.opts.globals.forEach((name) => {
          globals.add(name);
        });
    }
    */

    ReanimatedWorkletsVisitor
}
