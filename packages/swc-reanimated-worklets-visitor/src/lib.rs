mod constants;
use crate::constants::GLOBALS;
use swc_ecma_transforms_compat::{
    es2015::{arrow, shorthand, template_literal},
    es2020::{nullish_coalescing, optional_chaining},
};
use swc_ecmascript::{
    ast::*,
    visit::{as_folder, VisitMut},
};
use swc_visit::chain;

struct ReanimatedWorkletsVisitor {
    globals: Vec<String>,
}

impl ReanimatedWorkletsVisitor {
    pub fn new(globals: Vec<String>) -> Self {
        ReanimatedWorkletsVisitor { globals }
    }

    // Trying to get an ident from expr. This is for the call_expr's callee,
    // does not cover all of expr cases.
    fn get_callee_expr_ident(&mut self, expr: &Expr) -> Option<Ident> {
        match expr {
            Expr::Member(member_expr) => match &member_expr.prop {
                MemberProp::Ident(ident) => Some(ident.clone()),
                MemberProp::PrivateName(PrivateName { id, .. }) => Some(id.clone()),
                MemberProp::Computed(ComputedPropName { expr, .. }) => {
                    self.get_callee_expr_ident(&*expr)
                }
            },
            Expr::Fn(FnExpr { ident, .. }) => ident.clone(),
            Expr::Call(CallExpr { callee, .. }) => {
                if let Callee::Expr(expr) = callee {
                    self.get_callee_expr_ident(&*expr)
                } else {
                    None
                }
            }
            Expr::Ident(ident) => Some(ident.clone()),
            Expr::Class(ClassExpr { ident, .. }) => ident.clone(),
            Expr::Paren(ParenExpr { expr, .. }) => self.get_callee_expr_ident(&*expr),
            Expr::JSXMember(JSXMemberExpr { prop, .. }) => Some(prop.clone()),
            Expr::JSXNamespacedName(JSXNamespacedName { name, .. }) => Some(name.clone()),
            Expr::PrivateName(PrivateName { id, .. }) => Some(id.clone()),
            _ => None,
        }
    }

    fn process_worklets(&mut self, call_expr: &mut CallExpr) {
        let name = if let Callee::Expr(expr) = &call_expr.callee {
            self.get_callee_expr_ident(&*expr)
        } else {
            None
        };
    }
}

// TODO: this mimics existing plugin behavior runs specific transform pass
// before running actual visitor.
// 1. This may not required
// 2. If required, need to way to pass config to visitors instead of Default::default()
// https://github.com/software-mansion/react-native-reanimated/blob/b4ee4ea9a1f246c461dd1819c6f3d48440a25756/plugin.js#L367-L371=
impl VisitMut for ReanimatedWorkletsVisitor {
    fn visit_mut_call_expr(&mut self, call_expr: &mut CallExpr) {
        self.process_worklets(call_expr);
    }

    fn visit_mut_fn_decl(&mut self, fn_decl: &mut FnDecl) {}

    fn visit_mut_fn_expr(&mut self, fn_expr: &mut FnExpr) {}

    fn visit_mut_arrow_expr(&mut self, arrow_expr: &mut ArrowExpr) {}
}

pub struct WorkletsOptions {
    custom_globals: Option<Vec<String>>,
}

pub fn create_worklets_visitor(worklets_options: Option<WorkletsOptions>) -> impl VisitMut {
    let mut globals_vec = GLOBALS.map(|v| v.to_string()).to_vec();

    // allows adding custom globals such as host-functions
    if let Some(worklets_options) = worklets_options {
        if let Some(custom_globals) = worklets_options.custom_globals {
            globals_vec.extend(custom_globals);
        }
    };

    ReanimatedWorkletsVisitor::new(globals_vec)
}
