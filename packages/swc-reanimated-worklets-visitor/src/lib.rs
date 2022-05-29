mod constants;
use crate::constants::GLOBALS;
use constants::OBJECT_HOOKS;
use swc_common::DUMMY_SP;
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

    // Returns a new FunctionExpression which is a workletized version of provided
    // FunctionDeclaration, FunctionExpression, ArrowFunctionExpression or ObjectMethod.
    fn make_worklet_method_prop(&mut self, method_prop: &MethodProp) {
        let fn_name = match &method_prop.key {
            PropName::Ident(id) => id.clone(),
            PropName::Str(str) => Ident::new(str.value.clone(), DUMMY_SP),
            PropName::Num(num) => Ident::new(num.value.to_string().into(), DUMMY_SP),
            _ => Ident::new("_f".into(), DUMMY_SP),
        };
    }

    fn process_worklet_object_method(&mut self, method_prop: &mut PropOrSpread) {
        //let new_fn = self.make_worklet_method_prop(method_prop);

        //let replacement=
        /*
        const newFun = makeWorklet(t, path, state);

        const replacement = t.objectProperty(
          t.identifier(path.node.key.name),
          t.callExpression(newFun, [])
        );

        path.replaceWith(replacement);
        */
    }

    fn process_worklet_function(&mut self) {
        todo!("not implemented");
    }

    fn process_worklets(&mut self, call_expr: &mut CallExpr) {
        let name = if let Callee::Expr(expr) = &call_expr.callee {
            self.get_callee_expr_ident(&*expr)
        } else {
            None
        };

        match name {
            Some(name) if OBJECT_HOOKS.contains(&&*name.sym) && call_expr.args.len() > 0 => {
                let arg = call_expr.args.get_mut(0).expect("should have args");

                if let Expr::Object(object_expr) = &mut *arg.expr {
                    let properties = &mut object_expr.props;
                    for property in properties {
                        if let PropOrSpread::Prop(prop) = property {
                            match &mut **prop {
                                Prop::Method(..) => {
                                    self.process_worklet_object_method(property);
                                }
                                _ => {
                                    self.process_worklet_function();
                                }
                            };
                        }
                    }
                } else {
                    /*
                    const indexes = functionArgsToWorkletize.get(name);
                    if (Array.isArray(indexes)) {
                      indexes.forEach((index) => {
                        processWorkletFunction(t, path.get(`arguments.${index}`), state);
                      });
                    } */
                }
            }
            _ => {}
        }
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
