mod constants;
use hash32::{FnvHasher, Hasher};
use indexmap::IndexMap;
use std::cell::RefCell;
use std::path::PathBuf;
use std::{collections::HashSet, hash::Hash};
use swc_common::Mark;

use crate::constants::{GESTURE_HANDLER_GESTURE_OBJECTS, GLOBALS};
use constants::{
    FUNCTIONLESS_FLAG, FUNCTION_ARGS_TO_WORKLETIZE, GESTURE_HANDLER_BUILDER_METHODS, OBJECT_HOOKS,
    POSSIBLE_OPT_FUNCTION, STATEMENTLESS_FLAG,
};
use swc_common::{util::take::Take, FileName, Span, DUMMY_SP};
use swc_ecma_codegen::{self, text_writer::WriteJs, Emitter, Node};
use swc_ecma_transforms_compat::{
    es2015::{arrow, shorthand, template_literal},
    es2020::{nullish_coalescing, optional_chaining},
};
use swc_ecmascript::{
    ast::*,
    visit::{Visit, VisitMut, VisitMutWith, VisitWith},
};

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

/// This hash does not returns identical to original plugin's hash64.
fn calculate_hash(value: &str) -> f64 {
    let mut fnv = FnvHasher::default();
    value.hash(&mut fnv);
    fnv.finish32() as f64
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

/// Locate `'worklet';` directives and performs necessary transformation
/// if directive found.
/// - Removes comments explicitly
/// - Removes `worklet`; directive itself
struct DirectiveFinderVisitor<C: Clone + swc_common::comments::Comments> {
    pub has_worklet_directive: bool,
    in_fn_parent_node: bool,
    comments: C,
}

impl<C: Clone + swc_common::comments::Comments> DirectiveFinderVisitor<C> {
    pub fn new(comments: C) -> Self {
        DirectiveFinderVisitor {
            has_worklet_directive: false,
            in_fn_parent_node: false,
            comments,
        }
    }
}

impl<C: Clone + swc_common::comments::Comments> VisitMut for DirectiveFinderVisitor<C> {
    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        let old = self.in_fn_parent_node;
        match expr {
            Expr::Arrow(..) | Expr::Fn(..) => {
                self.in_fn_parent_node = true;
            }
            _ => {}
        }

        expr.visit_mut_children_with(self);
        self.in_fn_parent_node = old;
    }

    fn visit_mut_stmt(&mut self, stmt: &mut Stmt) {
        // TODO: There's no directive visitor
        if let Stmt::Expr(ExprStmt { expr, .. }) = stmt {
            if let Expr::Lit(Lit::Str(Str { value, .. })) = &**expr {
                if &*value == "worklet" {
                    self.has_worklet_directive = true;
                    // remove 'worklet'; directive before calling .toString()
                    *stmt = Stmt::dummy();
                }
            }
        }

        if self.has_worklet_directive {
            // remove comments if there's worklet directive.
            // TODO:
            // 1. This is not complete
            // 2. Do we need utility like .remove_comments_recursively()
            match &stmt {
                Stmt::Expr(ExprStmt { span, .. }) | Stmt::Return(ReturnStmt { span, .. }) => {
                    self.comments.take_leading(span.hi);
                    self.comments.take_leading(span.lo);
                    self.comments.take_trailing(span.hi);
                    self.comments.take_trailing(span.lo);
                }
                _ => {}
            };
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Block,
    Fn,
}

impl Default for ScopeKind {
    fn default() -> Self {
        ScopeKind::Fn
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentType {
    Binding,
    Ref,
    Label,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarType {
    Param,
    Var(VarDeclKind),
}

#[derive(Debug)]
pub struct VarInfo {
    pub kind: VarType,
    pub value: RefCell<Option<Expr>>,
}

#[derive(Default, Debug)]
struct Scope<'a> {
    /// Parent scope of the scope
    parent: Option<&'a Scope<'a>>,
    /// [Mark] of the current scope.
    mark: Mark,
    /// Kind of the scope.
    kind: ScopeKind,
    closure: swc_common::collections::AHashSet<Id>,
    bindings: IndexMap<Id, VarInfo, ahash::RandomState>,
}

impl<'a> Scope<'a> {
    pub fn new(kind: ScopeKind, mark: Mark, parent: Option<&'a Scope<'a>>) -> Self {
        Scope {
            parent,
            kind,
            mark,
            closure: Default::default(),
            bindings: Default::default(),
        }
    }
}

struct ClosureIdentVisitor<'a> {
    outputs: HashSet<Ident>,
    is_parent_member_expr: bool,
    is_parent_member_expr_computed: bool,
    is_in_object_expression: bool,
    is_in_object_prop: bool,
    parent_member_expr_prop_ident: Option<Ident>,
    parent_object_prop_ident: Option<Ident>,
    ident_type: Option<IdentType>,
    closure: Vec<Ident>,
    scope: Scope<'a>,
    in_type: bool,
    globals: &'a Vec<String>,
    fn_name: &'a Option<Ident>,
}

impl<'a> ClosureIdentVisitor<'a> {
    pub fn new(current: Scope<'a>, globals: &'a Vec<String>, fn_name: &'a Option<Ident>) -> Self {
        ClosureIdentVisitor {
            outputs: Default::default(),
            is_parent_member_expr: false,
            is_parent_member_expr_computed: false,
            is_in_object_expression: false,
            is_in_object_prop: false,
            parent_member_expr_prop_ident: Default::default(),
            parent_object_prop_ident: Default::default(),
            closure: Default::default(),
            scope: current,
            ident_type: None,
            in_type: false,
            globals,
            fn_name,
        }
    }

    pub fn from(value: &ClosureIdentVisitor<'a>, current: Scope<'a>) -> Self {
        ClosureIdentVisitor {
            outputs: value.outputs.clone(),
            is_parent_member_expr: value.is_parent_member_expr,
            is_parent_member_expr_computed: value.is_parent_member_expr_computed,
            is_in_object_expression: value.is_in_object_expression,
            is_in_object_prop: value.is_in_object_prop,
            parent_member_expr_prop_ident: value.parent_member_expr_prop_ident.clone(),
            parent_object_prop_ident: value.parent_object_prop_ident.clone(),
            closure: value.closure.clone(),
            scope: current,
            ident_type: value.ident_type.clone(),
            in_type: false,
            globals: value.globals,
            fn_name: value.fn_name,
        }
    }

    fn visit_stmt_within_child_scope(&mut self, s: &Stmt) {
        let child_mark = Mark::fresh(Mark::root());
        let mut child = ClosureIdentVisitor::from(
            self,
            Scope::new(ScopeKind::Block, child_mark, Some(&self.scope)),
        );

        child.visit_stmt_within_same_scope(s)
    }

    fn visit_stmt_within_same_scope(&mut self, s: &Stmt) {
        match s {
            Stmt::Block(s) => {
                s.visit_children_with(self);
            }
            _ => s.visit_with(self),
        }
    }

    fn visit_with_child<T>(&mut self, kind: ScopeKind, child_mark: Mark, node: &T)
    where
        T: 'static + for<'any> VisitWith<ClosureIdentVisitor<'any>>,
    {
        self.with_child(kind, child_mark, |child| {
            node.visit_children_with(child);
        });
    }

    fn with_child<F>(&mut self, kind: ScopeKind, child_mark: Mark, op: F)
    where
        F: for<'any> FnOnce(&mut ClosureIdentVisitor<'any>),
    {
        let bindings = {
            let mut child = ClosureIdentVisitor::new(
                Scope::new(kind, child_mark, Some(&self.scope)),
                &self.globals,
                self.fn_name,
            );

            op(&mut child);

            child.scope.bindings
        };

        if !matches!(kind, ScopeKind::Fn { .. }) {
            let v = bindings;

            for (id, v) in v.into_iter().filter_map(|(id, v)| {
                if v.kind == VarType::Var(VarDeclKind::Var) {
                    Some((id, v))
                } else {
                    None
                }
            }) {
                let v: VarInfo = v;

                //v.hoisted.set(true);

                *v.value.borrow_mut() = None;
                //v.is_undefined.set(false);
                self.scope.bindings.insert(id, v);
            }
        }
    }
}

impl<'a> Visit for ClosureIdentVisitor<'a> {
    fn visit_member_expr(&mut self, member_expr: &MemberExpr) {
        let old_computed = self.is_parent_member_expr_computed;
        let old = self.is_parent_member_expr;
        if let MemberProp::Computed(..) = member_expr.prop {
            self.is_parent_member_expr_computed = true;
        }

        if let MemberProp::Ident(ident) = &member_expr.prop {
            self.parent_member_expr_prop_ident = Some(ident.clone())
        }

        self.is_parent_member_expr = true;
        member_expr.visit_children_with(self);

        self.parent_member_expr_prop_ident = None;
        self.is_parent_member_expr = old;
        self.is_parent_member_expr_computed = old_computed;
    }

    fn visit_arrow_expr(&mut self, arrow_expr: &ArrowExpr) {
        let child_mark = Mark::fresh(Mark::root());

        self.with_child(ScopeKind::Fn, child_mark, |folder| {
            let old = folder.ident_type;
            folder.ident_type = Some(IdentType::Binding);
            arrow_expr.params.visit_with(folder);
            folder.ident_type = old;

            {
                match &arrow_expr.body {
                    BlockStmtOrExpr::BlockStmt(s) => s.stmts.visit_with(folder),
                    BlockStmtOrExpr::Expr(e) => e.visit_with(folder),
                }
            }

            arrow_expr.return_type.visit_with(folder);
        });
    }

    fn visit_binding_ident(&mut self, i: &BindingIdent) {
        let ident_type = self.ident_type;
        let in_type = self.in_type;

        self.ident_type = Some(IdentType::Ref);
        i.type_ann.visit_with(self);

        self.ident_type = ident_type;
        i.id.visit_with(self);

        self.in_type = in_type;
        self.ident_type = ident_type;
    }

    fn visit_block_stmt(&mut self, block: &BlockStmt) {
        let child_mark = Mark::fresh(Mark::root());
        self.visit_with_child(ScopeKind::Block, child_mark, block);
    }

    fn visit_catch_clause(&mut self, c: &CatchClause) {
        let child_mark = Mark::fresh(Mark::root());

        // Child folder
        self.with_child(ScopeKind::Fn, child_mark, |folder| {
            folder.ident_type = Some(IdentType::Binding);
            c.param.visit_with(folder);
            folder.ident_type = Some(IdentType::Ref);

            c.body.visit_children_with(folder);
        });
    }

    fn visit_class_decl(&mut self, n: &ClassDecl) {
        n.class.decorators.visit_with(self);

        // Create a child scope. The class name is only accessible within the class.
        let child_mark = Mark::fresh(Mark::root());

        self.with_child(ScopeKind::Fn, child_mark, |folder| {
            folder.ident_type = Some(IdentType::Ref);

            n.class.visit_with(folder);
        });
    }

    fn visit_class_expr(&mut self, n: &ClassExpr) {
        // Create a child scope. The class name is only accessible within the class.
        let child_mark = Mark::fresh(Mark::root());

        self.with_child(ScopeKind::Fn, child_mark, |folder| {
            folder.ident_type = Some(IdentType::Binding);
            n.ident.visit_with(folder);
            folder.ident_type = Some(IdentType::Ref);

            n.class.visit_with(folder);
        });
    }

    fn visit_class_method(&mut self, m: &ClassMethod) {
        m.key.visit_with(self);

        for p in m.function.params.iter() {
            p.decorators.visit_with(self);
        }

        {
            let child_mark = Mark::fresh(Mark::root());

            self.with_child(ScopeKind::Fn, child_mark, |child| {
                m.function.visit_with(child);
            });
        }
    }

    fn visit_constructor(&mut self, c: &Constructor) {
        let child_mark = Mark::fresh(Mark::root());

        for p in c.params.iter() {
            match p {
                ParamOrTsParamProp::TsParamProp(p) => {
                    p.decorators.visit_with(self);
                }
                ParamOrTsParamProp::Param(p) => {
                    p.decorators.visit_with(self);
                }
            }
        }

        {
            let old = self.ident_type;
            self.ident_type = Some(IdentType::Binding);
            self.with_child(ScopeKind::Fn, child_mark, |folder| {
                c.params.visit_with(folder);
            });
            self.ident_type = old;

            self.with_child(ScopeKind::Fn, child_mark, |folder| match &c.body {
                Some(body) => {
                    body.visit_children_with(folder);
                }
                None => {}
            });
        }
    }

    fn visit_export_default_decl(&mut self, e: &ExportDefaultDecl) {
        // Treat default exported functions and classes as declarations
        // even though they are parsed as expressions.
        match &e.decl {
            DefaultDecl::Fn(f) => {
                if f.ident.is_some() {
                    let child_mark = Mark::fresh(Mark::root());

                    self.with_child(ScopeKind::Fn, child_mark, |folder| {
                        f.function.visit_with(folder)
                    })
                } else {
                    f.visit_with(self)
                }
            }
            DefaultDecl::Class(c) => {
                // Skip class expression visitor to treat as a declaration.
                c.class.visit_with(self)
            }
            _ => e.visit_children_with(self),
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        let old = self.ident_type;
        self.ident_type = Some(IdentType::Ref);
        expr.visit_children_with(self);
        self.ident_type = old;
    }

    fn visit_fn_decl(&mut self, node: &FnDecl) {
        // We don't fold this as Hoister handles this.

        node.function.decorators.visit_with(self);

        {
            let child_mark = Mark::fresh(Mark::root());
            self.with_child(ScopeKind::Fn, child_mark, |folder| {
                node.function.visit_with(folder);
            });
        }
    }

    fn visit_fn_expr(&mut self, e: &FnExpr) {
        e.function.decorators.visit_with(self);

        let child_mark = Mark::fresh(Mark::root());
        self.with_child(ScopeKind::Fn, child_mark, |folder| {
            e.function.visit_with(folder);
        });
    }

    fn visit_for_in_stmt(&mut self, n: &ForInStmt) {
        let child_mark = Mark::fresh(Mark::root());

        self.with_child(ScopeKind::Block, child_mark, |child| {
            n.left.visit_with(child);
            n.right.visit_with(child);

            child.visit_stmt_within_child_scope(&*n.body);
        });
    }

    fn visit_for_of_stmt(&mut self, n: &ForOfStmt) {
        let child_mark = Mark::fresh(Mark::root());

        self.with_child(ScopeKind::Block, child_mark, |child| {
            n.left.visit_with(child);
            n.right.visit_with(child);

            child.visit_stmt_within_child_scope(&*n.body);
        });
    }

    fn visit_for_stmt(&mut self, n: &ForStmt) {
        let child_mark = Mark::fresh(Mark::root());

        self.ident_type = Some(IdentType::Binding);
        self.with_child(ScopeKind::Block, child_mark, |child| {
            n.init.visit_with(child);
        });

        self.ident_type = Some(IdentType::Ref);
        self.with_child(ScopeKind::Block, child_mark, |child| {
            n.test.visit_with(child);
        });

        self.ident_type = Some(IdentType::Ref);
        self.with_child(ScopeKind::Block, child_mark, |child| {
            n.update.visit_with(child);
            child.visit_stmt_within_child_scope(&*n.body);
        });
    }

    fn visit_function(&mut self, f: &Function) {
        f.type_params.visit_with(self);

        self.ident_type = Some(IdentType::Ref);
        f.decorators.visit_with(self);

        self.ident_type = Some(IdentType::Binding);
        f.params.visit_with(self);

        f.return_type.visit_with(self);

        self.ident_type = Some(IdentType::Ref);
        match &f.body {
            Some(body) => {
                // Prevent creating new scope.
                body.visit_children_with(self);
            }
            None => {}
        }
    }

    fn visit_import_decl(&mut self, n: &ImportDecl) {
        // Always resolve the import declaration identifiers even if it's type only.
        // We need to analyze these identifiers for type stripping purposes.
        self.ident_type = Some(IdentType::Binding);
        self.in_type = n.type_only;
        n.visit_children_with(self);
    }

    fn visit_import_named_specifier(&mut self, s: &ImportNamedSpecifier) {
        let old = self.ident_type;
        self.ident_type = Some(IdentType::Binding);
        s.local.visit_with(self);
        self.ident_type = old;
    }

    fn visit_method_prop(&mut self, m: &MethodProp) {
        m.key.visit_with(self);

        {
            let child_mark = Mark::fresh(Mark::root());

            self.with_child(ScopeKind::Fn, child_mark, |child| {
                m.function.visit_with(child);
            });
        };
    }

    fn visit_object_lit(&mut self, object_expr: &ObjectLit) {
        let child_mark = Mark::fresh(Mark::root());

        let bindings = {
            let mut child = ClosureIdentVisitor::from(
                self,
                Scope::new(ScopeKind::Fn, child_mark, Some(&self.scope)),
            );

            let old_in_object_expression = child.is_in_object_expression;
            child.is_in_object_expression = true;

            for prop in &object_expr.props {
                match prop {
                    PropOrSpread::Prop(p) => {
                        let old_in_object_prop = child.is_in_object_prop;
                        child.is_in_object_prop = true;
                        let prop = &**p;

                        // TODO: incomplete
                        match prop {
                            Prop::Shorthand(ident) => {
                                child.parent_object_prop_ident = Some(ident.clone());
                                prop.visit_children_with(&mut child);
                            }
                            Prop::KeyValue(KeyValueProp { value, .. }) => {
                                value.visit_children_with(&mut child);
                            }
                            _ => {
                                prop.visit_children_with(&mut child);
                            }
                        }

                        child.is_in_object_prop = old_in_object_prop;
                        child.parent_object_prop_ident = None;
                    }
                    PropOrSpread::Spread(..) => {}
                };
            }

            child.is_in_object_expression = old_in_object_expression;

            child.scope.bindings
        };

        if !matches!(ScopeKind::Fn, ScopeKind::Fn { .. }) {
            let v = bindings;

            for (id, v) in v.into_iter().filter_map(|(id, v)| {
                if v.kind == VarType::Var(VarDeclKind::Var) {
                    Some((id, v))
                } else {
                    None
                }
            }) {
                let v: VarInfo = v;
                *v.value.borrow_mut() = None;
                self.scope.bindings.insert(id, v);
            }
        }
    }

    fn visit_param(&mut self, param: &Param) {
        self.ident_type = Some(IdentType::Binding);
        param.visit_children_with(self);
    }

    fn visit_assign_pat(&mut self, node: &AssignPat) {
        node.left.visit_with(self);
        node.right.visit_with(self);
    }

    fn visit_rest_pat(&mut self, node: &RestPat) {
        node.arg.visit_with(self);
    }

    fn visit_private_method(&mut self, m: &PrivateMethod) {
        m.key.visit_with(self);

        {
            let child_mark = Mark::fresh(Mark::root());

            self.with_child(ScopeKind::Fn, child_mark, |child| {
                m.function.visit_with(child);
            });
        }
    }

    fn visit_setter_prop(&mut self, n: &SetterProp) {
        n.key.visit_with(self);

        {
            let child_mark = Mark::fresh(Mark::root());

            self.with_child(ScopeKind::Fn, child_mark, |child| {
                child.ident_type = Some(IdentType::Binding);
                n.param.visit_with(child);
                n.body.visit_with(child);
            });
        };
    }

    fn visit_switch_stmt(&mut self, s: &SwitchStmt) {
        s.discriminant.visit_with(self);

        let child_mark = Mark::fresh(Mark::root());

        self.with_child(ScopeKind::Block, child_mark, |folder| {
            s.cases.visit_with(folder);
        });
    }

    fn visit_ident(&mut self, ident: &Ident) {
        if let Some(fn_name) = self.fn_name {
            if fn_name == ident {
                return;
            }
        }

        if self.globals.iter().any(|v| &*ident.sym == v) {
            return;
        }

        if self.is_parent_member_expr && !self.is_parent_member_expr_computed {
            if let Some(parent_prop_ident) = &self.parent_member_expr_prop_ident {
                if ident == parent_prop_ident {
                    return;
                }
            }
        }

        if self.is_in_object_expression && self.is_in_object_prop {
            if let Some(parent_prop_ident) = &self.parent_object_prop_ident {
                if ident != parent_prop_ident {
                    return;
                }
            }
        }

        /* TODO
        if (
            parentNode.type === 'ObjectProperty' &&
            // object_lit
            path.parentPath.parent.type === 'ObjectExpression' &&
            path.node !== parentNode.value
        ) {
            return;
        }
        */

        if let Some(ident_type) = self.ident_type {
            if ident_type == IdentType::Ref {
                self.scope.closure.insert(ident.to_id());

                let mut current_scope = Some(&self.scope);
                while let Some(scope) = current_scope {
                    if scope.bindings.contains_key(&ident.to_id()) {
                        return;
                    }

                    current_scope = scope.parent;
                }

                self.closure.push(ident.clone());
            }

            /*
            closure.set(name, path.node);
            closureGenerator.addPath(name, path);
            */
        }
    }

    fn visit_assign_expr(&mut self, assign_expr: &AssignExpr) {
        // test for <something>.value = <something> expressions
        let left = &assign_expr.left;
        if let PatOrExpr::Expr(expr) = left {
            if let Expr::Member(member_expr) = &**expr {
                if let Expr::Ident(ident) = &*member_expr.obj {
                    if let MemberProp::Ident(prop) = &member_expr.prop {
                        if &*prop.sym == "value" {
                            self.outputs.insert(ident.clone());
                        }
                    }
                }
            }
        }
    }

    fn visit_var_declarator(&mut self, decl: &VarDeclarator) {
        // order is important

        let old_type = self.ident_type;
        self.ident_type = Some(IdentType::Binding);
        decl.name.visit_with(self);
        self.ident_type = old_type;

        decl.init.visit_children_with(self);
    }
}

struct ReanimatedWorkletsVisitor<
    C: Clone + swc_common::comments::Comments,
    S: swc_common::SourceMapper + SourceMapperExt,
> {
    globals: Vec<String>,
    filename: FileName,
    in_use_animated_style: bool,
    source_map: std::sync::Arc<S>,
    relative_cwd: Option<PathBuf>,
    in_gesture_handler_event_callback: bool,
    comments: C,
}

impl<C: Clone + swc_common::comments::Comments, S: swc_common::SourceMapper + SourceMapperExt>
    ReanimatedWorkletsVisitor<C, S>
{
    pub fn new(
        source_map: std::sync::Arc<S>,
        globals: Vec<String>,
        filename: FileName,
        relative_cwd: Option<PathBuf>,
        comments: C,
    ) -> Self {
        ReanimatedWorkletsVisitor {
            source_map,
            globals,
            filename,
            relative_cwd,
            in_use_animated_style: false,
            in_gesture_handler_event_callback: false,
            comments,
        }
    }

    /// Print givne fn's string with writer.
    /// This should be called with `cloned` node, as internally this'll take ownership.
    fn build_worklet_string(&mut self, fn_name: Ident, expr: Expr) -> String {
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
                    minify: true,
                    ..Default::default()
                },
                comments: Default::default(),
                cm: self.source_map.clone(),
                wr,
            };

            transformed_function
                .emit_with(&mut emitter)
                .ok()
                .expect("Should emit");
        }
        String::from_utf8(buf).expect("invalid utf8 character detected")
    }

    /// Actual fn to generate AST for worklet-ized function to be called across
    /// fn-like nodes (arrow fn, fnExpr)
    fn make_worklet_inner(
        &mut self,
        worklet_name: Option<Ident>,
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
        let function_name = if let Some(ident) = &worklet_name {
            ident.clone()
        } else {
            Ident::new("_f".into(), DUMMY_SP)
        };
        let private_fn_name = Ident::new("_f".into(), DUMMY_SP);

        let opt_flags = if self.in_use_animated_style {
            let mut opt_find_visitor = OptimizationFinderVisitor::new();
            cloned.visit_with(&mut opt_find_visitor);

            Some(opt_find_visitor.calculate_flags())
        } else {
            None
        };

        // TODO: this mimics existing plugin behavior runs specific transform pass
        // before running actual visitor.
        // 1. This may not required
        // 2. If required, need to way to pass config to visitors instead of Default::default()
        // https://github.com/software-mansion/react-native-reanimated/blob/b4ee4ea9a1f246c461dd1819c6f3d48440a25756/plugin.js#L367-L371=
        let mut preprocessors: Vec<Box<dyn VisitMut>> = vec![
            Box::new(shorthand()),
            Box::new(arrow()),
            Box::new(optional_chaining(Default::default())),
            Box::new(nullish_coalescing(Default::default())),
            Box::new(template_literal(Default::default())),
        ];

        for mut preprocessor in preprocessors.drain(..) {
            cloned.visit_mut_with(&mut *preprocessor);
        }

        let mut closure_visitor = ClosureIdentVisitor::new(
            Scope::new(ScopeKind::Fn, Mark::new(), None),
            &self.globals,
            &worklet_name,
        );
        cloned.visit_children_with(&mut closure_visitor);

        let func_string = self.build_worklet_string(function_name.clone(), cloned);
        let func_hash = calculate_hash(&func_string);

        /*
            const outputs = new Set();
            const closureGenerator = new ClosureGenerator();
        */

        let closure_ident = Ident::new("_closure".into(), DUMMY_SP);
        let as_string_ident = Ident::new("asString".into(), DUMMY_SP);
        let worklet_hash_ident = Ident::new("__workletHash".into(), DUMMY_SP);
        let location_ident = Ident::new("__location".into(), DUMMY_SP);
        let optimalization_ident = Ident::new("__optimalization".into(), DUMMY_SP);

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
                ident: Some(private_fn_name.clone()),
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
            BlockStmtOrExpr::Expr(e) => {
                // This is based on assumption if fn body is not a blockstmt
                // we'll manually need to create returnstmt always.
                // TODO: need to validated further cases.

                let body = if let Expr::Paren(paren) = *e {
                    *paren.expr
                } else {
                    *e
                };

                Expr::Fn(FnExpr {
                    ident: Some(private_fn_name.clone()),
                    function: Function {
                        params,
                        decorators,
                        span: DUMMY_SP,
                        body: Some(BlockStmt {
                            stmts: vec![Stmt::Return(ReturnStmt {
                                span: DUMMY_SP,
                                arg: Some(Box::new(body)),
                            })],
                            ..BlockStmt::dummy()
                        }),
                        is_generator,
                        is_async,
                        type_params,
                        return_type,
                    },
                })
            }
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
                    name: Pat::Ident(BindingIdent::from(private_fn_name.clone())),
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
                        obj: Box::new(Expr::Ident(private_fn_name.clone())),
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
                        obj: Box::new(Expr::Ident(private_fn_name.clone())),
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
                        obj: Box::new(Expr::Ident(private_fn_name.clone())),
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
                        obj: Box::new(Expr::Ident(private_fn_name.clone())),
                        prop: MemberProp::Ident(location_ident.clone()),
                    }))),
                    right: Box::new(Expr::Lit(Lit::Str(Str::from(code_location)))),
                })),
            }),
        ];

        if let Some(opt_flags) = opt_flags {
            stmts.push(Stmt::Expr(ExprStmt {
                span: DUMMY_SP,
                expr: Box::new(Expr::Assign(AssignExpr {
                    span: DUMMY_SP,
                    op: AssignOp::Assign,
                    left: PatOrExpr::Expr(Box::new(Expr::Member(MemberExpr {
                        span: DUMMY_SP,
                        obj: Box::new(Expr::Ident(private_fn_name.clone())),
                        prop: MemberProp::Ident(optimalization_ident.clone()),
                    }))),
                    right: Box::new(Expr::Lit(Lit::Num(Number {
                        span: DUMMY_SP,
                        value: opt_flags.into(),
                        raw: None,
                    }))),
                })),
            }));
        }

        stmts.push(Stmt::Return(ReturnStmt {
            span: DUMMY_SP,
            arg: Some(Box::new(Expr::Ident(private_fn_name))),
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
            ident.clone(),
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
            None,
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

    fn process_if_fn_decl_worklet_node(&mut self, decl: &mut Decl) {
        let mut visitor = DirectiveFinderVisitor::new(self.comments.clone());
        decl.visit_mut_children_with(&mut visitor);
        if visitor.has_worklet_directive {
            self.process_worklet_fn_decl(decl);
        }
    }

    // TODO: consolidate with process_if_fn_decl_worklet_node
    fn process_if_worklet_node(&mut self, fn_like_expr: &mut Expr) {
        let mut visitor = DirectiveFinderVisitor::new(self.comments.clone());
        fn_like_expr.visit_mut_children_with(&mut visitor);
        if visitor.has_worklet_directive {
            self.process_worklet_function(fn_like_expr);
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
                    // TODO: handle rest of proname enum
                    let mut fn_ident = if let PropName::Ident(i) = &key {
                        Some(i.clone())
                    } else {
                        None
                    };

                    Some(self.make_worklet_from_fn(&mut fn_ident, function))
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(function) = function {
                *method_prop = PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                    key,
                    value: Box::new(Expr::Fn(FnExpr {
                        function,
                        ..FnExpr::dummy()
                    })),
                })));
            }
        }
    }

    fn process_worklet_fn_decl(&mut self, decl: &mut Decl) {
        if let Decl::Fn(fn_decl) = decl {
            let worklet_fn =
                self.make_worklet_from_fn(&mut Some(fn_decl.ident.clone()), &mut fn_decl.function);

            let declarator = VarDeclarator {
                name: Pat::Ident(BindingIdent::from(fn_decl.ident.take())),
                init: Some(Box::new(Expr::Call(CallExpr {
                    callee: Callee::Expr(Box::new(Expr::Fn(FnExpr {
                        ident: None,
                        function: worklet_fn,
                    }))),
                    ..CallExpr::dummy()
                }))),
                ..VarDeclarator::dummy()
            };

            *decl = Decl::Var(VarDecl {
                kind: VarDeclKind::Const,
                decls: vec![declarator],
                ..VarDecl::dummy()
            });
        }
    }

    // TODO: consolidate with process_worklet_fn_decl
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
        let old = self.in_use_animated_style;
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
                }
                self.in_use_animated_style = false;
            }
            Some(name) => {
                if &*name.sym == "useAnimatedStyle" {
                    self.in_use_animated_style = true;
                }

                let indexes = FUNCTION_ARGS_TO_WORKLETIZE.get(&*name.sym);

                if let Some(indexes) = indexes {
                    indexes.iter().for_each(|idx| {
                        let arg = call_expr.args.get_mut(*idx);

                        if let Some(arg) = arg {
                            self.process_worklet_function(&mut *arg.expr);
                        }
                    });
                }

                self.in_use_animated_style = old;
            }
            _ => {}
        }
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

