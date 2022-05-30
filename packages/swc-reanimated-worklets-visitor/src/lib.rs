mod constants;
use crate::constants::GLOBALS;
use constants::{FUNCTIONLESS_FLAG, OBJECT_HOOKS, POSSIBLE_OPT_FUNCTION, STATEMENTLESS_FLAG};
use swc_common::{util::take::Take, DUMMY_SP};
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
            MemberProp::Computed(ComputedPropName { expr, .. }) => get_callee_expr_ident(&*expr),
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
                if !POSSIBLE_OPT_FUNCTION.iter().any(|v| *v == &*name.sym) {
                    self.is_fn_call = true;
                }
            }
        }
    }
}

struct ClosureIdentVisitor {}

impl Visit for ClosureIdentVisitor {
    fn visit_member_expr(&mut self, _member_expr: &MemberExpr) {
        //noop
    }

    fn visit_object_lit(&mut self, object_expr: &ObjectLit) {
        for prop in &object_expr.props {
            match prop {
                PropOrSpread::Prop(p) => {
                    let p = &**p;
                    match p {
                        Prop::Shorthand(..) | Prop::KeyValue(..) => {}
                        _ => prop.visit_with(self),
                    }
                }
                PropOrSpread::Spread(..) => prop.visit_with(self),
            };
        }
    }

    //fn visit_binding_ident(&mut self, ident: &BindingIdent) {}
    fn visit_ident(&mut self, ident: &Ident) {}

    fn visit_assign_expr(&mut self, assign_expr: &AssignExpr) {}
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
    fn make_worklet(&mut self, function: &mut Function) -> Function {
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

        function.visit_mut_with(&mut preprocessor);

        let mut opt_find_visitor = OptimizationFinderVisitor::new();
        function.visit_with(&mut opt_find_visitor);

        /*
          const closure = new Map();
            const outputs = new Set();
            const closureGenerator = new ClosureGenerator();
            const options = {};
        */

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

        let function_expr = if function.body.is_some() {
        } else {
        };

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

        Function { ..function.take() }
    }

