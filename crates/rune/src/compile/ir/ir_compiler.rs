use crate::ast;
use crate::ast::Spanned;
use crate::compile::ir;
use crate::compile::{IrError, IrEval, IrValue};
use crate::parse::Resolve;
use crate::query::{BuiltInMacro, BuiltInTemplate, Query};
use crate::runtime::{Bytes, Shared};

/// A c that compiles AST into Rune IR.
pub struct IrCompiler<'a> {
    pub(crate) q: Query<'a>,
}

impl IrCompiler<'_> {
    /// Compile the given target.
    pub(crate) fn compile<T>(&mut self, target: &T) -> Result<T::Output, IrError>
    where
        T: IrCompile,
    {
        target.compile(self)
    }

    /// Resolve the given resolvable value.
    pub(crate) fn resolve<'s, T>(&'s self, value: &T) -> Result<T::Output, IrError>
    where
        T: Resolve<'s>,
    {
        Ok(value.resolve(self.q.storage(), self.q.sources)?)
    }

    /// Resolve an ir target from an expression.
    fn ir_target(&self, expr: &ast::Expr) -> Result<ir::IrTarget, IrError> {
        match expr {
            ast::Expr::Path(path) => {
                if let Some(ident) = path.try_as_ident() {
                    let name = self.resolve(ident)?;

                    return Ok(ir::IrTarget {
                        span: expr.span(),
                        kind: ir::IrTargetKind::Name(name.into()),
                    });
                }
            }
            ast::Expr::FieldAccess(expr_field_access) => {
                let target = self.ir_target(&expr_field_access.expr)?;

                match &expr_field_access.expr_field {
                    ast::ExprField::Path(field) => {
                        let field = self.resolve(field)?;

                        return Ok(ir::IrTarget {
                            span: expr.span(),
                            kind: ir::IrTargetKind::Field(Box::new(target), field),
                        });
                    }
                    ast::ExprField::LitNumber(number) => {
                        let number = self.resolve(number)?;

                        if let Some(index) = number.as_tuple_index() {
                            return Ok(ir::IrTarget {
                                span: expr.span(),
                                kind: ir::IrTargetKind::Index(Box::new(target), index),
                            });
                        }
                    }
                }
            }
            _ => (),
        }

        Err(IrError::msg(expr, "not supported as a target"))
    }
}

/// The trait for a type that can be compiled into intermediate representation.
pub trait IrCompile {
    type Output: IrEval;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError>;
}

impl IrCompile for ast::Expr {
    type Output = ir::Ir;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        Ok(match self {
            ast::Expr::Vec(expr_vec) => ir::Ir::new(expr_vec.span(), expr_vec.compile(c)?),
            ast::Expr::Tuple(expr_tuple) => expr_tuple.compile(c)?,
            ast::Expr::Object(expr_object) => {
                ir::Ir::new(expr_object.span(), expr_object.compile(c)?)
            }
            ast::Expr::Group(expr_group) => expr_group.expr.compile(c)?,
            ast::Expr::Empty(expr_empty) => expr_empty.expr.compile(c)?,
            ast::Expr::Binary(expr_binary) => expr_binary.compile(c)?,
            ast::Expr::Assign(expr_assign) => expr_assign.compile(c)?,
            ast::Expr::Call(expr_call) => ir::Ir::new(self.span(), expr_call.compile(c)?),
            ast::Expr::If(expr_if) => ir::Ir::new(self.span(), expr_if.compile(c)?),
            ast::Expr::Loop(expr_loop) => ir::Ir::new(self.span(), expr_loop.compile(c)?),
            ast::Expr::While(expr_while) => ir::Ir::new(self.span(), expr_while.compile(c)?),
            ast::Expr::Lit(expr_lit) => expr_lit.compile(c)?,
            ast::Expr::Block(expr_block) => expr_block.compile(c)?,
            ast::Expr::Path(path) => path.compile(c)?,
            ast::Expr::FieldAccess(..) => ir::Ir::new(self.span(), c.ir_target(self)?),
            ast::Expr::Break(expr_break) => {
                ir::Ir::new(expr_break, ir::IrBreak::compile_ast(expr_break, c)?)
            }
            ast::Expr::Let(expr_let) => ir::Ir::new(expr_let, expr_let.compile(c)?),
            ast::Expr::MacroCall(macro_call) => {
                let internal_macro = c.q.builtin_macro_for(&**macro_call)?;

                match &*internal_macro {
                    BuiltInMacro::Template(template) => {
                        let ir_template = template.compile(c)?;
                        ir::Ir::new(self.span(), ir_template)
                    }
                    BuiltInMacro::File(file) => {
                        let s = c.resolve(&file.value)?;
                        ir::Ir::new(file.span, IrValue::String(Shared::new(s.into_owned())))
                    }
                    BuiltInMacro::Line(line) => {
                        let n = c.resolve(&line.value)?;

                        let const_value = match n {
                            ast::Number::Integer(n) => IrValue::Integer(n),
                            ast::Number::Float(n) => IrValue::Float(n),
                        };

                        ir::Ir::new(line.span, const_value)
                    }
                    _ => {
                        return Err(IrError::msg(self, "unsupported builtin macro"));
                    }
                }
            }
            _ => return Err(IrError::msg(self, "not supported yet")),
        })
    }
}

