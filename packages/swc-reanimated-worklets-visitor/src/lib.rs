mod constants;
use crate::constants::GLOBALS;
use constants::{FUNCTIONLESS_FLAG, OBJECT_HOOKS, STATEMENTLESS_FLAG, POSSIBLE_OPT_FUNCTION};
use swc_common::DUMMY_SP;
use swc_ecma_transforms_compat::{
    es2015::{arrow, shorthand, template_literal},
    es2020::{nullish_coalescing, optional_chaining},
};
use swc_ecmascript::{
    ast::*,
    utils::function,
    visit::{as_folder, Visit, VisitMut, VisitMutWith, VisitWith},
};
use swc_visit::chain;

// Trying to get an ident from expr. This is for the call_expr's callee,
// does not cover all of expr cases.
fn get_callee_expr_ident(expr: &Expr) -> Option<Ident> {
    match expr {
        Expr::Member(member_expr) => match &member_expr.prop {
            MemberProp::Ident(ident) => Some(ident.clone()),
            MemberProp::PrivateName(PrivateName { id, .. }) => Some(id.clone()),
            MemberProp::Computed(ComputedPropName { expr, .. }) => {
                get_callee_expr_ident(&*expr)
            }
        },
        Expr::Fn(FnExpr { ident, .. }) => ident.clone(),
        Expr::Call(CallExpr { callee, .. }) => {
            if let Callee::Expr(expr) = callee {
                get_callee_expr_ident(&*expr)
            } else {
                None
            }
        }
        Expr::Ident(ident) => Some(ident.clone()),
        Expr::Class(ClassExpr { ident, .. }) => ident.clone(),
        Expr::Paren(ParenExpr { expr, .. }) => get_callee_expr_ident(&*expr),
        Expr::JSXMember(JSXMemberExpr { prop, .. }) => Some(prop.clone()),
        Expr::JSXNamespacedName(JSXNamespacedName { name, .. }) => Some(name.clone()),
        Expr::PrivateName(PrivateName { id, .. }) => Some(id.clone()),
        _ => None,
    }
}

struct OptimizationFinderVisitor {
    is_stmt: bool,
    is_fn_call: bool,
}

impl OptimizationFinderVisitor {
    pub fn new() -> Self {
        OptimizationFinderVisitor {
            is_stmt: false,
            is_fn_call: false,
        }
    }

    pub fn calculate_flags(&self) -> i32 {
        let mut flags = 0;
        if !self.is_fn_call {
            flags = flags | FUNCTIONLESS_FLAG;
        }

        if !self.is_stmt {
            flags = flags | STATEMENTLESS_FLAG;
        }

        flags
    }
}

impl Visit for OptimizationFinderVisitor {
    fn visit_if_stmt(&mut self, _if: &IfStmt) {
        self.is_stmt = true;
    }

    fn visit_call_expr(&mut self, call_expr: &CallExpr) {
        if let Callee::Expr(expr) = &call_expr.callee {
            let name = get_callee_expr_ident(&*expr);

            if let Some(name) = name {
                if !POSSIBLE_OPT_FUNCTION.iter().any(|v| { *v == &*name.sym }) {
                    self.is_fn_call = true;
                }
            }
        }
    }
}

struct ReanimatedWorkletsVisitor {
    globals: Vec<String>,
    in_use_animated_style: bool,
}