    fn make_worklet_from_arrow(&mut self, arrow_expr: &mut ArrowExpr) -> Function {
        /*
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

              const statements = [
          //t.variableDeclaration('const', [ t.variableDeclarator(privateFunctionId, funExpression), ]),
          //t.expressionStatement(t.assignmentExpression('=',t.memberExpression(privateFunctionId, t.identifier('_closure'), false),closureGenerator.generate(t, variables, closure.keys()))),
          //t.expressionStatement(t.assignmentExpression('=', t.memberExpression(privateFunctionId, t.identifier('asString'), false), t.stringLiteral(funString))),
          //t.expressionStatement(t.assignmentExpression('=',t.memberExpression(privateFunctionId,t.identifier('__workletHash'),false),t.numericLiteral(workletHash))),
          //t.expressionStatement(
        //t.assignmentExpression('=',t.memberExpression(privateFunctionId,t.identifier('__location'),false),t.stringLiteral(location))
          ),
        ];

              if (options && options.optFlags) {
                  statements.push(
                  t.expressionStatement(
                      t.assignmentExpression(
                      '=',
                      t.memberExpression(
                          privateFunctionId,
                          t.identifier('__optimalization'),
                          false
                      ),
                      t.numericLiteral(options.optFlags)
                      )
                  )
                  );
              }
              */
        // TODO: consolidate into make_worklet_name
        let dummy_fn_name = Ident::new("_f".into(), DUMMY_SP);
        let closure_ident = Ident::new("_closure".into(), DUMMY_SP);
        let as_string_ident = Ident::new("asString".into(), DUMMY_SP);
        let worklet_hash_ident = Ident::new("__workletHash".into(), DUMMY_SP);
        let location_ident = Ident::new("__location".into(), DUMMY_SP);

        // TODO: need to use closuregenerator
        let dummy_closure = Expr::Object(ObjectLit::dummy());

        let func_expr = match arrow_expr.body.take() {
            BlockStmtOrExpr::BlockStmt(body) => Expr::Fn(FnExpr {
                ident: Some(dummy_fn_name.clone()),
                function: Function {
                    params: arrow_expr.params.drain(..).map(Param::from).collect(),
                    decorators: Default::default(),
                    span: DUMMY_SP,
                    body: Some(body),
                    is_generator: arrow_expr.is_generator,
                    is_async: arrow_expr.is_async,
                    type_params: arrow_expr.type_params.take(),
                    return_type: arrow_expr.return_type.take(),
                },
            }),
            BlockStmtOrExpr::Expr(e) => *e,
        };

        let mut stmts = vec![
            // a function closure wraps original,
            // const _f = function () { .. }
            Stmt::Decl(Decl::Var(VarDecl {
                span: DUMMY_SP,
                declare: false,
                kind: VarDeclKind::Const,
                decls: vec![VarDeclarator {
                    span: DUMMY_SP,
                    definite: false,
                    name: Pat::Ident(BindingIdent::from(dummy_fn_name.clone())),
                    init: Some(Box::new(func_expr)),
                }],
            })),
            // _f._closure = {...}
            Stmt::Expr(ExprStmt {
                span: DUMMY_SP,
                expr: Box::new(Expr::Assign(AssignExpr {
                    span: DUMMY_SP,
                    op: AssignOp::Assign,
                    left: PatOrExpr::Expr(Box::new(Expr::Member(MemberExpr {
                        span: DUMMY_SP,
                        obj: Box::new(Expr::Ident(dummy_fn_name.clone())),
                        prop: MemberProp::Ident(closure_ident.clone()),
                    }))),
                    // TODO: this is not complete
                    right: Box::new(dummy_closure.clone()),
                })),
            }),
            // _f.asString
            Stmt::Expr(ExprStmt {
                span: DUMMY_SP,
                expr: Box::new(Expr::Assign(AssignExpr {
                    span: DUMMY_SP,
                    op: AssignOp::Assign,
                    left: PatOrExpr::Expr(Box::new(Expr::Member(MemberExpr {
                        span: DUMMY_SP,
                        obj: Box::new(Expr::Ident(dummy_fn_name.clone())),
                        prop: MemberProp::Ident(as_string_ident.clone()),
                    }))),
                    // TODO: this is not complete
                    right: Box::new(dummy_closure.clone()),
                })),
            }),
            //_f.__workletHash
            Stmt::Expr(ExprStmt {
                span: DUMMY_SP,
                expr: Box::new(Expr::Assign(AssignExpr {
                    span: DUMMY_SP,
                    op: AssignOp::Assign,
                    left: PatOrExpr::Expr(Box::new(Expr::Member(MemberExpr {
                        span: DUMMY_SP,
                        obj: Box::new(Expr::Ident(dummy_fn_name.clone())),
                        prop: MemberProp::Ident(worklet_hash_ident.clone()),
                    }))),
                    // TODO: this is not complete
                    right: Box::new(Expr::Lit(Lit::Num(Number {
                        span: DUMMY_SP,
                        value: 1111.into(),
                        raw: None,
                    }))),
                })),
            }),
            //_f.__location
            Stmt::Expr(ExprStmt {
                span: DUMMY_SP,
                expr: Box::new(Expr::Assign(AssignExpr {
                    span: DUMMY_SP,
                    op: AssignOp::Assign,
                    left: PatOrExpr::Expr(Box::new(Expr::Member(MemberExpr {
                        span: DUMMY_SP,
                        obj: Box::new(Expr::Ident(dummy_fn_name.clone())),
                        prop: MemberProp::Ident(location_ident.clone()),
                    }))),
                    // TODO: this is not complete
                    right: Box::new(Expr::Lit(Lit::Str(Str::from("location_dummy")))),
                })),
            }),
        ];

        stmts.push(Stmt::Return(ReturnStmt {
            span: DUMMY_SP,
            arg: Some(Box::new(Expr::Ident(dummy_fn_name))),
        }));

        let body = BlockStmt {
            span: DUMMY_SP,
            stmts,
        };

        Function {
            params: Default::default(),
            decorators: Default::default(),
            span: DUMMY_SP,
            body: Some(body),
            is_generator: arrow_expr.is_generator,
            is_async: arrow_expr.is_async,
            type_params: arrow_expr.type_params.take(),
            return_type: arrow_expr.return_type.take(),
        }
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
            let function = if let PropOrSpread::Prop(prop) = method_prop {
                if let Prop::Method(MethodProp { function, .. }) = &mut **prop {
                    Some(self.make_worklet(function))
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(function) = function {
                *method_prop =
                    PropOrSpread::Prop(Box::new(Prop::Method(MethodProp { key, function })));
            }
        }
    }

    fn process_worklet_function(&mut self, fn_like_expr: &mut Expr) {
        /*
          const newFun = makeWorklet(t, fun, state);

        const replacement = t.callExpression(newFun, []);

        // we check if function needs to be assigned to variable declaration.
        // This is needed if function definition directly in a scope. Some other ways
        // where function definition can be used is for example with variable declaration:
        // const ggg = function foo() { }
        // ^ in such a case we don't need to define variable for the function
        const needDeclaration =
            t.isScopable(fun.parent) || t.isExportNamedDeclaration(fun.parent);
        fun.replaceWith(
            fun.node.id && needDeclaration
            ? t.variableDeclaration('const', [
                t.variableDeclarator(fun.node.id, replacement),
                ])
            : replacement
        );
        */
        match fn_like_expr {
            Expr::Arrow(arrow_expr) => {
                let fn_expr = self.make_worklet_from_arrow(arrow_expr);

                *fn_like_expr = Expr::Call(CallExpr {
                    callee: Callee::Expr(Box::new(Expr::Fn(FnExpr {
                        ident: Default::default(),
                        function: fn_expr,
                    }))),
                    ..CallExpr::dummy()
                });
            }
            Expr::Fn(fn_expr) => {}
            _ => {}
        }
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
                                Prop::KeyValue(KeyValueProp { value, .. }) => {
                                    self.process_worklet_function(&mut **value);
                                }
                                _ => {}
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