impl IrCompile for ast::ExprAssign {
    type Output = ir::Ir;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        let span = self.span();
        let target = c.ir_target(&self.lhs)?;

        Ok(ir::Ir::new(
            span,
            ir::IrSet {
                span,
                target,
                value: Box::new(self.rhs.compile(c)?),
            },
        ))
    }
}

impl IrCompile for ast::ExprCall {
    type Output = ir::IrCall;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        let span = self.span();

        let mut args = Vec::new();

        for (expr, _) in &self.args {
            args.push(expr.compile(c)?);
        }

        if let ast::Expr::Path(path) = &self.expr {
            if let Some(ident) = path.try_as_ident() {
                let target = c.resolve(ident)?;

                return Ok(ir::IrCall {
                    span,
                    target: target.into(),
                    args,
                });
            }
        }

        Err(IrError::msg(span, "call not supported"))
    }
}

impl IrCompile for ast::ExprBinary {
    type Output = ir::Ir;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        let span = self.span();

        if self.op.is_assign() {
            let op = match &self.op {
                ast::BinOp::AddAssign(..) => ir::IrAssignOp::Add,
                ast::BinOp::SubAssign(..) => ir::IrAssignOp::Sub,
                ast::BinOp::MulAssign(..) => ir::IrAssignOp::Mul,
                ast::BinOp::DivAssign(..) => ir::IrAssignOp::Div,
                ast::BinOp::ShlAssign(..) => ir::IrAssignOp::Shl,
                ast::BinOp::ShrAssign(..) => ir::IrAssignOp::Shr,
                _ => return Err(IrError::msg(&self.op, "op not supported yet")),
            };

            let target = c.ir_target(&self.lhs)?;

            return Ok(ir::Ir::new(
                span,
                ir::IrAssign {
                    span,
                    target,
                    value: Box::new(self.rhs.compile(c)?),
                    op,
                },
            ));
        }

        let lhs = self.lhs.compile(c)?;
        let rhs = self.rhs.compile(c)?;

        let op = match &self.op {
            ast::BinOp::Add(..) => ir::IrBinaryOp::Add,
            ast::BinOp::Sub(..) => ir::IrBinaryOp::Sub,
            ast::BinOp::Mul(..) => ir::IrBinaryOp::Mul,
            ast::BinOp::Div(..) => ir::IrBinaryOp::Div,
            ast::BinOp::Shl(..) => ir::IrBinaryOp::Shl,
            ast::BinOp::Shr(..) => ir::IrBinaryOp::Shr,
            ast::BinOp::Lt(..) => ir::IrBinaryOp::Lt,
            ast::BinOp::Lte(..) => ir::IrBinaryOp::Lte,
            ast::BinOp::Eq(..) => ir::IrBinaryOp::Eq,
            ast::BinOp::Gt(..) => ir::IrBinaryOp::Gt,
            ast::BinOp::Gte(..) => ir::IrBinaryOp::Gte,
            _ => return Err(IrError::msg(&self.op, "op not supported yet")),
        };

        Ok(ir::Ir::new(
            self.span(),
            ir::IrBinary {
                span,
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
        ))
    }
}

impl IrCompile for ast::ExprLit {
    type Output = ir::Ir;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        let span = self.span();