impl ReanimatedWorkletsVisitor {
    pub fn new(globals: Vec<String>) -> Self {
        ReanimatedWorkletsVisitor {
            globals,
            in_use_animated_style: false,
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

    fn make_worklet_name(&mut self) {
        todo!("not implemented");
    }

    // Returns a new FunctionExpression which is a workletized version of provided
    // FunctionDeclaration, FunctionExpression, ArrowFunctionExpression or ObjectMethod.
    fn make_worklet(&mut self, e: &mut PropOrSpread) -> Function {
        // TODO: consolidate into make_worklet_name
        let dummy_fn_name = Ident::new("_f".into(), DUMMY_SP);

        // TODO
        /*
        // remove 'worklet'; directive before calling .toString()
        fun.traverse({
            DirectiveLiteral(path) {
            if (path.node.value === 'worklet' && path.getFunctionParent() === fun) {
                path.parentPath.remove();
            }
            },
        });

        // We use copy because some of the plugins don't update bindings and
        // some even break them

        const code =
            '\n(' + (t.isObjectMethod(fun) ? 'function ' : '') + fun.toString() + '\n)';
        */

        // TODO: this mimics existing plugin behavior runs specific transform pass
        // before running actual visitor.
        // 1. This may not required
        // 2. If required, need to way to pass config to visitors instead of Default::default()
        // https://github.com/software-mansion/react-native-reanimated/blob/b4ee4ea9a1f246c461dd1819c6f3d48440a25756/plugin.js#L367-L371=
        let mut preprocessor = chain!(
            shorthand(),
            arrow(),
            optional_chaining(Default::default()),
            nullish_coalescing(Default::default()),
            template_literal(Default::default())
        );

        e.visit_mut_with(&mut preprocessor);

        let mut opt_find_visitor = OptimizationFinderVisitor::new();
        e.visit_with(&mut opt_find_visitor);

        let opt_flags = opt_find_visitor.calculate_flags();

        /*
         traverse(transformed.ast, {
            ReferencedIdentifier(path) {
            const name = path.node.name;
            if (globals.has(name) || (fun.node.id && fun.node.id.name === name)) {
                return;
            }

            const parentNode = path.parent;

            if (
                parentNode.type === 'MemberExpression' &&
                parentNode.property === path.node &&
                !parentNode.computed
            ) {
                return;
            }

            if (
                parentNode.type === 'ObjectProperty' &&
                path.parentPath.parent.type === 'ObjectExpression' &&
                path.node !== parentNode.value
            ) {
                return;
            }

            let currentScope = path.scope;

            while (currentScope != null) {
                if (currentScope.bindings[name] != null) {
                return;
                }
                currentScope = currentScope.parent;
            }
            closure.set(name, path.node);
            closureGenerator.addPath(name, path);
            },
            AssignmentExpression(path) {
            // test for <something>.value = <something> expressions
            const left = path.node.left;
            if (
                t.isMemberExpression(left) &&
                t.isIdentifier(left.object) &&
                t.isIdentifier(left.property, { name: 'value' })
            ) {
                outputs.add(left.object.name);
            }
            },
        });
        */

        /*
        const variables = Array.from(closure.values());

        const privateFunctionId = t.identifier('_f');
        const clone = t.cloneNode(fun.node);
        let funExpression;
        if (clone.body.type === 'BlockStatement') {
            funExpression = t.functionExpression(null, clone.params, clone.body);
        } else {
            funExpression = clone;
        }
        const funString = buildWorkletString(
            t,
            transformed.ast,
            variables,
            functionName
        );
        const workletHash = hash(funString);

        let location = state.file.opts.filename;
        if (state.opts.relativeSourceLocation) {
            const path = require('path');
            location = path.relative(state.cwd, location);
        }

        const loc = fun && fun.node && fun.node.loc && fun.node.loc.start;
        if (loc) {
            const { line, column } = loc;
            if (typeof line === 'number' && typeof column === 'number') {
            location = `${location} (${line}:${column})`;
            }
        }
        */

        todo!("not implemented");
    }

    fn process_worklet_object_method(&mut self, method_prop: &mut PropOrSpread) {
        let key = if let PropOrSpread::Prop(prop) = method_prop {
            match &**prop {
                Prop::Method(MethodProp { key, .. }) => Some(key.clone()),
                _ => None,
            }
        } else {
            None
        };

        if let Some(key) = key {
            let function = self.make_worklet(method_prop);
            *method_prop = PropOrSpread::Prop(Box::new(Prop::Method(MethodProp { key, function })));
        }
    }

    fn process_worklet_function(&mut self) {
        todo!("not implemented");
    }

    fn process_worklets(&mut self, call_expr: &mut CallExpr) {
        let name = if let Callee::Expr(expr) = &call_expr.callee {
            get_callee_expr_ident(&*expr)
        } else {
            None
        };

        match name {
            Some(name) if OBJECT_HOOKS.contains(&&*name.sym) && call_expr.args.len() > 0 => {
                if &*name.sym == "useAnimatedStyle" {
                    self.in_use_animated_style = true;
                }

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

                self.in_use_animated_style = false;
            }
            _ => {}
        }
    }
}

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