impl<C: Clone + swc_common::comments::Comments, S: swc_common::SourceMapper + SourceMapperExt>
    VisitMut for ReanimatedWorkletsVisitor<C, S>
{
    fn visit_mut_call_expr(&mut self, call_expr: &mut CallExpr) {
        if is_gesture_object_event_callback_method(&call_expr.callee) {
            let old = self.in_gesture_handler_event_callback;
            self.in_gesture_handler_event_callback =
                is_gesture_object_event_callback_method(&mut call_expr.callee);
            call_expr.visit_mut_children_with(self);
            self.in_gesture_handler_event_callback = old;
        } else {
            self.process_worklets(call_expr);
            call_expr.visit_mut_children_with(self);
        }
    }

    fn visit_mut_decl(&mut self, decl: &mut Decl) {
        decl.visit_mut_children_with(self);

        match decl {
            Decl::Fn(..) => {
                self.process_if_fn_decl_worklet_node(decl);
                if self.in_gesture_handler_event_callback {
                    self.process_worklet_fn_decl(decl);
                }
            }
            _ => {}
        }
    }

    // Note we do not transform class method itself - it should be performed by core transform instead
    fn visit_mut_class_method(&mut self, class_method: &mut ClassMethod) {
        match &mut class_method.key {
            PropName::Ident(ident) => {
                let mut visitor = DirectiveFinderVisitor::new(self.comments.clone());
                class_method.function.visit_mut_children_with(&mut visitor);

                // TODO: consolidate with process_if_fn_decl_worklet_node
                if visitor.has_worklet_directive {
                    let worklet_fn = self
                        .make_worklet_from_fn(&mut Some(ident.clone()), &mut class_method.function);
                    class_method.function = worklet_fn;
                }
            }
            _ => {}
        }
    }

    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        expr.visit_mut_children_with(self);

        match expr {
            Expr::Arrow(..) | Expr::Fn(..) => {
                self.process_if_worklet_node(expr);
                if self.in_gesture_handler_event_callback {
                    self.process_worklet_function(expr);
                }
            }
            _ => {}
        }
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

pub fn create_worklets_visitor<
    C: Clone + swc_common::comments::Comments,
    S: swc_common::SourceMapper + SourceMapperExt,
>(
    worklets_options: WorkletsOptions,
    source_map: std::sync::Arc<S>,
    comments: C,
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
        comments,
    )
}