        Ok(match &self.lit {
            ast::Lit::Bool(b) => ir::Ir::new(span, IrValue::Bool(b.value)),
            ast::Lit::Str(s) => {
                let s = c.resolve(s)?;
                ir::Ir::new(span, IrValue::String(Shared::new(s.into_owned())))
            }
            ast::Lit::Number(n) => {
                let n = c.resolve(n)?;

                let const_value = match n {
                    ast::Number::Integer(n) => IrValue::Integer(n),
                    ast::Number::Float(n) => IrValue::Float(n),
                };

                ir::Ir::new(span, const_value)
            }
            ast::Lit::Byte(lit_byte) => ir::Ir::new(span, lit_byte.compile(c)?),
            ast::Lit::ByteStr(lit_byte_str) => ir::Ir::new(span, lit_byte_str.compile(c)?),
            ast::Lit::Char(lit_char) => ir::Ir::new(span, lit_char.compile(c)?),
        })
    }
}

impl IrCompile for ast::ExprTuple {
    type Output = ir::Ir;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        let span = self.span();

        if self.items.is_empty() {
            return Ok(ir::Ir::new(span, IrValue::Unit));
        }

        let mut items = Vec::new();

        for (expr, _) in &self.items {
            items.push(expr.compile(c)?);
        }

        Ok(ir::Ir::new(
            span,
            ir::IrTuple {
                span: self.span(),
                items: items.into_boxed_slice(),
            },
        ))
    }
}

impl IrCompile for ast::ExprVec {
    type Output = ir::IrVec;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        let mut items = Vec::new();

        for (expr, _) in &self.items {
            items.push(expr.compile(c)?);
        }

        Ok(ir::IrVec {
            span: self.span(),
            items: items.into_boxed_slice(),
        })
    }
}

impl IrCompile for ast::ExprObject {
    type Output = ir::IrObject;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        let mut assignments = Vec::new();

        for (assign, _) in &self.assignments {
            let key = c.resolve(&assign.key)?.into_owned().into_boxed_str();

            let ir = if let Some((_, expr)) = &assign.assign {
                expr.compile(c)?
            } else {
                ir::Ir::new(
                    assign,
                    ir::IrKind::Target(ir::IrTarget {
                        span: assign.span(),
                        kind: ir::IrTargetKind::Name(key.clone()),
                    }),
                )
            };

            assignments.push((key, ir))
        }

        Ok(ir::IrObject {
            span: self.span(),
            assignments: assignments.into_boxed_slice(),
        })
    }
}

impl IrCompile for ast::LitByteStr {
    type Output = IrValue;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        let byte_str = c.resolve(self)?;
        Ok(IrValue::Bytes(Shared::new(Bytes::from_vec(
            byte_str.into_owned(),
        ))))
    }
}

impl IrCompile for ast::LitByte {
    type Output = IrValue;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        let b = c.resolve(self)?;
        Ok(IrValue::Byte(b))
    }
}

impl IrCompile for ast::LitChar {
    type Output = IrValue;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        let c = c.resolve(self)?;
        Ok(IrValue::Char(c))
    }
}

impl IrCompile for ast::ExprBlock {
    type Output = ir::Ir;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        Ok(ir::Ir::new(self.span(), self.block.compile(c)?))
    }
}

impl IrCompile for ast::Block {
    type Output = ir::IrScope;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        let span = self.span();

        let mut last = None::<(&ast::Expr, bool)>;
        let mut instructions = Vec::new();

        for stmt in &self.statements {
            let (expr, term) = match stmt {
                ast::Stmt::Local(local) => {
                    if let Some((expr, _)) = std::mem::take(&mut last) {
                        instructions.push(expr.compile(c)?);
                    }

                    instructions.push(local.compile(c)?);
                    continue;
                }
                ast::Stmt::Expr(expr, semi) => (expr, semi.is_some()),
                ast::Stmt::Item(..) => continue,
            };

            if let Some((expr, _)) = std::mem::replace(&mut last, Some((expr, term))) {
                instructions.push(expr.compile(c)?);
            }
        }

        let last = if let Some((expr, term)) = last {
            if term {
                instructions.push(expr.compile(c)?);
                None
            } else {
                Some(Box::new(expr.compile(c)?))
            }
        } else {
            None
        };

