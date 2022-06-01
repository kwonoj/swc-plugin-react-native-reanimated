mod constants;
use std::path::{Path, PathBuf};

use crate::constants::{GLOBALS, GESTURE_HANDLER_GESTURE_OBJECTS};
use constants::{
    FUNCTIONLESS_FLAG, GESTURE_HANDLER_BUILDER_METHODS, OBJECT_HOOKS, POSSIBLE_OPT_FUNCTION,
    STATEMENTLESS_FLAG,
};
use swc_common::{util::take::Take, FileName, SourceMapper, Span, DUMMY_SP};
use swc_ecma_codegen::{self, text_writer::WriteJs, Emitter, Node};
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

// Trying to get an ident from expr. This is for The call_expr's callee,
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

/// Naive port to string-hash-64 original plugin uses
/// to calculate hash.
///
/// TODO: This may not be required. The only reason to port
/// this fn is trying to mimic original behavior as close.
/// Confirm if any other hash can be used instead.
fn calculate_hash(value: &str) -> f64 {
    let mut hash1: i32 = 5381;
    let mut hash2: i32 = 52711;

    for c in value.chars().rev() {
        let char_code = c as i32;

        hash1 = (hash1.overflowing_mul(33).0) ^ char_code;
        hash2 = (hash2.overflowing_mul(33).0) ^ char_code;
    }

    (hash1 as u64 >> 0)
        .overflowing_mul(4096)
        .0
        .overflowing_add((hash2 as u32 >> 0) as u64)
        .0 as f64
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

struct ReanimatedWorkletsVisitor<S: swc_common::SourceMapper> {
    globals: Vec<String>,
    filename: FileName,
    in_use_animated_style: bool,
    source_map: std::sync::Arc<S>,
    relative_cwd: Option<PathBuf>,
}

impl<S: swc_common::SourceMapper> ReanimatedWorkletsVisitor<S> {
    pub fn new(
        source_map: std::sync::Arc<S>,
        globals: Vec<String>,
        filename: FileName,
        relative_cwd: Option<PathBuf>,
    ) -> Self {
        ReanimatedWorkletsVisitor {
            source_map,
            globals,
            filename,
            relative_cwd,
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

    /// Print givne fn's string with writer.
    /// This should be called with `cloned` node, as internally this'll take ownership.
    fn build_worklet_string(&mut self, fn_name: Ident, expr: Expr) -> String {
        /*
        function prependClosureVariablesIfNecessary(closureVariables, body) {
            if (closureVariables.length === 0) {
              return body;
            }

            return t.blockStatement([
              t.variableDeclaration('const', [
                t.variableDeclarator(
                  t.objectPattern(
                    closureVariables.map((variable) =>
                      t.objectProperty(
                        t.identifier(variable.name),
                        t.identifier(variable.name),
                        false,
                        true
                      )
                    )
                  ),
                  t.memberExpression(t.identifier('jsThis'), t.identifier('_closure'))
                ),
              ]),
              body,
            ]);
          }

          traverse(fun, {
            enter(path) {
              t.removeComments(path.node);
            },
          });

          const workletFunction = t.functionExpression(
            t.identifier(name),
            fun.program.body[0].expression.params,
            prependClosureVariablesIfNecessary(
              closureVariables,
              fun.program.body[0].expression.body
            )
          );

          return generate(workletFunction, { compact: true }).code;
         */

        let (params, body) = match expr {
            Expr::Arrow(mut arrow_expr) => (
                arrow_expr.params.drain(..).map(Param::from).collect(),
                arrow_expr.body,
            ),
            Expr::Fn(fn_expr) => (
                fn_expr.function.params,
                BlockStmtOrExpr::BlockStmt(
                    fn_expr
                        .function
                        .body
                        .expect("Expect fn body exists to make worklet fn"),
                ),
            ),
            _ => todo!("unexpected"),
        };

        let body = match body {
            BlockStmtOrExpr::BlockStmt(body) => body,
            BlockStmtOrExpr::Expr(e) => BlockStmt {
                stmts: vec![Stmt::Expr(ExprStmt {
                    span: DUMMY_SP,
                    expr: e,
                })],
                ..BlockStmt::dummy()
            },
        };

        let transformed_function = FnExpr {
            ident: Some(fn_name),
            function: Function {
                params,
                body: Some(body),
                ..Function::dummy()
            },
            ..FnExpr::dummy()
        };

        let mut buf = vec![];
        {
            let wr = Box::new(swc_ecma_codegen::text_writer::JsWriter::new(
                Default::default(),
                "", //"\n",
                &mut buf,
                None,
            )) as Box<dyn WriteJs>;

            let mut emitter = Emitter {
                cfg: swc_ecma_codegen::Config {
                    minify: true, //TODO : is this `compact`?
                    ..Default::default()
                },
                comments: Default::default(),
                cm: Default::default(),
                wr,
            };

            transformed_function
                .emit_with(&mut emitter)
                .ok()
                .expect("Should emit");
        }
        String::from_utf8(buf).expect("invalid utf8 character detected")
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

    /// Actual fn to generate AST for worklet-ized function to be called across
    /// fn-like nodes (arrow fn, fnExpr)
    fn make_worklet_inner(
        &mut self,
        mut cloned: Expr,
        span: &Span,
        mut body: BlockStmtOrExpr,
        params: Vec<Param>,
        is_generator: bool,
        is_async: bool,
        type_params: Option<TsTypeParamDecl>,
        return_type: Option<TsTypeAnn>,
        decorators: Option<Vec<Decorator>>,
    ) -> Function {
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

        /*
         const code = '\n(' + (t.isObjectMethod(fun) ? 'function ' : '') + fun.toString() + '\n)';
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
        cloned.visit_mut_with(&mut preprocessor);

        let func_string = self.build_worklet_string(dummy_fn_name.clone(), cloned);
        let func_hash = calculate_hash(&func_string);

        let closure_ident = Ident::new("_closure".into(), DUMMY_SP);
        let as_string_ident = Ident::new("asString".into(), DUMMY_SP);
        let worklet_hash_ident = Ident::new("__workletHash".into(), DUMMY_SP);
        let location_ident = Ident::new("__location".into(), DUMMY_SP);

        // Naive approach to calcuate relative path from options.
        // Note this relies on plugin config option (relative_cwd) to pass specific cwd.
        // unlike original babel plugin, we can't calculate cwd inside of plugin.
        // TODO: This is not sound relative path calcuation
        let filename_str = if let Some(relative_cwd) = &self.relative_cwd {
            self.filename
                .to_string()
                .strip_prefix(
                    relative_cwd
                        .as_os_str()
                        .to_str()
                        .expect("Should able to convert cwd to string"),
                )
                .expect("Should able to strip relative cwd")
                .to_string()
        } else {
            self.filename.to_string()
        };

        let loc = self.source_map.lookup_char_pos(span.lo);
        let code_location = format!("{} ({}:{})", filename_str, loc.line, loc.col_display);

        // TODO: need to use closuregenerator
        let dummy_closure = Expr::Object(ObjectLit::dummy());

        let decorators = if let Some(decorators) = decorators {
            decorators
        } else {
            Default::default()
        };

        let func_expr = match body.take() {
            BlockStmtOrExpr::BlockStmt(body) => Expr::Fn(FnExpr {
                ident: Some(dummy_fn_name.clone()),
                function: Function {
                    params,
                    decorators,
                    span: DUMMY_SP,
                    body: Some(body),
                    is_generator,
                    is_async,
                    type_params,
                    return_type,
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
                    right: Box::new(Expr::Lit(Lit::Str(Str::from(func_string)))),
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
                        value: func_hash.into(),
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
                    right: Box::new(Expr::Lit(Lit::Str(Str::from(code_location)))),
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
            body: Some(body),
            ..Function::dummy()
        }
    }

    fn make_worklet_from_fn(
        &mut self,
        ident: &mut Option<Ident>,
        function: &mut Function,
    ) -> Function {
        self.make_worklet_inner(
            // Have to clone to run transform preprocessor without changing original codes
            Expr::Fn(FnExpr {
                ident: ident.take(),
                function: function.clone(),
            }),
            &function.span,
            BlockStmtOrExpr::BlockStmt(
                function
                    .body
                    .take()
                    .expect("Expect fn body exists to make worklet fn"),
            ),
            function.params.take(),
            function.is_generator,
            function.is_async,
            function.type_params.take(),
            function.return_type.take(),
            Some(function.decorators.take()),
        )
    }

    fn make_worklet_from_fn_expr(&mut self, fn_expr: &mut FnExpr) -> Function {
        self.make_worklet_from_fn(&mut fn_expr.ident, &mut fn_expr.function)
    }

    fn make_worklet_from_arrow(&mut self, arrow_expr: &mut ArrowExpr) -> Function {
        self.make_worklet_inner(
            Expr::Arrow(arrow_expr.clone()),
            &arrow_expr.span,
            arrow_expr.body.take(),
            arrow_expr.params.drain(..).map(Param::from).collect(),
            arrow_expr.is_generator,
            arrow_expr.is_async,
            arrow_expr.type_params.take(),
            arrow_expr.return_type.take(),
            None,
        )
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
                    Some(self.make_worklet_from_fn(&mut None, function))
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
            Expr::Fn(fn_expr) => {
                // TODO: do we need to care about if fn body is empty?
                if fn_expr.function.body.is_some() {
                    let fn_expr = self.make_worklet_from_fn_expr(fn_expr);
                    *fn_like_expr = Expr::Call(CallExpr {
                        callee: Callee::Expr(Box::new(Expr::Fn(FnExpr {
                            ident: Default::default(),
                            function: fn_expr,
                        }))),
                        ..CallExpr::dummy()
                    });
                }
            }
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

    fn process_if_gesture_handler_event_callback_function(&mut self, callee: &mut Callee) {
        if is_gesture_object_event_callback_method(callee) {

        }
        /*if (
          t.isCallExpression(fun.parent) &&
          isGestureObjectEventCallbackMethod(t, fun.parent.callee)
        ) {
          processWorkletFunction(t, fun, state);
        }*/
    }
}

/// Checks if node matches `Gesture.Tap()` or similar.
/*
node: CallExpression(
callee: MemberExpression(
    object: Identifier('Gesture')
    property: Identifier('Tap')
)
)
*/
fn is_gesture_object(expr: &Expr) -> bool {
    if let Expr::Call(call_expr) = expr {
        if let Callee::Expr(callee) = &call_expr.callee {
            if let Expr::Member(member_expr) = &**callee {
                if let Expr::Ident(ident) = &*member_expr.obj {
                    if let MemberProp::Ident(prop_ident) = &member_expr.prop {
                        return &*ident.sym == "Gesture"
                            && GESTURE_HANDLER_GESTURE_OBJECTS
                                .iter()
                                .any(|m| *m == &*prop_ident.sym);
                    }
                }
            }
        }
    }

    false
}

/// Checks if node matches the pattern `Gesture.Foo()[*]`
/// where `[*]` represents any number of chained method calls, like `.something(42)`.
fn contains_gesture_object(expr: &Expr) -> bool {
    // direct call
    if is_gesture_object(expr) {
        return true;
    }

    // method chaining
    if let Expr::Call(call_expr) = expr {
        if let Callee::Expr(expr) = &call_expr.callee {
            if let Expr::Member(expr) = &**expr {
                return contains_gesture_object(&expr.obj);
            }
        }
    }
    return false;
}

/// Checks if node matches the pattern `Gesture.Foo()[*].onBar`
/// where `[*]` represents any number of method calls.
fn is_gesture_object_event_callback_method(callee: &Callee) -> bool {
    if let Callee::Expr(expr) = callee {
        if let Expr::Member(expr) = &**expr {
            if let MemberProp::Ident(ident) = &expr.prop {
                if GESTURE_HANDLER_BUILDER_METHODS
                    .iter()
                    .any(|m| *m == &*ident.sym)
                {
                    return contains_gesture_object(&*expr.obj);
                }
            }
        }
    }

    return false;
}

impl<S: SourceMapper> VisitMut for ReanimatedWorkletsVisitor<S> {
    fn visit_mut_call_expr(&mut self, call_expr: &mut CallExpr) {
        if is_gesture_object_event_callback_method(&call_expr.callee) {
            self.process_if_gesture_handler_event_callback_function(&mut call_expr.callee);
        } else {
            self.process_worklets(call_expr);
        }

        call_expr.visit_mut_children_with(self);
    }

    fn visit_mut_fn_decl(&mut self, fn_decl: &mut FnDecl) {
        fn_decl.visit_mut_children_with(self);
    }

    fn visit_mut_fn_expr(&mut self, fn_expr: &mut FnExpr) {
        //processIfWorkletNode(t, path, state);

        fn_expr.visit_mut_children_with(self);
    }

    fn visit_mut_arrow_expr(&mut self, arrow_expr: &mut ArrowExpr) {
        arrow_expr.visit_mut_children_with(self);
    }
}

pub struct WorkletsOptions {
    custom_globals: Option<Vec<String>>,
    filename: FileName,
    relative_cwd: Option<PathBuf>,
}

impl WorkletsOptions {
    pub fn new(
        custom_globals: Option<Vec<String>>,
        filename: FileName,
        relative_cwd: Option<PathBuf>,
    ) -> Self {
        WorkletsOptions {
            custom_globals,
            filename,
            relative_cwd,
        }
    }
}

pub fn create_worklets_visitor<S: SourceMapper>(
    worklets_options: WorkletsOptions,
    source_map: std::sync::Arc<S>,
) -> impl VisitMut {
    let mut globals_vec = GLOBALS.map(|v| v.to_string()).to_vec();

    // allows adding custom globals such as host-functions
    if let Some(custom_globals) = worklets_options.custom_globals {
        globals_vec.extend(custom_globals);
    };

    ReanimatedWorkletsVisitor::new(
        source_map,
        globals_vec,
        worklets_options.filename,
        worklets_options.relative_cwd,
    )
}