        Ok(ir::IrScope {
            span,
            instructions,
            last,
        })
    }
}

impl IrCompile for BuiltInTemplate {
    type Output = ir::IrTemplate;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        let span = self.span;
        let mut components = Vec::new();

        for expr in &self.exprs {
            if let ast::Expr::Lit(expr_lit) = expr {
                if let ast::ExprLit {
                    lit: ast::Lit::Str(s),
                    ..
                } = &**expr_lit
                {
                    let s = s.resolve_template_string(&c.q.storage(), c.q.sources)?;

                    components.push(ir::IrTemplateComponent::String(
                        s.into_owned().into_boxed_str(),
                    ));

                    continue;
                }
            }

            let ir = expr.compile(c)?;
            components.push(ir::IrTemplateComponent::Ir(ir));
        }

        Ok(ir::IrTemplate { span, components })
    }
}

impl IrCompile for ast::Path {
    type Output = ir::Ir;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        let span = self.span();

        if let Some(name) = self.try_as_ident() {
            let name = c.resolve(name)?;
            return Ok(ir::Ir::new(span, <Box<str>>::from(name)));
        }

        Err(IrError::msg(span, "not supported yet"))
    }
}

impl IrCompile for ast::ExprLet {
    type Output = ir::IrDecl;

    fn compile(&self, _: &mut IrCompiler) -> Result<Self::Output, IrError> {
        Err(IrError::msg(self, "not supported yet"))
    }
}

impl IrCompile for ast::Local {
    type Output = ir::Ir;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        let span = self.span();

        let name = loop {
            match &self.pat {
                ast::Pat::PatIgnore(_) => {
                    return self.expr.compile(c);
                }
                ast::Pat::PatPath(path) => {
                    if let Some(ident) = path.path.try_as_ident() {
                        break ident;
                    }
                }
                _ => (),
            }

            return Err(IrError::msg(span, "not supported yet"));
        };

        Ok(ir::Ir::new(
            span,
            ir::IrDecl {
                span,
                name: c.resolve(name)?.into(),
                value: Box::new(self.expr.compile(c)?),
            },
        ))
    }
}

impl IrCompile for ast::Condition {
    type Output = ir::IrCondition;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        match self {
            ast::Condition::Expr(expr) => Ok(ir::IrCondition::Ir(expr.compile(c)?)),
            ast::Condition::ExprLet(expr_let) => {
                let pat = ir::IrPat::compile_ast(&expr_let.pat, c)?;
                let ir = expr_let.expr.compile(c)?;

                Ok(ir::IrCondition::Let(ir::IrLet {
                    span: expr_let.span(),
                    pat,
                    ir,
                }))
            }
        }
    }
}

impl IrCompile for ast::ExprIf {
    type Output = ir::IrBranches;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        let mut branches = Vec::new();
        let mut default_branch = None;

        let condition = self.condition.compile(c)?;
        let ir = self.block.compile(c)?;
        branches.push((condition, ir));

        for expr_else_if in &self.expr_else_ifs {
            let condition = expr_else_if.condition.compile(c)?;
            let ir = expr_else_if.block.compile(c)?;
            branches.push((condition, ir));
        }

        if let Some(expr_else) = &self.expr_else {
            let ir = expr_else.block.compile(c)?;
            default_branch = Some(ir);
        }

        Ok(ir::IrBranches {
            branches,
            default_branch,
        })
    }
}

impl IrCompile for ast::ExprWhile {
    type Output = ir::IrLoop;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        Ok(ir::IrLoop {
            span: self.span(),
            label: match &self.label {
                Some((label, _)) => Some(c.resolve(label)?.into()),
                None => None,
            },
            condition: Some(Box::new(self.condition.compile(c)?)),
            body: self.body.compile(c)?,
        })
    }
}

impl IrCompile for ast::ExprLoop {
    type Output = ir::IrLoop;

    fn compile(&self, c: &mut IrCompiler<'_>) -> Result<Self::Output, IrError> {
        Ok(ir::IrLoop {
            span: self.span(),
            label: match &self.label {
                Some((label, _)) => Some(c.resolve(label)?.into()),
                None => None,
            },
            condition: None,
            body: self.body.compile(c)?,
        })
    }
}